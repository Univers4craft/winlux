//! Capture vidéo Video4Linux2 (webcams) pour Media Foundation : liste des
//! caméras et de leurs formats, flux en mémoire partagée (`mmap`) et
//! conversions YUYV/NV12 vers RGB32.

use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*s)
}
pub const YUYV: u32 = fourcc(b"YUYV");
pub const MJPG: u32 = fourcc(b"MJPG");
pub const NV12: u32 = fourcc(b"NV12");

// ioctl et tailles vérifiées avec linux/videodev2.h (x86_64).
const VIDIOC_QUERYCAP: u64 = 0x8068_5600;
const VIDIOC_ENUM_FMT: u64 = 0xC040_5602;
const VIDIOC_S_FMT: u64 = 0xC0D0_5605;
const VIDIOC_REQBUFS: u64 = 0xC014_5608;
const VIDIOC_QUERYBUF: u64 = 0xC058_5609;
const VIDIOC_QBUF: u64 = 0xC058_560F;
const VIDIOC_DQBUF: u64 = 0xC058_5611;
const VIDIOC_STREAMON: u64 = 0x4004_5612;
const VIDIOC_STREAMOFF: u64 = 0x4004_5613;
const VIDIOC_ENUM_FRAMESIZES: u64 = 0xC02C_564A;
const VIDIOC_ENUM_FRAMEINTERVALS: u64 = 0xC034_564B;
const VIDIOC_S_PARM: u64 = 0xC0CC_5616;

const CAP_VIDEO_CAPTURE: u32 = 0x1;
const CAP_STREAMING: u32 = 0x0400_0000;
const CAP_DEVICE_CAPS: u32 = 0x8000_0000;
const BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const MEMORY_MMAP: u32 = 1;

/// Un mode de capture : format, définition, cadence (num/den images/s).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub fourcc: u32,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
}

impl Mode {
    /// Taille d'une image dans le format natif (MJPG : borne supérieure).
    pub fn frame_bytes(&self) -> usize {
        let (w, h) = (self.width as usize, self.height as usize);
        match self.fourcc {
            NV12 => w * h * 3 / 2,
            MJPG => w * h * 2,
            _ => w * h * 2,
        }
    }
}

/// Une caméra détectée.
#[derive(Clone, Debug)]
pub struct Camera {
    pub path: PathBuf,
    pub name: String,
    pub modes: Vec<Mode>,
}

fn ioctl<T>(fd: i32, req: u64, arg: &mut T) -> bool {
    loop {
        let r = unsafe { libc::ioctl(fd, req as _, arg as *mut T) };
        if r >= 0 {
            return true;
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return false;
        }
    }
}

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_ne_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn wr32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_ne_bytes());
}
fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

fn open(path: &Path) -> Option<OwnedFd> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .ok()
        .map(Into::into)
}

/// Dossier des périphériques vidéo ; `WINLUX_VIDEO_DIR` pour les tests.
fn video_dir() -> PathBuf {
    std::env::var_os("WINLUX_VIDEO_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev"))
}

/// Définitions proposées quand la caméra accepte une plage continue.
const STEP_SIZES: [(u32, u32); 5] = [
    (320, 240),
    (640, 480),
    (1280, 720),
    (1920, 1080),
    (800, 600),
];

fn frame_sizes(fd: i32, pixfmt: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for index in 0..64 {
        let mut b = [0u8; 44];
        wr32(&mut b, 0, index);
        wr32(&mut b, 4, pixfmt);
        if !ioctl(fd, VIDIOC_ENUM_FRAMESIZES, &mut b) {
            break;
        }
        match rd32(&b, 8) {
            1 => out.push((rd32(&b, 12), rd32(&b, 16))),
            _ => {
                let (min_w, max_w, min_h, max_h) =
                    (rd32(&b, 12), rd32(&b, 16), rd32(&b, 24), rd32(&b, 28));
                for (w, h) in STEP_SIZES {
                    if (min_w..=max_w).contains(&w) && (min_h..=max_h).contains(&h) {
                        out.push((w, h));
                    }
                }
                break;
            }
        }
    }
    out
}

/// Cadence la plus rapide proposée pour cette définition (30 i/s par défaut).
fn best_rate(fd: i32, pixfmt: u32, w: u32, h: u32) -> (u32, u32) {
    let mut best = (30, 1);
    let mut best_fps = 0.0f64;
    for index in 0..32 {
        let mut b = [0u8; 52];
        wr32(&mut b, 0, index);
        wr32(&mut b, 4, pixfmt);
        wr32(&mut b, 8, w);
        wr32(&mut b, 12, h);
        if !ioctl(fd, VIDIOC_ENUM_FRAMEINTERVALS, &mut b) {
            break;
        }
        // Intervalle = num/den secondes ; on garde le minimum (stepwise : min).
        let (num, den) = (rd32(&b, 20), rd32(&b, 24));
        if num > 0 && den > 0 {
            let fps = den as f64 / num as f64;
            if fps > best_fps && fps <= 120.0 {
                best_fps = fps;
                best = (den, num);
            }
        }
        if rd32(&b, 16) != 1 {
            break;
        }
    }
    best
}

fn modes(fd: i32) -> Vec<Mode> {
    let mut out = Vec::new();
    for index in 0..32 {
        let mut b = [0u8; 64];
        wr32(&mut b, 0, index);
        wr32(&mut b, 4, BUF_TYPE_VIDEO_CAPTURE);
        if !ioctl(fd, VIDIOC_ENUM_FMT, &mut b) {
            break;
        }
        let pixfmt = rd32(&b, 44);
        if ![YUYV, MJPG, NV12].contains(&pixfmt) {
            continue;
        }
        for (w, h) in frame_sizes(fd, pixfmt) {
            let (fps_num, fps_den) = best_rate(fd, pixfmt, w, h);
            out.push(Mode {
                fourcc: pixfmt,
                width: w,
                height: h,
                fps_num,
                fps_den,
            });
        }
    }
    // Formats non compressés d'abord, puis par définition croissante.
    out.sort_by_key(|m| (m.fourcc == MJPG, m.width * m.height));
    out.dedup();
    out
}

/// Interroge une caméra ; `None` si ce n'est pas un nœud de capture.
pub fn probe(path: &Path) -> Option<Camera> {
    let fd = open(path)?;
    let raw = fd.as_raw_fd();
    let mut cap = [0u8; 104];
    if !ioctl(raw, VIDIOC_QUERYCAP, &mut cap) {
        return None;
    }
    let mut caps = rd32(&cap, 84);
    if caps & CAP_DEVICE_CAPS != 0 {
        caps = rd32(&cap, 88);
    }
    if caps & CAP_VIDEO_CAPTURE == 0 || caps & CAP_STREAMING == 0 {
        return None;
    }
    let modes = modes(raw);
    if modes.is_empty() {
        return None;
    }
    Some(Camera {
        path: path.to_path_buf(),
        name: cstr(&cap[16..48]),
        modes,
    })
}

/// Autorisation « Caméra » de la page Confidentialité des Paramètres
/// (`/etc/winlux/privacy.conf`, `CAMERA=deny` bloque l'accès des .exe).
pub fn camera_allowed() -> bool {
    let path = std::env::var_os("WINLUX_PRIVACY_CONF")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/winlux/privacy.conf"));
    std::fs::read_to_string(path).map_or(true, |conf| {
        !conf.lines().any(|l| l.trim() == "CAMERA=deny")
    })
}

/// Caméras branchées, triées par numéro de nœud (aucune si l'utilisateur
/// a bloqué la caméra).
pub fn cameras() -> Vec<Camera> {
    if !camera_allowed() {
        return Vec::new();
    }
    let mut nodes: Vec<(u32, PathBuf)> = std::fs::read_dir(video_dir())
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n: u32 = e
                        .file_name()
                        .to_str()?
                        .strip_prefix("video")?
                        .parse()
                        .ok()?;
                    Some((n, e.path()))
                })
                .collect()
        })
        .unwrap_or_default();
    nodes.sort();
    nodes.into_iter().filter_map(|(_, p)| probe(&p)).collect()
}

struct Mapped {
    ptr: *mut libc::c_void,
    len: usize,
}

/// Flux de capture ouvert sur un mode donné.
pub struct Stream {
    fd: OwnedFd,
    buffers: Vec<Mapped>,
    pub mode: Mode,
    pub stride: u32,
    started: Instant,
}

unsafe impl Send for Stream {}

impl Stream {
    pub fn open(path: &Path, mode: Mode) -> Option<Stream> {
        if !camera_allowed() {
            return None;
        }
        let fd = open(path)?;
        let raw = fd.as_raw_fd();
        let mut f = [0u8; 208];
        wr32(&mut f, 0, BUF_TYPE_VIDEO_CAPTURE);
        wr32(&mut f, 8, mode.width);
        wr32(&mut f, 12, mode.height);
        wr32(&mut f, 16, mode.fourcc);
        wr32(&mut f, 20, 1); // V4L2_FIELD_NONE
        if !ioctl(raw, VIDIOC_S_FMT, &mut f) {
            return None;
        }
        let mode = Mode {
            width: rd32(&f, 8),
            height: rd32(&f, 12),
            fourcc: rd32(&f, 16),
            ..mode
        };
        let stride = rd32(&f, 24);
        // Cadence : v4l2_streamparm.parm.capture.timeperframe.
        let mut p = [0u8; 204];
        wr32(&mut p, 0, BUF_TYPE_VIDEO_CAPTURE);
        wr32(&mut p, 12, mode.fps_den.max(1));
        wr32(&mut p, 16, mode.fps_num.max(1));
        ioctl(raw, VIDIOC_S_PARM, &mut p);
        let mut req = [0u8; 20];
        wr32(&mut req, 0, 4);
        wr32(&mut req, 4, BUF_TYPE_VIDEO_CAPTURE);
        wr32(&mut req, 8, MEMORY_MMAP);
        if !ioctl(raw, VIDIOC_REQBUFS, &mut req) || rd32(&req, 0) == 0 {
            return None;
        }
        let mut s = Stream {
            fd,
            buffers: Vec::new(),
            mode,
            stride,
            started: Instant::now(),
        };
        for i in 0..rd32(&req, 0) {
            let mut b = Self::buf(i);
            if !ioctl(raw, VIDIOC_QUERYBUF, &mut b) {
                return None;
            }
            let (offset, len) = (rd32(&b, 64), rd32(&b, 72) as usize);
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    raw,
                    offset as libc::off_t,
                )
            };
            if ptr == libc::MAP_FAILED {
                return None;
            }
            s.buffers.push(Mapped { ptr, len });
            if !ioctl(raw, VIDIOC_QBUF, &mut b) {
                return None;
            }
        }
        let mut ty = BUF_TYPE_VIDEO_CAPTURE as i32;
        if !ioctl(raw, VIDIOC_STREAMON, &mut ty) {
            return None;
        }
        s.started = Instant::now();
        Some(s)
    }

    fn buf(index: u32) -> [u8; 88] {
        let mut b = [0u8; 88];
        wr32(&mut b, 0, index);
        wr32(&mut b, 4, BUF_TYPE_VIDEO_CAPTURE);
        wr32(&mut b, 60, MEMORY_MMAP);
        b
    }

    /// Attend une image (au plus `timeout`) ; renvoie ses octets et son
    /// horodatage en unités de 100 ns depuis le début de la capture.
    pub fn read_frame(&mut self, timeout: Duration) -> Option<(Vec<u8>, i64)> {
        let raw = self.fd.as_raw_fd();
        let mut pfd = libc::pollfd {
            fd: raw,
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        if unsafe { libc::poll(&mut pfd, 1, ms) } <= 0 {
            return None;
        }
        let mut b = Self::buf(0);
        if !ioctl(raw, VIDIOC_DQBUF, &mut b) {
            return None;
        }
        let index = rd32(&b, 0) as usize;
        let used = rd32(&b, 8) as usize;
        let m = self.buffers.get(index)?;
        let data =
            unsafe { std::slice::from_raw_parts(m.ptr as *const u8, used.min(m.len)) }.to_vec();
        ioctl(raw, VIDIOC_QBUF, &mut b);
        let ts = (self.started.elapsed().as_nanos() / 100) as i64;
        Some((data, ts))
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let mut ty = BUF_TYPE_VIDEO_CAPTURE as i32;
        ioctl(self.fd.as_raw_fd(), VIDIOC_STREAMOFF, &mut ty);
        for m in &self.buffers {
            unsafe { libc::munmap(m.ptr, m.len) };
        }
    }
}

// ---------- Conversions de pixels ----------

fn clamp8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// BT.601 limité : (Y, U, V) vers (B, G, R).
fn yuv_to_bgr(y: u8, u: u8, v: u8) -> [u8; 3] {
    let c = (y as i32 - 16).max(0) * 298;
    let d = u as i32 - 128;
    let e = v as i32 - 128;
    [
        clamp8((c + 516 * d + 128) >> 8),
        clamp8((c - 100 * d - 208 * e + 128) >> 8),
        clamp8((c + 409 * e + 128) >> 8),
    ]
}

/// YUYV (YUY2) → RGB32 (B, G, R, 0xFF), de haut en bas.
pub fn yuyv_to_rgb32(src: &[u8], w: usize, h: usize, stride: usize, out: &mut Vec<u8>) {
    out.clear();
    out.resize(w * h * 4, 0);
    for row in 0..h {
        let line = &src[(row * stride).min(src.len())..];
        for x in (0..w).step_by(2) {
            let o = x * 2;
            if o + 3 >= line.len() {
                break;
            }
            let (y0, u, y1, v) = (line[o], line[o + 1], line[o + 2], line[o + 3]);
            for (k, yy) in [y0, y1].into_iter().enumerate() {
                if x + k < w {
                    let p = (row * w + x + k) * 4;
                    out[p..p + 3].copy_from_slice(&yuv_to_bgr(yy, u, v));
                    out[p + 3] = 0xFF;
                }
            }
        }
    }
}

/// NV12 → RGB32.
pub fn nv12_to_rgb32(src: &[u8], w: usize, h: usize, stride: usize, out: &mut Vec<u8>) {
    out.clear();
    out.resize(w * h * 4, 0);
    let uv_base = stride * h;
    for row in 0..h {
        for x in 0..w {
            let y = src.get(row * stride + x).copied().unwrap_or(16);
            let uv = uv_base + (row / 2) * stride + (x & !1);
            let u = src.get(uv).copied().unwrap_or(128);
            let v = src.get(uv + 1).copied().unwrap_or(128);
            let p = (row * w + x) * 4;
            out[p..p + 3].copy_from_slice(&yuv_to_bgr(y, u, v));
            out[p + 3] = 0xFF;
        }
    }
}

/// YUYV → NV12 (luminance complète, chrominance sous-échantillonnée).
pub fn yuyv_to_nv12(src: &[u8], w: usize, h: usize, stride: usize, out: &mut Vec<u8>) {
    out.clear();
    out.resize(w * h * 3 / 2, 128);
    for row in 0..h {
        for x in 0..w {
            out[row * w + x] = src.get(row * stride + x * 2).copied().unwrap_or(16);
        }
    }
    for row in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let o = row * stride + x * 2;
            let uv = w * h + (row / 2) * w + x;
            if uv + 1 < out.len() {
                out[uv] = src.get(o + 1).copied().unwrap_or(128);
                out[uv + 1] = src.get(o + 3).copied().unwrap_or(128);
            }
        }
    }
}

/// Copie compacte d'une image native (retire le remplissage de ligne).
pub fn pack(src: &[u8], mode: &Mode, stride: usize) -> Vec<u8> {
    let w = mode.width as usize;
    let h = mode.height as usize;
    let row = match mode.fourcc {
        YUYV => w * 2,
        NV12 => w,
        _ => return src.to_vec(),
    };
    if stride == row || stride == 0 {
        return src.to_vec();
    }
    let rows = if mode.fourcc == NV12 { h * 3 / 2 } else { h };
    let mut out = Vec::with_capacity(row * rows);
    for r in 0..rows {
        let s = r * stride;
        if s + row > src.len() {
            break;
        }
        out.extend_from_slice(&src[s..s + row]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_de_couleurs() {
        // Blanc, noir, puis rouge pur (Y=81, U=90, V=240 en BT.601).
        let mut out = Vec::new();
        yuyv_to_rgb32(&[235, 128, 16, 128], 2, 1, 4, &mut out);
        assert_eq!(&out[..4], &[255, 255, 255, 255]);
        assert_eq!(&out[4..8], &[0, 0, 0, 255]);
        yuyv_to_rgb32(&[81, 90, 81, 240], 2, 1, 4, &mut out);
        assert!(
            out[2] > 240 && out[1] < 10 && out[0] < 10,
            "{:?}",
            &out[..4]
        );
        // NV12 2x2 gris moyen.
        nv12_to_rgb32(&[126, 126, 126, 126, 128, 128], 2, 2, 2, &mut out);
        assert!(out.chunks(4).all(|p| p[0] == p[1] && p[1] == p[2]));
        let mut nv = Vec::new();
        yuyv_to_nv12(&[10, 1, 20, 2, 30, 3, 40, 4], 2, 2, 4, &mut nv);
        assert_eq!(nv, vec![10, 20, 30, 40, 1, 2]);
    }

    #[test]
    fn retire_le_remplissage_des_lignes() {
        let mode = Mode {
            fourcc: YUYV,
            width: 2,
            height: 2,
            fps_num: 30,
            fps_den: 1,
        };
        let src = [1, 2, 3, 4, 0, 0, 5, 6, 7, 8, 0, 0];
        assert_eq!(pack(&src, &mode, 6), vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(mode.frame_bytes(), 8);
    }

    #[test]
    fn la_confidentialite_bloque_la_camera() {
        let conf = std::env::temp_dir().join(format!("winlux-privacy-{}", std::process::id()));
        std::fs::write(&conf, "MICROPHONE=allow\nCAMERA=deny\n").unwrap();
        std::env::set_var("WINLUX_PRIVACY_CONF", &conf);
        assert!(!camera_allowed());
        let mode = Mode {
            fourcc: YUYV,
            width: 640,
            height: 480,
            fps_num: 30,
            fps_den: 1,
        };
        assert!(Stream::open(Path::new("/dev/null"), mode).is_none());
        std::fs::write(&conf, "CAMERA=allow\n").unwrap();
        assert!(camera_allowed());
        std::env::remove_var("WINLUX_PRIVACY_CONF");
        std::fs::remove_file(conf).unwrap();
    }

    #[test]
    fn aucune_camera_dans_un_dossier_vide() {
        std::env::set_var("WINLUX_VIDEO_DIR", "/nonexistent-winlux-video");
        assert!(cameras().is_empty());
        assert!(probe(Path::new("/nonexistent-winlux-video/video0")).is_none());
    }
}
