//! Extensions Winlux pour l'outil Capture du bureau (Win+Maj+S) :
//! lecture de l'écran, annotations (stylo, surligneur, flèche, texte),
//! export PNG et enregistrement vidéo de l'écran.
//!
//! Les images échangées avec l'invité sont des tampons 32 bits BGRX de haut
//! en bas, soit exactement les pixels d'une DIB section 32 bpp « top-down ».

use std::ffi::{c_char, c_int, c_void};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::gui::{self, Rect};
use super::lock;

/// Vue mutable sur un tampon BGRX fourni par l'invité.
struct Canvas<'a> {
    bytes: &'a mut [u8],
    width: usize,
    height: usize,
    stride: usize,
}

impl<'a> Canvas<'a> {
    unsafe fn from_raw(bits: *mut u8, width: i32, height: i32, stride: i32) -> Option<Self> {
        if bits.is_null() || width <= 0 || height <= 0 || stride < width.checked_mul(4)? {
            return None;
        }
        let len = (stride as usize).checked_mul(height as usize)?;
        if len > gui::MAX_BITMAP_PIXELS * 4 {
            return None;
        }
        Some(Self {
            bytes: std::slice::from_raw_parts_mut(bits, len),
            width: width as usize,
            height: height as usize,
            stride: stride as usize,
        })
    }

    fn blend(&mut self, x: usize, y: usize, color: u32, coverage: f32, mode: StrokeMode) {
        let o = y * self.stride + x * 4;
        let px = &mut self.bytes[o..o + 3];
        // COLORREF 0x00BBGGRR → canaux B, G, R du tampon.
        let src = [
            ((color >> 16) & 0xFF) as f32,
            ((color >> 8) & 0xFF) as f32,
            (color & 0xFF) as f32,
        ];
        for (i, channel) in px.iter_mut().enumerate() {
            let dst = *channel as f32;
            let target = match mode {
                StrokeMode::Pen | StrokeMode::Arrow => src[i],
                // Surligneur : multiplication, le texte sombre reste lisible
                // et repasser sur la même zone n'assombrit pas davantage.
                // (plus une part de couleur pour rester visible sur fond sombre)
                StrokeMode::Highlighter => dst * src[i] / 255.0 * 0.6 + src[i] * 0.4,
            };
            *channel = (dst + (target - dst) * coverage).round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeMode {
    Pen,
    Highlighter,
    Arrow,
}

impl StrokeMode {
    fn from_raw(mode: u32) -> Option<Self> {
        Some(match mode {
            0 => Self::Pen,
            1 => Self::Highlighter,
            2 => Self::Arrow,
            _ => return None,
        })
    }
}

fn segment_distance(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

fn inside_triangle(p: (f32, f32), t: &[(f32, f32); 3]) -> bool {
    let sign = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        (a.0 - c.0) * (b.1 - c.1) - (b.0 - c.0) * (a.1 - c.1)
    };
    let d1 = sign(p, t[0], t[1]);
    let d2 = sign(p, t[1], t[2]);
    let d3 = sign(p, t[2], t[0]);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// Trace une polyligne antialiasée (bouts ronds). La couverture de tout le
/// trait est calculée avant le mélange, pour qu'un surligneur ne se cumule
/// pas sur ses propres recouvrements.
fn stroke(canvas: &mut Canvas, points: &[(f32, f32)], color: u32, width: f32, mode: StrokeMode) {
    if points.is_empty() || width <= 0.0 {
        return;
    }
    let r = width / 2.0;
    let mut segments: Vec<((f32, f32), (f32, f32))> = if points.len() == 1 {
        vec![(points[0], points[0])]
    } else {
        points.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let mut head: Option<[(f32, f32); 3]> = None;
    if mode == StrokeMode::Arrow && points.len() >= 2 {
        let tip = points[points.len() - 1];
        let from = points[0];
        let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len > 1.0 {
            let (ux, uy) = (dx / len, dy / len);
            let size = (width * 4.0).max(12.0).min(len);
            let base = (tip.0 - ux * size, tip.1 - uy * size);
            let half = size * 0.55;
            head = Some([
                tip,
                (base.0 - uy * half, base.1 + ux * half),
                (base.0 + uy * half, base.1 - ux * half),
            ]);
            // Le trait s'arrête sous la pointe pour garder une pointe nette.
            let shaft_end = (tip.0 - ux * size * 0.6, tip.1 - uy * size * 0.6);
            segments = vec![(from, shaft_end)];
        }
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let all = points.iter().chain(head.iter().flat_map(|t| t.iter()));
    for p in all {
        min_x = min_x.min(p.0);
        min_y = min_y.min(p.1);
        max_x = max_x.max(p.0);
        max_y = max_y.max(p.1);
    }
    let x0 = (min_x - r - 1.0).floor().max(0.0) as usize;
    let y0 = (min_y - r - 1.0).floor().max(0.0) as usize;
    let x1 = ((max_x + r + 2.0).ceil().max(0.0) as usize).min(canvas.width);
    let y1 = ((max_y + r + 2.0).ceil().max(0.0) as usize).min(canvas.height);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let alpha = match mode {
        StrokeMode::Highlighter => 0.85,
        _ => 1.0,
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut d = f32::MAX;
            for (a, b) in &segments {
                d = d.min(segment_distance(px, py, *a, *b));
                if d <= 0.0 {
                    break;
                }
            }
            let mut coverage = (r + 0.5 - d).clamp(0.0, 1.0);
            if let Some(t) = head.as_ref() {
                // Pointe : suréchantillonnage 4×4.
                let mut hits = 0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let p = (
                            x as f32 + 0.125 + sx as f32 * 0.25,
                            y as f32 + 0.125 + sy as f32 * 0.25,
                        );
                        if inside_triangle(p, t) {
                            hits += 1;
                        }
                    }
                }
                coverage = coverage.max(hits as f32 / 16.0);
            }
            if coverage > 0.0 {
                canvas.blend(x, y, color, coverage * alpha, mode);
            }
        }
    }
}

/// Lit une zone de l'écran dans un tampon BGRX. Renvoie 0 sans affichage.
pub unsafe extern "win64" fn winlux_capture_screen(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    bits: *mut u8,
    stride: i32,
) -> i32 {
    let Some(canvas) = Canvas::from_raw(bits, width, height, stride) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let Some(snapshot) = gui::capture_screen(Rect::from_size(x, y, width, height)) else {
        crate::process::set_last_error(1400);
        return 0;
    };
    let w = snapshot.width.min(canvas.width);
    let h = snapshot.height.min(canvas.height);
    for row in 0..h {
        for col in 0..w {
            let c = snapshot.pixels[row * snapshot.width + col];
            let o = row * canvas.stride + col * 4;
            canvas.bytes[o] = ((c >> 16) & 0xFF) as u8;
            canvas.bytes[o + 1] = ((c >> 8) & 0xFF) as u8;
            canvas.bytes[o + 2] = (c & 0xFF) as u8;
            canvas.bytes[o + 3] = 0;
        }
    }
    1
}

/// Trace un trait d'annotation : `points` contient `count` couples (x, y).
/// `mode` : 0 stylo, 1 surligneur, 2 flèche (du premier au dernier point).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn winlux_draw_stroke(
    bits: *mut u8,
    width: i32,
    height: i32,
    stride: i32,
    points: *const i32,
    count: i32,
    color: u32,
    pen: i32,
    mode: u32,
) -> i32 {
    let (Some(mut canvas), Some(mode)) = (
        Canvas::from_raw(bits, width, height, stride),
        StrokeMode::from_raw(mode),
    ) else {
        crate::process::set_last_error(87);
        return 0;
    };
    if points.is_null() || !(1..=1_000_000).contains(&count) || pen <= 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let raw = std::slice::from_raw_parts(points, count as usize * 2);
    let pts: Vec<(f32, f32)> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| (p[0] as f32 + 0.5, p[1] as f32 + 0.5))
        .collect();
    stroke(&mut canvas, &pts, color, pen as f32, mode);
    1
}

type CairoSurfaceCreate = unsafe extern "C" fn(*mut u8, c_int, c_int, c_int, c_int) -> *mut c_void;

/// Écrit un texte antialiasé (police DejaVu Sans, gras) avec cairo.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn winlux_draw_text(
    bits: *mut u8,
    width: i32,
    height: i32,
    stride: i32,
    x: i32,
    y: i32,
    text: *const u8,
    size: i32,
    color: u32,
) -> i32 {
    let Some(_) = Canvas::from_raw(bits, width, height, stride) else {
        crate::process::set_last_error(87);
        return 0;
    };
    if text.is_null() || size <= 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let text = super::cstr(text);
    let Ok(text) = std::ffi::CString::new(text) else {
        return 0;
    };
    let library = libc::dlopen(c"libcairo.so.2".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
    if library.is_null() {
        crate::process::set_last_error(126); // ERROR_MOD_NOT_FOUND
        return 0;
    }
    macro_rules! sym {
        ($name:literal, $ty:ty) => {{
            let p = libc::dlsym(library, concat!($name, "\0").as_ptr().cast());
            if p.is_null() {
                libc::dlclose(library);
                return 0;
            }
            std::mem::transmute::<*mut c_void, $ty>(p)
        }};
    }
    let create_surface = sym!("cairo_image_surface_create_for_data", CairoSurfaceCreate);
    let create = sym!(
        "cairo_create",
        unsafe extern "C" fn(*mut c_void) -> *mut c_void
    );
    let destroy = sym!("cairo_destroy", unsafe extern "C" fn(*mut c_void));
    let surface_destroy = sym!("cairo_surface_destroy", unsafe extern "C" fn(*mut c_void));
    let surface_flush = sym!("cairo_surface_flush", unsafe extern "C" fn(*mut c_void));
    let select_font = sym!(
        "cairo_select_font_face",
        unsafe extern "C" fn(*mut c_void, *const c_char, c_int, c_int)
    );
    let font_size = sym!(
        "cairo_set_font_size",
        unsafe extern "C" fn(*mut c_void, f64)
    );
    let source = sym!(
        "cairo_set_source_rgb",
        unsafe extern "C" fn(*mut c_void, f64, f64, f64)
    );
    let move_to = sym!("cairo_move_to", unsafe extern "C" fn(*mut c_void, f64, f64));
    let show_text = sym!(
        "cairo_show_text",
        unsafe extern "C" fn(*mut c_void, *const c_char)
    );
    let status = sym!("cairo_status", unsafe extern "C" fn(*mut c_void) -> c_int);
    // CAIRO_FORMAT_RGB24 = 1 : 32 bits natifs, octets B, G, R, X sur x86.
    let surface = create_surface(bits, 1, width, height, stride);
    let cr = create(surface);
    let ok = status(cr) == 0;
    if ok {
        select_font(cr, c"DejaVu Sans".as_ptr(), 0, 1);
        font_size(cr, size as f64);
        source(
            cr,
            (color & 0xFF) as f64 / 255.0,
            ((color >> 8) & 0xFF) as f64 / 255.0,
            ((color >> 16) & 0xFF) as f64 / 255.0,
        );
        // `y` est le haut du texte, cairo attend la ligne de base.
        move_to(cr, x as f64, y as f64 + size as f64 * 0.8);
        show_text(cr, text.as_ptr());
    }
    let ok = ok && status(cr) == 0;
    destroy(cr);
    surface_flush(surface);
    surface_destroy(surface);
    libc::dlclose(library);
    i32::from(ok)
}

/// Encode un tampon BGRX en PNG RVB. `path` est un chemin Windows ANSI.
pub unsafe extern "win64" fn winlux_save_png(
    path: *const u8,
    bits: *const u8,
    width: i32,
    height: i32,
    stride: i32,
) -> i32 {
    let Some(canvas) = Canvas::from_raw(bits.cast_mut(), width, height, stride) else {
        crate::process::set_last_error(87);
        return 0;
    };
    if path.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let target = crate::paths::to_linux(&super::cstr(path));
    match write_png(&target, &canvas) {
        Ok(()) => 1,
        Err(_) => {
            crate::process::set_last_error(5);
            0
        }
    }
}

fn write_png(target: &Path, canvas: &Canvas) -> std::io::Result<()> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = std::fs::File::create(target)?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        canvas.width as u32,
        canvas.height as u32,
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    let mut rgb = Vec::with_capacity(canvas.width * canvas.height * 3);
    for row in 0..canvas.height {
        let line = &canvas.bytes[row * canvas.stride..row * canvas.stride + canvas.width * 4];
        for px in line.as_chunks::<4>().0 {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
    }
    writer
        .write_image_data(&rgb)
        .map_err(std::io::Error::other)?;
    writer.finish().map_err(std::io::Error::other)
}

// ---------- Enregistrement vidéo de l'écran ----------

pub const RECORD_MIC: u32 = 1;
pub const RECORD_SYSTEM_AUDIO: u32 = 2;
pub const RECORD_WEBM: u32 = 4;

/// Source audio disponible sur la session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioServer {
    /// PulseAudio ou PipeWire (pipewire-pulse) : micro et son système.
    Pulse,
    /// ALSA seul : micro uniquement.
    Alsa,
    None,
}

fn detect_audio() -> AudioServer {
    let pulse_socket = std::env::var_os("PULSE_SERVER").is_some()
        || std::env::var_os("XDG_RUNTIME_DIR")
            .map(|d| PathBuf::from(d).join("pulse/native").exists())
            .unwrap_or(false);
    if pulse_socket {
        AudioServer::Pulse
    } else if Path::new("/dev/snd").exists() {
        AudioServer::Alsa
    } else {
        AudioServer::None
    }
}

/// Arguments ffmpeg d'un enregistrement (fonction pure, testée).
pub fn record_args(
    display: &str,
    area: Rect,
    flags: u32,
    audio: AudioServer,
    output: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-y"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // libx264 et yuv420p imposent des dimensions paires.
    let width = (area.width().max(2)) & !1;
    let height = (area.height().max(2)) & !1;
    args.extend([
        "-thread_queue_size".to_string(),
        "512".to_string(),
        "-f".to_string(),
        "x11grab".to_string(),
        "-framerate".to_string(),
        "30".to_string(),
        "-draw_mouse".to_string(),
        "1".to_string(),
        "-video_size".to_string(),
        format!("{width}x{height}"),
        "-i".to_string(),
        format!("{display}+{},{}", area.left.max(0), area.top.max(0)),
    ]);
    let mut audio_inputs = 0;
    let mut push_audio = |args: &mut Vec<String>, format: &str, device: &str| {
        args.extend(
            ["-thread_queue_size", "512", "-f", format, "-i", device]
                .iter()
                .map(|s| s.to_string()),
        );
        audio_inputs += 1;
    };
    match audio {
        AudioServer::Pulse => {
            if flags & RECORD_MIC != 0 {
                push_audio(&mut args, "pulse", "default");
            }
            if flags & RECORD_SYSTEM_AUDIO != 0 {
                push_audio(&mut args, "pulse", "@DEFAULT_MONITOR@");
            }
        }
        AudioServer::Alsa => {
            if flags & RECORD_MIC != 0 {
                push_audio(&mut args, "alsa", "default");
            }
        }
        AudioServer::None => {}
    }
    if audio_inputs == 2 {
        args.extend(
            [
                "-filter_complex",
                "[1:a][2:a]amix=inputs=2:duration=longest[a]",
                "-map",
                "0:v",
                "-map",
                "[a]",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
    }
    let webm = flags & RECORD_WEBM != 0;
    let codec: &[&str] = if webm {
        &[
            "-c:v",
            "libvpx-vp9",
            "-deadline",
            "realtime",
            "-cpu-used",
            "8",
            "-row-mt",
            "1",
            "-b:v",
            "0",
            "-crf",
            "34",
            "-pix_fmt",
            "yuv420p",
        ]
    } else {
        &[
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
        ]
    };
    args.extend(codec.iter().map(|s| s.to_string()));
    if audio_inputs > 0 {
        let audio_codec: &[&str] = if webm {
            &["-c:a", "libopus", "-b:a", "128k"]
        } else {
            &["-c:a", "aac", "-b:a", "160k"]
        };
        args.extend(audio_codec.iter().map(|s| s.to_string()));
    }
    args.push(output.to_string_lossy().into_owned());
    args
}

struct Recording {
    child: Child,
    started: Instant,
}

static RECORDING: Mutex<Option<Recording>> = Mutex::new(None);

fn ffmpeg_binary() -> PathBuf {
    std::env::var_os("WINLUX_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

/// Démarre l'enregistrement d'une zone de l'écran (`flags` : RECORD_*).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn winlux_record_start(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    flags: u32,
    path: *const u8,
) -> i32 {
    if path.is_null() || width < 2 || height < 2 {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut slot = lock(&RECORDING);
    if let Some(rec) = slot.as_mut() {
        if matches!(rec.child.try_wait(), Ok(None)) {
            crate::process::set_last_error(170); // ERROR_BUSY
            return 0;
        }
    }
    let Some(display) = std::env::var_os("DISPLAY") else {
        crate::process::set_last_error(1400);
        return 0;
    };
    let output = crate::paths::to_linux(&super::cstr(path));
    if let Some(dir) = output.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let args = record_args(
        &display.to_string_lossy(),
        Rect::from_size(x, y, width, height),
        flags,
        detect_audio(),
        &output,
    );
    let log = crate::paths::data_dir()
        .map(|d| d.join("screen-recorder.log"))
        .and_then(|p| {
            let _ = std::fs::create_dir_all(p.parent()?);
            std::fs::File::create(p).ok()
        });
    let stderr = log.map(Stdio::from).unwrap_or_else(Stdio::null);
    match Command::new(ffmpeg_binary())
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
    {
        Ok(child) => {
            *slot = Some(Recording {
                child,
                started: Instant::now(),
            });
            1
        }
        Err(_) => {
            crate::process::set_last_error(2); // ffmpeg introuvable
            0
        }
    }
}

/// Arrête proprement l'enregistrement (ffmpeg finalise le fichier).
pub extern "win64" fn winlux_record_stop() -> i32 {
    let Some(mut rec) = lock(&RECORDING).take() else {
        return 0;
    };
    if let Some(mut stdin) = rec.child.stdin.take() {
        let _ = stdin.write_all(b"q");
        let _ = stdin.flush();
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match rec.child.try_wait() {
            Ok(Some(status)) => return i32::from(status.success()),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => break,
        }
    }
    unsafe {
        libc::kill(rec.child.id() as i32, libc::SIGINT);
    }
    std::thread::sleep(Duration::from_millis(500));
    let _ = rec.child.kill();
    let _ = rec.child.wait();
    0
}

/// 0 : aucun enregistrement, 1 : en cours, 2 : ffmpeg s'est arrêté tout seul.
/// `elapsed_ms` reçoit la durée écoulée.
pub unsafe extern "win64" fn winlux_record_state(elapsed_ms: *mut u32) -> i32 {
    let mut slot = lock(&RECORDING);
    let Some(rec) = slot.as_mut() else {
        return 0;
    };
    if !elapsed_ms.is_null() {
        *elapsed_ms = rec.started.elapsed().as_millis().min(u32::MAX as u128) as u32;
    }
    match rec.child.try_wait() {
        Ok(None) => 1,
        _ => {
            *slot = None;
            2
        }
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxCaptureScreen" => fn winlux_capture_screen,
        "WinluxDrawStroke" => fn winlux_draw_stroke,
        "WinluxDrawText" => fn winlux_draw_text,
        "WinluxSavePng" => fn winlux_save_png,
        "WinluxRecordStart" => fn winlux_record_start,
        "WinluxRecordStop" => fn winlux_record_stop,
        "WinluxRecordState" => fn winlux_record_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(w: usize, h: usize) -> Vec<u8> {
        vec![255; w * h * 4]
    }

    fn px(buf: &[u8], w: usize, x: usize, y: usize) -> [u8; 3] {
        let o = (y * w + x) * 4;
        [buf[o], buf[o + 1], buf[o + 2]]
    }

    #[test]
    fn stylo_trace_rouge_opaque_et_antialiase() {
        let mut buf = white(40, 20);
        let pts = [2, 10, 37, 10];
        let ok = unsafe {
            winlux_draw_stroke(
                buf.as_mut_ptr(),
                40,
                20,
                160,
                pts.as_ptr(),
                2,
                0x0000FF,
                4,
                0,
            )
        };
        assert_eq!(ok, 1);
        assert_eq!(px(&buf, 40, 20, 10), [0, 0, 255]); // BGR : rouge
        assert_eq!(px(&buf, 40, 20, 2), [255, 255, 255]);
        let edge = px(&buf, 40, 20, 12);
        assert!(edge[0] > 0 && edge[0] < 255, "bord antialiasé : {edge:?}");
    }

    #[test]
    fn surligneur_multiplie_sans_cumul() {
        let mut buf = white(30, 10);
        // Le trait revient sur lui-même : la couleur ne doit pas s'assombrir.
        let pts = [2, 5, 27, 5, 2, 5];
        unsafe {
            winlux_draw_stroke(
                buf.as_mut_ptr(),
                30,
                10,
                120,
                pts.as_ptr(),
                3,
                0x00FFFF,
                6,
                1,
            );
        }
        let p = px(&buf, 30, 15, 5);
        // Jaune (R=G=255, B bas) sur blanc.
        assert_eq!(p[1], 255);
        assert_eq!(p[2], 255);
        assert!(p[0] < 60, "{p:?}");
        // Sur du noir, le surligneur reste visible mais sombre.
        let mut dark = vec![0u8; 30 * 10 * 4];
        unsafe {
            winlux_draw_stroke(
                dark.as_mut_ptr(),
                30,
                10,
                120,
                pts.as_ptr(),
                3,
                0x00FFFF,
                6,
                1,
            );
        }
        let d = px(&dark, 30, 15, 5);
        assert_eq!(d[0], 0);
        assert!((70..110).contains(&d[2]) && d[1] == d[2], "{d:?}");
    }

    #[test]
    fn fleche_a_une_pointe_pleine() {
        let mut buf = white(60, 30);
        let pts = [4, 15, 56, 15];
        unsafe {
            winlux_draw_stroke(buf.as_mut_ptr(), 60, 30, 240, pts.as_ptr(), 2, 0, 3, 2);
        }
        // Près de la pointe, la tête est plus large que le trait.
        assert_eq!(px(&buf, 60, 48, 18), [0, 0, 0]);
        assert_eq!(px(&buf, 60, 20, 19), [255, 255, 255]);
    }

    #[test]
    fn tampon_invalide_refuse() {
        let pts = [0, 0];
        let r = unsafe {
            winlux_draw_stroke(std::ptr::null_mut(), 10, 10, 40, pts.as_ptr(), 1, 0, 2, 0)
        };
        assert_eq!(r, 0);
        let mut small = white(4, 4);
        let r =
            unsafe { winlux_draw_stroke(small.as_mut_ptr(), 4, 4, 8, pts.as_ptr(), 1, 0, 2, 0) };
        assert_eq!(r, 0, "pas plus petit que 4 octets par pixel");
    }

    #[test]
    fn export_png_relu_a_l_identique() {
        let dir = std::env::temp_dir().join(format!("winlux-png-{}", std::process::id()));
        let path = dir.join("capture.png");
        let mut buf = white(3, 2);
        buf[0..4].copy_from_slice(&[0x10, 0x20, 0x30, 0]); // B G R X
        let canvas = Canvas {
            bytes: &mut buf,
            width: 3,
            height: 2,
            stride: 12,
        };
        write_png(&path, &canvas).unwrap();
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut out = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut out).unwrap();
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(&out[0..3], &[0x30, 0x20, 0x10]);
        assert_eq!(&out[3..6], &[255, 255, 255]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn arguments_ffmpeg_mp4_avec_micro_et_son_systeme() {
        let args = record_args(
            ":0",
            Rect::from_size(10, 20, 801, 601),
            RECORD_MIC | RECORD_SYSTEM_AUDIO,
            AudioServer::Pulse,
            Path::new("/home/u/Vidéos/a.mp4"),
        );
        let joined = args.join(" ");
        assert!(joined.contains("-f x11grab"));
        assert!(joined.contains("-video_size 800x600"), "{joined}");
        assert!(joined.contains("-i :0+10,20"));
        assert!(joined.contains("-f pulse -i default"));
        assert!(joined.contains("-f pulse -i @DEFAULT_MONITOR@"));
        assert!(joined.contains("amix=inputs=2"));
        assert!(joined.contains("-c:v libx264"));
        assert!(joined.contains("-c:a aac"));
        assert_eq!(args.last().unwrap(), "/home/u/Vidéos/a.mp4");
    }

    #[test]
    fn arguments_ffmpeg_webm_sans_audio_et_alsa_sans_son_systeme() {
        let args = record_args(
            ":1",
            Rect::from_size(0, 0, 1920, 1080),
            RECORD_WEBM,
            AudioServer::Pulse,
            Path::new("/tmp/b.webm"),
        );
        let joined = args.join(" ");
        assert!(joined.contains("-c:v libvpx-vp9"));
        assert!(!joined.contains("-c:a"));
        assert!(!joined.contains("pulse"));
        let alsa = record_args(
            ":0",
            Rect::from_size(0, 0, 640, 480),
            RECORD_MIC | RECORD_SYSTEM_AUDIO | RECORD_WEBM,
            AudioServer::Alsa,
            Path::new("/tmp/c.webm"),
        )
        .join(" ");
        assert!(alsa.contains("-f alsa -i default"));
        assert!(!alsa.contains("amix"));
        assert!(alsa.contains("-c:a libopus"));
    }
}
