//! Écran de démarrage pendant le chargement du système : le logo Winlux
//! « Prisme » sur un halo violet, avec le cercle de points qui tourne, dessiné
//! directement dans le framebuffer (`/dev/fb0`) entre GRUB et le bureau.
//!
//! L'init lance `winlux --boot-splash &` dès que `/dev` est monté et l'arrête
//! juste avant Xorg (lancé avec `-background none`, qui garde l'image jusqu'au
//! premier dessin du bureau). Même géométrie et mêmes couleurs que
//! `testexe/src/shell/brand.rs` et l'écran de démarrage du shell.
//!
//! `winlux --boot-splash --ppm FICHIER [LARGEUR HAUTEUR]` écrit une image au
//! lieu du framebuffer (tests et captures).

use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Centre optique du losange (fraction de sa hauteur).
const CY: f32 = 0.42;
/// Facettes (haut, bas) : haut-gauche, bas-gauche, haut-droite, bas-droite.
const FACETS: [(u32, u32); 4] = [
    (0xB69CFF, 0x7C4DFF),
    (0x7C4DFF, 0x4B23C8),
    (0x7DE8FF, 0x22B8F0),
    (0x22B8F0, 0x125CD6),
];
const HALO: u32 = 0x2E1C70;
const DOT: u32 = 0xF2F2F2;
/// Au-delà, l'écran est laissé tel quel (le bureau aurait dû prendre la main).
const MAX_RUN: Duration = Duration::from_secs(90);

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn channel(c: u32, shift: u32) -> f32 {
    ((c >> shift) & 0xFF) as f32
}

/// Mélange `a` → `b` (`t` de 0 à 1), couleurs 0xRRGGBB.
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s| (channel(a, s) + (channel(b, s) - channel(a, s)) * t).round() as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Disposition de l'écran : mêmes proportions que le shell (desktop.rs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub w: i32,
    pub h: i32,
    pub size: i32,
    pub logo_x: i32,
    pub logo_y: i32,
    pub spin_x: i32,
    pub spin_y: i32,
}

pub fn layout(w: i32, h: i32) -> Layout {
    let size = (h / 9).clamp(72, 160);
    Layout {
        w,
        h,
        size,
        logo_x: w / 2 - size / 2,
        logo_y: h / 2 - size - 30,
        spin_x: w / 2,
        spin_y: h / 2 + 70,
    }
}

/// Couleur du logo au point (`u`, `v`) de sa boîte (0..1), avec la couverture
/// du bord (0..1) pour un contour lisse.
fn logo_at(u: f32, v: f32, size: f32) -> Option<(u32, f32)> {
    let inside = 0.5 - (u - 0.5).abs() - (v - 0.5).abs();
    let cover = (inside * size * std::f32::consts::SQRT_2 + 0.5).clamp(0.0, 1.0);
    if cover <= 0.0 {
        return None;
    }
    let edge = CY + (0.5 - CY) * (u - 0.5).abs() / 0.5;
    let k = match (u < 0.5, v < edge) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    };
    Some((mix(FACETS[k].0, FACETS[k].1, v), cover))
}

/// Fond : noir, halo violet elliptique derrière le logo, puis le logo.
pub fn background_pixel(l: &Layout, x: i32, y: i32) -> u32 {
    let (gx, gy) = (l.w / 2, l.logo_y + l.size / 2);
    let r = l.size as f32 * 3.75;
    let dx = (x - gx) as f32 / r;
    let dy = (y - gy) as f32 / (r * 0.75);
    let fall = (1.0 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
    let base = mix(0, HALO, fall * fall);
    let (lx, ly) = (x - l.logo_x, y - l.logo_y);
    if lx < -1 || ly < -1 || lx > l.size || ly > l.size {
        return base;
    }
    let s = l.size as f32;
    match logo_at((lx as f32 + 0.5) / s, (ly as f32 + 0.5) / s, s) {
        Some((c, cover)) => mix(base, c, cover),
        None => base,
    }
}

/// Positions des cinq points au temps `t` (ms depuis le premier dessin) :
/// accélèrent puis ralentissent sur deux tours, puis courte pause.
pub fn dots(l: &Layout, t: u64) -> Vec<(f32, f32)> {
    let cycle = 2200u64;
    let radius = 22.0f32;
    let mut out = Vec::new();
    for k in 0..5u64 {
        let local = (t + cycle * 10 - k * 110) % cycle;
        let u = local as f32 / cycle as f32;
        if u > 0.88 {
            continue;
        }
        let v = u / 0.88;
        let deg = 270.0 + v * 720.0 - (v * 360.0).to_radians().sin() * 70.0;
        let a = deg.to_radians();
        out.push((
            l.spin_x as f32 + a.cos() * radius,
            l.spin_y as f32 + a.sin() * radius,
        ));
    }
    out
}

/// Zone (carrée) redessinée à chaque image : le cercle de points.
fn spinner_box(l: &Layout) -> (i32, i32, i32, i32) {
    (l.spin_x - 30, l.spin_y - 30, l.spin_x + 31, l.spin_y + 31)
}

fn spinner_pixel(l: &Layout, d: &[(f32, f32)], x: i32, y: i32) -> u32 {
    let mut c = background_pixel(l, x, y);
    for &(px, py) in d {
        let dist = ((x as f32 + 0.5 - px).powi(2) + (y as f32 + 0.5 - py).powi(2)).sqrt();
        let cover = (3.0 - dist).clamp(0.0, 1.0);
        if cover > 0.0 {
            c = mix(c, DOT, cover);
        }
    }
    c
}

/// Format d'un pixel du framebuffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Format {
    pub bpp: u32,
    pub stride: usize,
}

fn encode(c: u32, f: Format, out: &mut Vec<u8>) {
    let (r, g, b) = ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF);
    match f.bpp {
        16 => {
            let v = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
            out.extend_from_slice(&(v as u16).to_le_bytes());
        }
        24 => out.extend_from_slice(&[b as u8, g as u8, r as u8]),
        _ => out.extend_from_slice(&[b as u8, g as u8, r as u8, 0]),
    }
}

/// Une ligne `[x0, x1)` de pixels encodés.
fn row(f: Format, y: i32, x0: i32, x1: i32, pixel: &dyn Fn(i32, i32) -> u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(((x1 - x0).max(0) as usize) * (f.bpp as usize / 8));
    for x in x0..x1 {
        encode(pixel(x, y), f, &mut out);
    }
    out
}

struct Fb {
    file: File,
    layout: Layout,
    format: Format,
}

fn read_trim(path: PathBuf) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Géométrie lue dans sysfs (`virtual_size`, `bits_per_pixel`, `stride`).
fn open_fb(dev: &Path, sys: &Path) -> Option<Fb> {
    let size = read_trim(sys.join("virtual_size"))?;
    let (w, h) = size.split_once(',')?;
    let (w, h): (i32, i32) = (w.parse().ok()?, h.parse().ok()?);
    let bpp: u32 = read_trim(sys.join("bits_per_pixel"))?.parse().ok()?;
    if w < 320 || h < 200 || !matches!(bpp, 16 | 24 | 32) {
        return None;
    }
    let stride = read_trim(sys.join("stride"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(w as usize * bpp as usize / 8);
    let file = OpenOptions::new().write(true).open(dev).ok()?;
    Some(Fb {
        file,
        layout: layout(w, h),
        format: Format { bpp, stride },
    })
}

impl Fb {
    fn write_rect(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        pixel: &dyn Fn(i32, i32) -> u32,
    ) -> bool {
        let l = self.layout;
        let (x0, x1) = (x0.max(0), x1.min(l.w));
        let (y0, y1) = (y0.max(0), y1.min(l.h));
        let px = self.format.bpp as usize / 8;
        for y in y0..y1 {
            let bytes = row(self.format, y, x0, x1, pixel);
            let at = y as usize * self.format.stride + x0 as usize * px;
            if self.file.seek(SeekFrom::Start(at as u64)).is_err()
                || self.file.write_all(&bytes).is_err()
            {
                return false;
            }
        }
        true
    }
}

/// Image entière en PPM (texte d'en-tête + RVB), pour tests et captures.
pub fn render_ppm(w: i32, h: i32, t: u64) -> Vec<u8> {
    let l = layout(w, h);
    let d = dots(&l, t);
    let (sx0, sy0, sx1, sy1) = spinner_box(&l);
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for y in 0..h {
        for x in 0..w {
            let c = if x >= sx0 && x < sx1 && y >= sy0 && y < sy1 {
                spinner_pixel(&l, &d, x, y)
            } else {
                background_pixel(&l, x, y)
            };
            out.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
        }
    }
    out
}

fn run(dev: &Path, sys: &Path) -> i32 {
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }
    let start = Instant::now();
    let mut fb: Option<Fb> = None;
    let mut last_geometry = Instant::now() - Duration::from_secs(10);
    while !STOP.load(Ordering::SeqCst) && start.elapsed() < MAX_RUN {
        if Path::new("/run/winlux/splash.stop").exists() {
            break;
        }
        // Le pilote graphique définitif (virtio-gpu, i915…) remplace le
        // framebuffer du micrologiciel en cours de route : relire sa géométrie.
        if fb.is_none() || last_geometry.elapsed() > Duration::from_millis(500) {
            last_geometry = Instant::now();
            let fresh = open_fb(dev, sys);
            let changed = match (&fb, &fresh) {
                (Some(a), Some(b)) => a.layout != b.layout || a.format != b.format,
                (None, Some(_)) => true,
                _ => false,
            };
            if changed || fresh.is_none() {
                fb = fresh;
                if let Some(f) = fb.as_mut() {
                    let l = f.layout;
                    if !f.write_rect(0, 0, l.w, l.h, &|x, y| background_pixel(&l, x, y)) {
                        fb = None;
                    }
                }
            } else if let (Some(f), Some(n)) = (fb.as_mut(), fresh) {
                // Même géométrie : garder un descripteur neuf si le pilote a changé.
                f.file = n.file;
            }
        }
        if let Some(f) = fb.as_mut() {
            let l = f.layout;
            let d = dots(&l, start.elapsed().as_millis() as u64);
            let (x0, y0, x1, y1) = spinner_box(&l);
            if !f.write_rect(x0, y0, x1, y1, &|x, y| spinner_pixel(&l, &d, x, y)) {
                fb = None;
            }
        }
        std::thread::sleep(Duration::from_millis(33));
    }
    0
}

/// `winlux --boot-splash [--fb DEV] [--sys DIR]` ou `--ppm FICHIER [L H [T]]`.
pub fn cli(args: &[String]) -> i32 {
    let mut dev = PathBuf::from("/dev/fb0");
    let mut sys = PathBuf::from("/sys/class/graphics/fb0");
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--fb" => dev = it.next().map(PathBuf::from).unwrap_or(dev),
            "--sys" => sys = it.next().map(PathBuf::from).unwrap_or(sys),
            "--ppm" => {
                let Some(out) = it.next() else {
                    eprintln!("winlux --boot-splash --ppm FICHIER [LARGEUR HAUTEUR [MS]]");
                    return 2;
                };
                let mut num = |d: u64| it.next().and_then(|v| v.parse().ok()).unwrap_or(d);
                let (w, h, t) = (num(1920) as i32, num(1080) as i32, num(600));
                return match fs::write(out, render_ppm(w.max(320), h.max(200), t)) {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("winlux --boot-splash : {e}");
                        1
                    }
                };
            }
            _ => {}
        }
    }
    run(&dev, &sys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_centre_et_coins_noirs() {
        let l = layout(1920, 1080);
        assert_eq!(l.size, 120);
        assert_eq!(background_pixel(&l, 0, 0), 0);
        // Facette haut-gauche : violet clair, haut-droite : cyan.
        let y = l.logo_y + l.size / 4;
        let left = background_pixel(&l, l.logo_x + l.size / 2 - 5, y);
        let right = background_pixel(&l, l.logo_x + l.size / 2 + 5, y);
        assert!(
            (left >> 16) & 0xFF > (left & 0xFF) / 2,
            "violet : {left:06x}"
        );
        assert!(
            right & 0xFF > 0xC0 && (right >> 16) & 0xFF < 0x90,
            "cyan : {right:06x}"
        );
        // Halo violet sombre à côté du logo.
        let halo = background_pixel(&l, l.logo_x - 20, l.logo_y + l.size / 2);
        assert!(halo != 0 && halo & 0xFF > (halo >> 8) & 0xFF);
    }

    #[test]
    fn points_du_cercle_de_chargement() {
        let l = layout(1280, 800);
        let d = dots(&l, 600);
        assert!(!d.is_empty() && d.len() <= 5);
        for (x, y) in d {
            let r = ((x - l.spin_x as f32).powi(2) + (y - l.spin_y as f32).powi(2)).sqrt();
            assert!((r - 22.0).abs() < 0.5);
        }
    }

    #[test]
    fn encodage_des_formats() {
        let mut v = Vec::new();
        encode(0x7C4DFF, Format { bpp: 32, stride: 0 }, &mut v);
        assert_eq!(v, [0xFF, 0x4D, 0x7C, 0]);
        v.clear();
        encode(0xFFFFFF, Format { bpp: 16, stride: 0 }, &mut v);
        assert_eq!(v, [0xFF, 0xFF]);
    }

    #[test]
    fn image_ppm() {
        let img = render_ppm(320, 200, 0);
        assert!(img.starts_with(b"P6\n320 200\n255\n"));
        assert_eq!(img.len(), "P6\n320 200\n255\n".len() + 320 * 200 * 3);
    }

    #[test]
    fn framebuffer_fichier() {
        let dir = std::env::temp_dir().join(format!("winlux-splash-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("virtual_size"), "640,480\n").unwrap();
        fs::write(dir.join("bits_per_pixel"), "32\n").unwrap();
        fs::write(dir.join("stride"), "2560\n").unwrap();
        let dev = dir.join("fb");
        fs::write(&dev, vec![0u8; 2560 * 480]).unwrap();
        let mut fb = open_fb(&dev, &dir).expect("framebuffer");
        let l = fb.layout;
        assert!(fb.write_rect(0, 0, l.w, l.h, &|x, y| background_pixel(&l, x, y)));
        let bytes = fs::read(&dev).unwrap();
        let at = (l.logo_y + l.size / 4) as usize * 2560 + (l.w / 2 + 5) as usize * 4;
        assert!(bytes[at] > 0xC0, "bleu du cyan écrit dans le framebuffer");
        fs::remove_dir_all(dir).ok();
    }
}
