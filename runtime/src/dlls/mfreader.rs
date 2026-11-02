//! Lecture de fichiers par Media Foundation : `MFCreateSourceReaderFromURL`
//! renvoie un `IMFSourceReader` qui décode la vidéo et l'audio du fichier
//! avec le moteur multimédia de Winlux (FFmpeg en processus séparé, voir
//! `winlux_media`), pour tous les formats qu'il connaît (H.264, HEVC, VP9,
//! AV1, MPEG-4, AAC, MP3, Opus, Vorbis, FLAC, AC-3…).
//!
//! Types de sortie : vidéo RGB32 (de haut en bas, pas positif) ou NV12,
//! audio PCM 16 bits ou flottant 32 bits, au débit et aux canaux natifs.
//! Les types natifs annoncent le codec du fichier ; tant que le programme
//! ne choisit pas de type, les échantillons sont déjà décodés (RGB32, PCM).
//! Positionnement par `SetCurrentPosition`, durée par
//! `GetPresentationAttribute(MF_PD_DURATION)`, modes synchrone et
//! asynchrone (IMFSourceReaderCallback).

use super::comobj::{
    self, guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_POINTER, IID_IUNKNOWN, S_OK,
};
use super::lock;
use super::mfcapture::{
    self as mf, attr_get_guid, attr_get_u64, attr_get_unknown, com_qi, com_release, media_subtype,
    slot, Val, MF_E_INVALIDMEDIATYPE, MF_E_INVALIDREQUEST, MF_E_INVALIDSTREAMNUMBER,
    MF_E_NO_MORE_TYPES, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
};
use super::winlux_media::{self, MediaInfo};
use std::ffi::c_void;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};

const MF_E_UNSUPPORTED_BYTESTREAM_TYPE: i32 = 0xC00D_36C4u32 as i32;
const MF_E_UNSUPPORTED_TIME_FORMAT: i32 = 0xC00D_36E0u32 as i32;

pub const MFMEDIATYPE_AUDIO: Guid = media_subtype(0x7364_7561);
pub const MFAUDIOFORMAT_PCM: Guid = media_subtype(1);
pub const MFAUDIOFORMAT_FLOAT: Guid = media_subtype(3);

pub const MF_MT_AUDIO_NUM_CHANNELS: Guid = guid(
    0x37e48bf5,
    0x645e,
    0x4c5b,
    [0x89, 0xde, 0xad, 0xa9, 0xe2, 0x9b, 0x69, 0x6a],
);
pub const MF_MT_AUDIO_SAMPLES_PER_SECOND: Guid = guid(
    0x5faeeae7,
    0x0290,
    0x4c31,
    [0x9e, 0x8a, 0xc5, 0x34, 0xf6, 0x8d, 0x9d, 0xba],
);
pub const MF_MT_AUDIO_BITS_PER_SAMPLE: Guid = guid(
    0xf2deb57f,
    0x40fa,
    0x4764,
    [0xaa, 0x33, 0xed, 0x4f, 0x2d, 0x1f, 0xf6, 0x69],
);
pub const MF_MT_AUDIO_BLOCK_ALIGNMENT: Guid = guid(
    0x322de230,
    0x9eeb,
    0x43bd,
    [0xab, 0x7a, 0xff, 0x41, 0x22, 0x51, 0x54, 0x1d],
);
pub const MF_MT_AUDIO_AVG_BYTES_PER_SECOND: Guid = guid(
    0x1aab75c8,
    0xcfef,
    0x451c,
    [0xab, 0x95, 0xac, 0x03, 0x4b, 0x8e, 0x17, 0x31],
);
pub const MF_PD_DURATION: Guid = guid(
    0x6c990d33,
    0xbb8e,
    0x477a,
    [0x85, 0x98, 0x0d, 0x5d, 0x96, 0xfc, 0xd8, 0x8a],
);

const STREAM_FIRST_VIDEO: u32 = 0xFFFF_FFFC;
const STREAM_FIRST_AUDIO: u32 = 0xFFFF_FFFD;
const STREAM_ANY: u32 = 0xFFFF_FFFE;
const STREAM_MEDIASOURCE: u32 = 0xFFFF_FFFF;
const READERF_ERROR: u32 = 0x1;
const READERF_ENDOFSTREAM: u32 = 0x2;

/// Morceau d'audio rendu par ReadSample : 1024 trames, comme un décodeur AAC.
const AUDIO_FRAMES: usize = 1024;

/// Format de sortie d'un flux.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    Rgb32,
    Nv12,
    Pcm16,
    Float,
}

impl Out {
    fn subtype(self) -> Guid {
        match self {
            Out::Rgb32 => mf::MFVIDEOFORMAT_RGB32,
            Out::Nv12 => mf::MFVIDEOFORMAT_NV12,
            Out::Pcm16 => MFAUDIOFORMAT_PCM,
            Out::Float => MFAUDIOFORMAT_FLOAT,
        }
    }
    fn from_subtype(g: &Guid, video: bool) -> Option<Out> {
        let all = if video {
            [Out::Rgb32, Out::Nv12]
        } else {
            [Out::Pcm16, Out::Float]
        };
        all.into_iter()
            .find(|o| o.subtype() == *g)
            .or_else(|| (video && *g == mf::MFVIDEOFORMAT_ARGB32).then_some(Out::Rgb32))
    }
}

/// Un flux du fichier vu par Media Foundation.
#[derive(Clone, Debug)]
pub struct Stream {
    pub video: bool,
    /// Rang parmi les flux du même genre (0:v:N, 0:a:N pour FFmpeg).
    pub ordinal: u32,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps_x100: u32,
    pub channels: u32,
    pub rate: u32,
    pub out: Out,
    pub selected: bool,
}

impl Stream {
    /// Taille d'un échantillon décodé, en octets.
    pub fn sample_bytes(&self) -> usize {
        match self.out {
            Out::Rgb32 => self.width as usize * self.height as usize * 4,
            Out::Nv12 => self.width as usize * self.height as usize * 3 / 2,
            Out::Pcm16 => AUDIO_FRAMES * self.channels.max(1) as usize * 2,
            Out::Float => AUDIO_FRAMES * self.channels.max(1) as usize * 4,
        }
    }
    /// Durée d'un échantillon, en unités de 100 ns.
    pub fn sample_duration(&self) -> i64 {
        if self.video {
            10_000_000 * 100 / self.fps_x100.max(1) as i64
        } else {
            10_000_000 * AUDIO_FRAMES as i64 / self.rate.max(1) as i64
        }
    }
    /// Arguments FFmpeg qui décodent ce flux depuis `start` (100 ns).
    pub fn ffmpeg_args(&self, path: &Path, start: i64) -> Vec<String> {
        let mut args: Vec<String> = ["-nostdin", "-v", "error"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        if start > 0 {
            args.extend(["-ss".into(), format!("{:.3}", start as f64 / 1e7)]);
        }
        args.extend(["-i".into(), path.display().to_string()]);
        let kind = if self.video { 'v' } else { 'a' };
        args.extend(["-map".into(), format!("0:{kind}:{}", self.ordinal)]);
        match self.out {
            Out::Rgb32 | Out::Nv12 => {
                let pix = if self.out == Out::Rgb32 {
                    "bgra"
                } else {
                    "nv12"
                };
                args.extend(
                    ["-f", "rawvideo", "-pix_fmt", pix]
                        .iter()
                        .map(|s| s.to_string()),
                );
                // Débit constant : une image par intervalle, comme MF.
                args.extend(["-fps_mode".into(), "cfr".into()]);
                args.extend([
                    "-r".into(),
                    format!("{:.3}", self.fps_x100.max(1) as f64 / 100.0),
                ]);
            }
            Out::Pcm16 | Out::Float => {
                let fmt = if self.out == Out::Pcm16 {
                    "s16le"
                } else {
                    "f32le"
                };
                args.extend(["-f".into(), fmt.into()]);
                args.extend(["-ac".into(), self.channels.max(1).to_string()]);
                args.extend(["-ar".into(), self.rate.max(1).to_string()]);
            }
        }
        args.push("-".into());
        args
    }
}

/// Code FOURCC Media Foundation d'un codec FFmpeg (type natif annoncé).
pub fn native_subtype(codec: &str, video: bool) -> Guid {
    let fourcc = |s: &[u8; 4]| u32::from_le_bytes(*s);
    let code = match codec {
        "h264" => fourcc(b"H264"),
        "hevc" => fourcc(b"HEVC"),
        "vp8" => fourcc(b"VP80"),
        "vp9" => fourcc(b"VP90"),
        "av1" => fourcc(b"AV01"),
        "mpeg4" => fourcc(b"MP4V"),
        "mpeg2video" => fourcc(b"MPG2"),
        "mjpeg" => fourcc(b"MJPG"),
        "wmv3" => fourcc(b"WMV3"),
        "aac" => 0x1610,
        "mp3" | "mp2" => 0x55,
        "ac3" => 0x2000,
        "eac3" => fourcc(b"ec-3"),
        "flac" => 0xF1AC,
        "opus" => 0x704F,
        "alac" => 0x6C61,
        "wmav2" => 0x161,
        c if c.starts_with("pcm_f") => 3,
        c if c.starts_with("pcm_") => 1,
        _ if video => fourcc(b"H264"),
        _ => 0x1610,
    };
    media_subtype(code)
}

/// Flux décodables du fichier, vidéo d'abord puis audio (pochettes exclues).
pub fn streams_of(info: &MediaInfo) -> Vec<Stream> {
    let mut out = Vec::new();
    for s in info
        .streams
        .iter()
        .filter(|s| s.kind == 'v' && !s.attached_pic)
    {
        out.push(Stream {
            video: true,
            ordinal: s.ordinal,
            codec: s.codec.clone(),
            width: s.width & !1,
            height: s.height & !1,
            fps_x100: if s.fps_x100 == 0 { 2500 } else { s.fps_x100 },
            channels: 0,
            rate: 0,
            out: Out::Rgb32,
            selected: out.is_empty(),
        });
    }
    let first_audio = out.len();
    for s in info.streams.iter().filter(|s| s.kind == 'a') {
        out.push(Stream {
            video: false,
            ordinal: s.ordinal,
            codec: s.codec.clone(),
            width: 0,
            height: 0,
            fps_x100: 0,
            channels: s.channels.clamp(1, 8),
            rate: if s.sample_rate == 0 {
                48_000
            } else {
                s.sample_rate
            },
            out: Out::Pcm16,
            selected: out.len() == first_audio,
        });
    }
    out
}

fn media_type(s: &Stream, native: bool) -> *mut c_void {
    let mut v: Vec<(Guid, Val)> = Vec::new();
    if s.video {
        v.push((MF_MT_MAJOR_TYPE, Val::Guid(mf::MFMEDIATYPE_VIDEO)));
        let sub = if native {
            native_subtype(&s.codec, true)
        } else {
            s.out.subtype()
        };
        v.push((MF_MT_SUBTYPE, Val::Guid(sub)));
        v.push((
            MF_MT_FRAME_SIZE,
            Val::U64(((s.width as u64) << 32) | s.height as u64),
        ));
        v.push((
            MF_MT_FRAME_RATE,
            Val::U64(((s.fps_x100 as u64) << 32) | 100),
        ));
        v.push((mf::MF_MT_PIXEL_ASPECT_RATIO, Val::U64((1 << 32) | 1)));
        v.push((mf::MF_MT_INTERLACE_MODE, Val::U32(2)));
        if !native {
            v.push((mf::MF_MT_ALL_SAMPLES_INDEPENDENT, Val::U32(1)));
            v.push((mf::MF_MT_FIXED_SIZE_SAMPLES, Val::U32(1)));
            v.push((mf::MF_MT_SAMPLE_SIZE, Val::U32(s.sample_bytes() as u32)));
            let stride = if s.out == Out::Rgb32 {
                s.width * 4
            } else {
                s.width
            };
            v.push((mf::MF_MT_DEFAULT_STRIDE, Val::U32(stride)));
        }
    } else {
        v.push((MF_MT_MAJOR_TYPE, Val::Guid(MFMEDIATYPE_AUDIO)));
        let sub = if native {
            native_subtype(&s.codec, false)
        } else {
            s.out.subtype()
        };
        v.push((MF_MT_SUBTYPE, Val::Guid(sub)));
        v.push((MF_MT_AUDIO_NUM_CHANNELS, Val::U32(s.channels)));
        v.push((MF_MT_AUDIO_SAMPLES_PER_SECOND, Val::U32(s.rate)));
        if !native {
            let bits = if s.out == Out::Pcm16 { 16 } else { 32 };
            let block = s.channels * bits / 8;
            v.push((MF_MT_AUDIO_BITS_PER_SAMPLE, Val::U32(bits)));
            v.push((MF_MT_AUDIO_BLOCK_ALIGNMENT, Val::U32(block)));
            v.push((MF_MT_AUDIO_AVG_BYTES_PER_SECOND, Val::U32(block * s.rate)));
            v.push((mf::MF_MT_ALL_SAMPLES_INDEPENDENT, Val::U32(1)));
        }
    }
    mf::media_type_from(v)
}

// ---------- Décodage ----------

struct Decoder {
    child: Child,
    out: ChildStdout,
    next: i64,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct State {
    path: PathBuf,
    duration: i64,
    streams: Vec<Stream>,
    decoders: Vec<Option<Decoder>>,
    ended: Vec<bool>,
    position: i64,
}

impl State {
    fn resolve(&self, s: u32) -> Option<usize> {
        match s {
            STREAM_FIRST_VIDEO => self.streams.iter().position(|x| x.video),
            STREAM_FIRST_AUDIO => self.streams.iter().position(|x| !x.video),
            n if (n as usize) < self.streams.len() => Some(n as usize),
            _ => None,
        }
    }

    /// Lit un échantillon : (hr, index, drapeaux, horodatage, échantillon).
    fn read(&mut self, s: u32) -> (i32, u32, u32, i64, *mut c_void) {
        let index = if s == STREAM_ANY {
            // Le flux sélectionné le moins avancé, comme l'entrelacement de MF.
            let next = |i: usize| self.decoders[i].as_ref().map_or(self.position, |d| d.next);
            match (0..self.streams.len())
                .filter(|&i| self.streams[i].selected && !self.ended[i])
                .min_by_key(|&i| next(i))
            {
                Some(i) => i,
                None => {
                    let any = self.streams.iter().position(|x| x.selected).unwrap_or(0);
                    return (
                        S_OK,
                        any as u32,
                        READERF_ENDOFSTREAM,
                        0,
                        std::ptr::null_mut(),
                    );
                }
            }
        } else {
            match self.resolve(s) {
                Some(i) => i,
                None => {
                    return (
                        MF_E_INVALIDSTREAMNUMBER,
                        s,
                        READERF_ERROR,
                        0,
                        std::ptr::null_mut(),
                    )
                }
            }
        };
        if !self.streams[index].selected {
            return (
                MF_E_INVALIDREQUEST,
                index as u32,
                READERF_ERROR,
                0,
                std::ptr::null_mut(),
            );
        }
        if self.ended[index] {
            return (
                S_OK,
                index as u32,
                READERF_ENDOFSTREAM,
                self.position,
                std::ptr::null_mut(),
            );
        }
        if self.decoders[index].is_none() {
            let Some(ffmpeg) = winlux_media::caps().ffmpeg.clone() else {
                return (
                    MF_E_UNSUPPORTED_BYTESTREAM_TYPE,
                    index as u32,
                    READERF_ERROR,
                    0,
                    std::ptr::null_mut(),
                );
            };
            let args = self.streams[index].ffmpeg_args(&self.path, self.position);
            let spawned = Command::new(ffmpeg)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = spawned else {
                return (
                    MF_E_UNSUPPORTED_BYTESTREAM_TYPE,
                    index as u32,
                    READERF_ERROR,
                    0,
                    std::ptr::null_mut(),
                );
            };
            let Some(out) = child.stdout.take() else {
                return (
                    MF_E_UNSUPPORTED_BYTESTREAM_TYPE,
                    index as u32,
                    READERF_ERROR,
                    0,
                    std::ptr::null_mut(),
                );
            };
            self.decoders[index] = Some(Decoder {
                child,
                out,
                next: self.position,
            });
        }
        let stream = self.streams[index].clone();
        let Some(dec) = self.decoders[index].as_mut() else {
            return (
                MF_E_INVALIDREQUEST,
                index as u32,
                READERF_ERROR,
                0,
                std::ptr::null_mut(),
            );
        };
        let mut buf = vec![0u8; stream.sample_bytes()];
        let mut filled = 0;
        while filled < buf.len() {
            match dec.out.read(&mut buf[filled..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => filled += n,
            }
        }
        let whole = if stream.video {
            filled == buf.len()
        } else {
            filled > 0
        };
        if !whole {
            self.decoders[index] = None;
            self.ended[index] = true;
            return (
                S_OK,
                index as u32,
                READERF_ENDOFSTREAM,
                self.position,
                std::ptr::null_mut(),
            );
        }
        buf.truncate(filled);
        let time = dec.next;
        let duration = if stream.video {
            stream.sample_duration()
        } else {
            let frame =
                stream.channels.max(1) as usize * if stream.out == Out::Pcm16 { 2 } else { 4 };
            10_000_000 * (filled / frame) as i64 / stream.rate.max(1) as i64
        };
        dec.next += duration;
        (
            S_OK,
            index as u32,
            0,
            time,
            mf::sample_from(&buf, time, duration),
        )
    }

    fn seek(&mut self, position: i64) {
        self.position = position.clamp(0, self.duration.max(0));
        for d in &mut self.decoders {
            *d = None;
        }
        self.ended.fill(false);
    }
}

// ---------- IMFSourceReader ----------

enum Job {
    Read(u32),
    Flush(u32),
}

#[repr(C)]
struct Reader {
    vtbl: *const (),
    refs: AtomicU32,
    state: Arc<Mutex<State>>,
    callback: usize,
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
}

impl Drop for Reader {
    fn drop(&mut self) {
        // Fin du fil asynchrone avant de rendre la procédure de rappel.
        lock(&self.jobs).take();
        unsafe { com_release(self.callback as *mut c_void) };
    }
}

unsafe fn me<'a>(this: *mut Reader) -> &'a Reader {
    &*this
}

unsafe extern "win64" fn r_qi(this: *mut Reader, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let iid = comobj::read_guid(riid);
    if iid == Some(IID_IUNKNOWN) || iid == Some(mf::IID_IMFSOURCEREADER) {
        me(this).refs.fetch_add(1, Ordering::Relaxed);
        *out = this as *mut c_void;
        S_OK
    } else {
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn r_addref(this: *mut Reader) -> u32 {
    me(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn r_release(this: *mut Reader) -> u32 {
    let n = me(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this));
    }
    n
}
unsafe extern "win64" fn r_get_stream_selection(this: *mut Reader, s: u32, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let st = lock(&me(this).state);
    match st.resolve(s) {
        Some(i) => {
            *out = i32::from(st.streams[i].selected);
            S_OK
        }
        None => MF_E_INVALIDSTREAMNUMBER,
    }
}
unsafe extern "win64" fn r_set_stream_selection(this: *mut Reader, s: u32, sel: i32) -> i32 {
    let mut st = lock(&me(this).state);
    let targets: Vec<usize> = if s == STREAM_ANY || s == STREAM_MEDIASOURCE {
        (0..st.streams.len()).collect()
    } else {
        match st.resolve(s) {
            Some(i) => vec![i],
            None => return MF_E_INVALIDSTREAMNUMBER,
        }
    };
    for i in targets {
        st.streams[i].selected = sel != 0;
        if sel == 0 {
            st.decoders[i] = None;
        }
    }
    S_OK
}
unsafe extern "win64" fn r_get_native_media_type(
    this: *mut Reader,
    s: u32,
    index: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let st = lock(&me(this).state);
    let Some(i) = st.resolve(s) else {
        return MF_E_INVALIDSTREAMNUMBER;
    };
    if index != 0 {
        *out = std::ptr::null_mut();
        return MF_E_NO_MORE_TYPES;
    }
    *out = media_type(&st.streams[i], true);
    S_OK
}
unsafe extern "win64" fn r_get_current_media_type(
    this: *mut Reader,
    s: u32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let st = lock(&me(this).state);
    let Some(i) = st.resolve(s) else {
        return MF_E_INVALIDSTREAMNUMBER;
    };
    *out = media_type(&st.streams[i], false);
    S_OK
}
unsafe extern "win64" fn r_set_current_media_type(
    this: *mut Reader,
    s: u32,
    _reserved: *mut u32,
    t: *mut c_void,
) -> i32 {
    if t.is_null() {
        return E_POINTER;
    }
    let mut st = lock(&me(this).state);
    let Some(i) = st.resolve(s) else {
        return MF_E_INVALIDSTREAMNUMBER;
    };
    let video = st.streams[i].video;
    let Some(sub) = attr_get_guid(t, &MF_MT_SUBTYPE) else {
        return MF_E_INVALIDMEDIATYPE;
    };
    // Le type natif (compressé) est accepté et rendu décodé.
    let out = if sub == native_subtype(&st.streams[i].codec, video) {
        Some(st.streams[i].out)
    } else {
        Out::from_subtype(&sub, video)
    };
    let Some(out) = out else {
        return MF_E_INVALIDMEDIATYPE;
    };
    let stream = &mut st.streams[i];
    stream.out = out;
    // Conversion de définition, de débit et de canaux demandée par le programme.
    if video {
        if let Some(size) = attr_get_u64(t, &MF_MT_FRAME_SIZE) {
            let (w, h) = ((size >> 32) as u32 & !1, size as u32 & !1);
            if (16..=8192).contains(&w) && (16..=8192).contains(&h) {
                stream.width = w;
                stream.height = h;
            }
        }
    } else {
        if let Some(rate) =
            attr_u32(t, &MF_MT_AUDIO_SAMPLES_PER_SECOND).filter(|r| (8000..=192_000).contains(r))
        {
            stream.rate = rate;
        }
        if let Some(ch) = attr_u32(t, &MF_MT_AUDIO_NUM_CHANNELS).filter(|c| (1..=8).contains(c)) {
            stream.channels = ch;
        }
    }
    st.decoders[i] = None;
    S_OK
}
/// IMFAttributes::GetUINT32 (index 7).
unsafe fn attr_u32(obj: *mut c_void, key: &Guid) -> Option<u32> {
    let f: unsafe extern "win64" fn(*mut c_void, *const u8, *mut u32) -> i32 =
        std::mem::transmute(slot(obj, 7));
    let mut v = 0u32;
    (f(obj, key.as_ptr(), &mut v) >= 0).then_some(v)
}
unsafe extern "win64" fn r_set_current_position(
    this: *mut Reader,
    format: *const u8,
    pv: *const u8,
) -> i32 {
    if pv.is_null() {
        return E_POINTER;
    }
    if !format.is_null() && comobj::read_guid(format).is_some_and(|g| g != [0u8; 16]) {
        return MF_E_UNSUPPORTED_TIME_FORMAT;
    }
    // PROPVARIANT VT_I8 : valeur à l'octet 8.
    let vt = std::ptr::read_unaligned(pv as *const u16);
    if vt != 20 && vt != 21 {
        return E_INVALIDARG;
    }
    let pos = std::ptr::read_unaligned(pv.add(8) as *const i64);
    lock(&me(this).state).seek(pos);
    S_OK
}
unsafe extern "win64" fn r_read_sample(
    this: *mut Reader,
    s: u32,
    _control: u32,
    actual: *mut u32,
    flags: *mut u32,
    ts: *mut i64,
    sample: *mut *mut c_void,
) -> i32 {
    let r = me(this);
    if r.callback != 0 {
        // Mode asynchrone : la réponse arrive par OnReadSample.
        if !actual.is_null() || !flags.is_null() || !ts.is_null() || !sample.is_null() {
            return E_INVALIDARG;
        }
        return match lock(&r.jobs).as_ref() {
            Some(tx) if tx.send(Job::Read(s)).is_ok() => S_OK,
            _ => MF_E_INVALIDREQUEST,
        };
    }
    if sample.is_null() {
        return E_POINTER;
    }
    let (hr, index, f, time, smp) = lock(&r.state).read(s);
    if !actual.is_null() {
        *actual = index;
    }
    if !flags.is_null() {
        *flags = f;
    }
    if !ts.is_null() {
        *ts = time;
    }
    *sample = smp;
    hr
}
unsafe extern "win64" fn r_flush(this: *mut Reader, s: u32) -> i32 {
    let r = me(this);
    {
        let mut st = lock(&r.state);
        let pos = st.position;
        // Les décodeurs repartent de la position courante de chaque flux.
        let resume: Vec<i64> = st
            .decoders
            .iter()
            .map(|d| d.as_ref().map_or(pos, |d| d.next))
            .collect();
        st.seek(resume.into_iter().min().unwrap_or(pos));
    }
    if r.callback != 0 {
        if let Some(tx) = lock(&r.jobs).as_ref() {
            let _ = tx.send(Job::Flush(s));
        }
    }
    S_OK
}
unsafe extern "win64" fn r_get_service_for_stream(
    _this: *mut Reader,
    _s: u32,
    _service: *const u8,
    _riid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    mf::MF_E_UNSUPPORTED_SERVICE
}
unsafe extern "win64" fn r_get_presentation_attribute(
    this: *mut Reader,
    s: u32,
    key: *const u8,
    pv: *mut u8,
) -> i32 {
    if key.is_null() || pv.is_null() {
        return E_POINTER;
    }
    let st = lock(&me(this).state);
    let key = comobj::read_guid(key).unwrap_or([0; 16]);
    if s == STREAM_MEDIASOURCE && key == MF_PD_DURATION {
        mf::write_propvariant(pv, &Val::U64(st.duration as u64));
        return S_OK;
    }
    mf::MF_E_ATTRIBUTENOTFOUND
}

static READER_VTBL: VTable<13> = VTable([
    r_qi as *const (),
    r_addref as *const (),
    r_release as *const (),
    r_get_stream_selection as *const (),
    r_set_stream_selection as *const (),
    r_get_native_media_type as *const (),
    r_get_current_media_type as *const (),
    r_set_current_media_type as *const (),
    r_set_current_position as *const (),
    r_read_sample as *const (),
    r_flush as *const (),
    r_get_service_for_stream as *const (),
    r_get_presentation_attribute as *const (),
]);

/// Fil du mode asynchrone : appelle IMFSourceReaderCallback::OnReadSample.
fn spawn_worker(state: Arc<Mutex<State>>, callback: usize) -> Option<mpsc::Sender<Job>> {
    let (tx, rx) = mpsc::channel::<Job>();
    std::thread::Builder::new()
        .name("winlux-mf-file".into())
        .spawn(move || {
            crate::process::init_thread();
            let cb = callback as *mut c_void;
            for job in rx {
                unsafe {
                    match job {
                        Job::Read(s) => {
                            let (hr, index, flags, ts, smp) = lock(&state).read(s);
                            let f: unsafe extern "win64" fn(
                                *mut c_void,
                                i32,
                                u32,
                                u32,
                                i64,
                                *mut c_void,
                            ) -> i32 = std::mem::transmute(slot(cb, 3));
                            f(cb, hr, index, flags, ts, smp);
                            com_release(smp);
                        }
                        Job::Flush(s) => {
                            let f: unsafe extern "win64" fn(*mut c_void, u32) -> i32 =
                                std::mem::transmute(slot(cb, 4));
                            f(cb, s);
                        }
                    }
                }
            }
        })
        .ok()?;
    Some(tx)
}

/// Chemin Linux d'une URL de fichier (« C:\… », « file:///… »).
pub fn url_to_path(url: &str) -> Option<PathBuf> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return None;
    }
    let text = match url
        .strip_prefix("file:///")
        .or_else(|| url.strip_prefix("file://"))
    {
        Some(rest) => {
            let decoded = percent_decode(rest);
            if decoded.as_bytes().get(1) == Some(&b':') {
                decoded
            } else {
                format!("/{decoded}")
            }
        }
        None => url.to_string(),
    };
    let path = if text.starts_with('/') {
        PathBuf::from(&text)
    } else {
        crate::paths::to_linux(&text)
    };
    path.is_file().then_some(path)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// MFCreateSourceReaderFromURL : lecteur de source sur un fichier local.
pub unsafe fn create_from_url(url: &str, attrs: *mut c_void) -> Result<*mut c_void, i32> {
    let path = url_to_path(url).ok_or(MF_E_UNSUPPORTED_BYTESTREAM_TYPE)?;
    let info = winlux_media::probe(&path).ok_or(MF_E_UNSUPPORTED_BYTESTREAM_TYPE)?;
    let streams = streams_of(&info);
    if streams.is_empty() {
        return Err(MF_E_UNSUPPORTED_BYTESTREAM_TYPE);
    }
    let callback = attr_get_unknown(
        attrs,
        &mf::MF_SOURCE_READER_ASYNC_CALLBACK,
        &mf::IID_IMFSOURCEREADERCALLBACK,
    )
    .or_else(|| {
        attr_get_unknown(attrs, &mf::MF_SOURCE_READER_ASYNC_CALLBACK, &IID_IUNKNOWN).and_then(|u| {
            let cb = com_qi(u, &mf::IID_IMFSOURCEREADERCALLBACK);
            com_release(u);
            cb
        })
    })
    .map_or(0, |p| p as usize);
    let n = streams.len();
    let state = Arc::new(Mutex::new(State {
        path,
        duration: info.duration_ms as i64 * 10_000,
        streams,
        decoders: (0..n).map(|_| None).collect(),
        ended: vec![false; n],
        position: 0,
    }));
    let jobs = if callback != 0 {
        spawn_worker(state.clone(), callback)
    } else {
        None
    };
    Ok(Box::into_raw(Box::new(Reader {
        vtbl: READER_VTBL.0.as_ptr() as *const (),
        refs: AtomicU32::new(1),
        state,
        callback,
        jobs: Mutex::new(jobs),
    })) as *mut c_void)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_et_types() {
        assert!(url_to_path("https://exemple.org/v.mp4").is_none());
        assert_eq!(percent_decode("Vid%C3%A9o%20test.mp4"), "Vidéo test.mp4");
        assert_eq!(
            native_subtype("h264", true),
            media_subtype(u32::from_le_bytes(*b"H264"))
        );
        assert_eq!(native_subtype("aac", false), media_subtype(0x1610));
        let s = Stream {
            video: true,
            ordinal: 0,
            codec: "h264".into(),
            width: 64,
            height: 48,
            fps_x100: 2500,
            channels: 0,
            rate: 0,
            out: Out::Rgb32,
            selected: true,
        };
        assert_eq!(s.sample_bytes(), 64 * 48 * 4);
        assert_eq!(s.sample_duration(), 400_000);
        let args = s.ffmpeg_args(Path::new("/tmp/v.mp4"), 20_000_000);
        assert!(args.windows(2).any(|w| w == ["-ss", "2.000"]));
        assert!(args.windows(2).any(|w| w == ["-pix_fmt", "bgra"]));
    }

    /// Bout en bout avec FFmpeg s'il est installé : vidéo de 1 s générée.
    #[test]
    fn lecture_d_un_fichier_avec_ffmpeg() {
        let Some(ffmpeg) = winlux_media::caps().ffmpeg.clone() else {
            return;
        };
        if winlux_media::caps().ffprobe.is_none() {
            return;
        }
        let file = std::env::temp_dir().join(format!("winlux-mf-{}.mkv", std::process::id()));
        let made = Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=64x48:rate=10:duration=1",
            ])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=8000:duration=1",
            ])
            .args(["-c:v", "mpeg4", "-c:a", "flac", "-shortest"])
            .arg(&file)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            return;
        }
        unsafe {
            let reader = create_from_url(&file.display().to_string(), std::ptr::null_mut())
                .expect("lecteur") as *mut Reader;
            let st = lock(&me(reader).state).streams.clone();
            assert_eq!(st.len(), 2);
            assert!(st[0].video && !st[1].video);
            let mut frames = 0;
            let mut last = -1i64;
            loop {
                let (mut idx, mut flags, mut ts, mut smp) =
                    (0u32, 0u32, 0i64, std::ptr::null_mut());
                let hr = r_read_sample(
                    reader,
                    STREAM_FIRST_VIDEO,
                    0,
                    &mut idx,
                    &mut flags,
                    &mut ts,
                    &mut smp,
                );
                assert_eq!(hr, S_OK);
                if flags & READERF_ENDOFSTREAM != 0 {
                    break;
                }
                assert!(ts > last, "horodatages croissants");
                last = ts;
                frames += 1;
                com_release(smp);
                assert!(frames < 100);
            }
            assert!((9..=11).contains(&frames), "10 images attendues, {frames}");
            // Audio PCM, puis retour au début.
            let (mut idx, mut flags, mut ts, mut smp) = (0u32, 0u32, 0i64, std::ptr::null_mut());
            assert_eq!(
                r_read_sample(
                    reader,
                    STREAM_FIRST_AUDIO,
                    0,
                    &mut idx,
                    &mut flags,
                    &mut ts,
                    &mut smp
                ),
                S_OK
            );
            assert_eq!((idx, flags), (1, 0));
            com_release(smp);
            let mut pv = [0u8; 24];
            pv[0] = 20;
            pv[8..16].copy_from_slice(&5_000_000i64.to_le_bytes());
            assert_eq!(
                r_set_current_position(reader, std::ptr::null(), pv.as_ptr()),
                S_OK
            );
            assert_eq!(
                r_read_sample(
                    reader,
                    STREAM_FIRST_VIDEO,
                    0,
                    &mut idx,
                    &mut flags,
                    &mut ts,
                    &mut smp
                ),
                S_OK
            );
            assert_eq!(ts, 5_000_000, "lecture reprise à 0,5 s");
            com_release(smp);
            let mut dur = [0u8; 24];
            let key = MF_PD_DURATION;
            assert_eq!(
                r_get_presentation_attribute(
                    reader,
                    STREAM_MEDIASOURCE,
                    key.as_ptr(),
                    dur.as_mut_ptr()
                ),
                S_OK
            );
            let d = u64::from_le_bytes(dur[8..16].try_into().unwrap());
            assert!((9_000_000..=11_000_000).contains(&d), "durée {d}");
            r_release(reader);
        }
        let _ = std::fs::remove_file(file);
    }
}
