//! Moteur multimédia de Winlux : sonde, lecture vidéo/audio, vignettes et
//! reprise de lecture, exposés au shell par des exports privés `WinluxMedia*`.
//!
//! Le décodage est confié au moteur FFmpeg du système Linux (`ffmpeg` et
//! `ffprobe`, lancés comme processus séparés), de la même manière que
//! `PlaySound` s'appuie sur mpg123/FAAD. Ce choix donne d'un coup tous les
//! conteneurs et codecs courants ainsi que le décodage matériel (VA-API,
//! NVDEC/CUDA), et isole le shell : un fichier piégé ou un décodeur qui
//! plante ne fait tomber que le processus de décodage. Aucune DLL Windows
//! ni couche Wine n'intervient.
//!
//! Garde-fous :
//! - un format n'est annoncé lisible que si le décodeur existe vraiment sur la
//!   machine (`ffmpeg -decoders`) ;
//! - le décodage matériel est tenté seulement si un périphérique existe, puis
//!   on se replie sur le logiciel, puis sur la vidéo sans son si la sortie
//!   audio est indisponible ;
//! - moteur absent, fichier illisible ou délai dépassé : erreur propre, jamais
//!   de plantage du shell.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Taille maximale d'une image décodée transmise au shell (4K).
const MAX_FRAME_W: u32 = 3840;
const MAX_FRAME_H: u32 = 2160;
/// Taille maximale d'une vignette.
const MAX_THUMB: u32 = 512;
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const THUMB_TIMEOUT: Duration = Duration::from_secs(15);
const THUMB_WORKERS: usize = 2;
const THUMB_MEMORY_ENTRIES: usize = 384;
const RESUME_MAX_ENTRIES: usize = 500;

// ---------------------------------------------------------------------------
// Catalogue des formats
// ---------------------------------------------------------------------------

/// Codecs vidéo suivis : nom affiché, décodeurs FFmpeg acceptés.
pub const VIDEO_CODECS: &[(&str, &[&str])] = &[
    ("H.264 / AVC", &["h264", "h264_cuvid", "h264_qsv"]),
    ("H.265 / HEVC", &["hevc", "hevc_cuvid", "hevc_qsv"]),
    (
        "AV1",
        &["libdav1d", "av1", "libaom-av1", "av1_cuvid", "av1_qsv"],
    ),
    ("VP8", &["vp8", "libvpx"]),
    ("VP9", &["vp9", "libvpx-vp9"]),
    ("MPEG-2", &["mpeg2video", "mpegvideo"]),
    ("MPEG-4 ASP (DivX, Xvid)", &["mpeg4"]),
    ("VC-1 / WMV", &["vc1", "wmv3", "wmv2", "wmv1"]),
    ("Theora", &["theora"]),
    ("Apple ProRes", &["prores"]),
    ("Avid DNxHD / DNxHR", &["dnxhd"]),
    ("H.263", &["h263"]),
    ("Motion JPEG", &["mjpeg"]),
];

/// Codecs audio suivis.
pub const AUDIO_CODECS: &[(&str, &[&str])] = &[
    ("AAC", &["aac", "aac_fixed", "libfdk_aac"]),
    ("MP3", &["mp3float", "mp3"]),
    ("Opus", &["opus", "libopus"]),
    ("Vorbis", &["vorbis", "libvorbis"]),
    ("FLAC", &["flac"]),
    ("ALAC", &["alac"]),
    ("WAV / PCM", &["pcm_s16le", "pcm_s24le", "pcm_f32le"]),
    ("Dolby Digital (AC-3)", &["ac3", "ac3_fixed"]),
    ("Dolby Digital Plus (E-AC-3)", &["eac3"]),
    ("DTS", &["dca"]),
    ("Dolby TrueHD", &["truehd"]),
    ("WMA", &["wmav2", "wmav1", "wmapro", "wmalossless"]),
    ("Monkey's Audio (APE)", &["ape"]),
    ("WavPack", &["wavpack"]),
    ("AMR", &["amrnb", "amrwb", "libopencore_amrnb"]),
];

/// Extensions de conteneurs vidéo reconnues par le lecteur et l'Explorer.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mkv", "webm", "avi", "mov", "qt", "flv", "f4v", "wmv", "asf", "mpeg", "mpg",
    "mpe", "m1v", "m2v", "vob", "ts", "m2ts", "mts", "m2t", "ogv", "3gp", "3g2", "mxf", "divx",
    "rm", "rmvb", "dv", "y4m",
];

/// Extensions de conteneurs audio.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "aac", "m4a", "m4b", "flac", "alac", "ogg", "oga", "opus", "wav", "wave", "wma", "ape",
    "ac3", "eac3", "dts", "mka", "wv", "aif", "aiff", "aifc", "caf", "amr", "au", "mp2", "mpc",
    "tta", "thd",
];

/// Extensions de sous-titres externes cherchées à côté d'une vidéo.
pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "sup"];

pub fn friendly_codec(codec: &str) -> String {
    let all = VIDEO_CODECS.iter().chain(AUDIO_CODECS.iter());
    for (label, names) in all {
        if names.contains(&codec) {
            return (*label).to_string();
        }
    }
    let label = match codec {
        "subrip" | "srt" => "SRT",
        "ass" | "ssa" => "ASS/SSA",
        "webvtt" => "WebVTT",
        "mov_text" => "MP4 Timed Text",
        "hdmv_pgs_subtitle" => "PGS (Blu-ray)",
        "dvd_subtitle" => "VobSub (DVD)",
        "dvb_subtitle" => "DVB",
        "pcm_s16be" | "pcm_s24be" | "pcm_u8" | "pcm_s32le" | "pcm_f64le" | "pcm_alaw"
        | "pcm_mulaw" => "WAV / PCM",
        "mpeg1video" => "MPEG-1",
        "wmav2" => "WMA",
        _ => return codec.to_ascii_uppercase(),
    };
    label.to_string()
}

fn is_bitmap_subtitle(codec: &str) -> bool {
    matches!(
        codec,
        "hdmv_pgs_subtitle" | "dvd_subtitle" | "dvb_subtitle" | "xsub"
    )
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// 1 = audio, 2 = vidéo, 3 = sous-titres, 0 = autre.
pub fn media_kind(path: &Path) -> u32 {
    let e = ext_of(path);
    if AUDIO_EXTENSIONS.contains(&e.as_str()) {
        1
    } else if VIDEO_EXTENSIONS.contains(&e.as_str()) {
        2
    } else if SUBTITLE_EXTENSIONS.contains(&e.as_str()) {
        3
    } else {
        0
    }
}

/// Tri « naturel » : « Épisode 2 » avant « Épisode 10 ».
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    m.push(d);
                    y.next();
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let o = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                if c != d {
                    return c.cmp(&d);
                }
                x.next();
                y.next();
            }
        }
    }
}

/// Fichiers audio/vidéo du même dossier (liste de lecture implicite).
pub fn siblings(path: &Path) -> Vec<PathBuf> {
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && matches!(media_kind(p), 1 | 2))
        .take(2000)
        .collect();
    v.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    v
}

/// Sous-titres externes d'une vidéo : `film.srt`, `film.fr.srt`, `film.eng.ass`…
pub fn sidecar_subtitles(path: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (path.parent(), path.file_stem()) else {
        return Vec::new();
    };
    let stem = stem.to_string_lossy().to_lowercase();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            media_kind(p) == 3
                && p.is_file()
                && name.starts_with(&stem)
                && name[stem.len()..].starts_with('.')
        })
        .take(32)
        .collect();
    v.sort();
    v
}

// ---------------------------------------------------------------------------
// Outils et capacités
// ---------------------------------------------------------------------------

fn find_tool(name: &str) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("WINLUX_MEDIA_DIR") {
        let p = PathBuf::from(dir).join(name);
        return p.is_file().then_some(p);
    }
    let fixed = Path::new("/usr/lib/winlux/media").join(name);
    if fixed.is_file() {
        return Some(fixed);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// Capacités du moteur détectées une seule fois par processus.
#[derive(Debug, Default, Clone)]
pub struct Caps {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub decoders: HashSet<String>,
    pub hwaccels: Vec<String>,
    pub render_node: Option<PathBuf>,
    pub nvidia: bool,
}

impl Caps {
    pub fn engine(&self) -> bool {
        self.ffmpeg.is_some() && self.ffprobe.is_some()
    }
    pub fn has_decoder(&self, codec: &str) -> bool {
        if self.decoders.contains(codec) {
            return true;
        }
        // Les noms de codec de ffprobe ne sont pas toujours ceux des décodeurs.
        let family = VIDEO_CODECS
            .iter()
            .chain(AUDIO_CODECS.iter())
            .find(|(_, names)| names.contains(&codec));
        match family {
            Some((_, names)) => names.iter().any(|n| self.decoders.contains(*n)),
            None => false,
        }
    }
    /// Arguments `-hwaccel` choisis selon la politique et le matériel présent.
    pub fn hw_args(&self, policy: &str) -> Vec<String> {
        let has = |n: &str| self.hwaccels.iter().any(|h| h == n);
        let cuda = || {
            (self.nvidia && has("cuda")).then(|| vec!["-hwaccel".to_string(), "cuda".to_string()])
        };
        let vaapi = || {
            let node = self.render_node.as_ref()?;
            has("vaapi").then(|| {
                vec![
                    "-hwaccel".to_string(),
                    "vaapi".to_string(),
                    "-hwaccel_device".to_string(),
                    node.to_string_lossy().into_owned(),
                ]
            })
        };
        let vdpau = || {
            (self.nvidia && has("vdpau")).then(|| vec!["-hwaccel".to_string(), "vdpau".to_string()])
        };
        let chosen = match policy {
            "off" | "none" | "software" => None,
            "cuda" | "nvdec" => cuda(),
            "vaapi" => vaapi(),
            "vdpau" => vdpau(),
            _ => cuda().or_else(vaapi).or_else(vdpau),
        };
        chosen.unwrap_or_default()
    }
    pub fn hw_label(&self) -> &'static str {
        let args = self.hw_args(&hw_policy());
        match args.get(1).map(String::as_str) {
            Some("cuda") => "NVDEC (CUDA)",
            Some("vaapi") => "VA-API",
            Some("vdpau") => "VDPAU",
            _ => "",
        }
    }
}

fn hw_policy() -> String {
    std::env::var("WINLUX_MEDIA_HWACCEL").unwrap_or_else(|_| "auto".to_string())
}

/// Analyse la sortie de `ffmpeg -decoders`.
pub fn parse_decoders(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut in_list = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("------") {
            in_list = true;
            continue;
        }
        if !in_list {
            continue;
        }
        let mut parts = t.split_whitespace();
        let (Some(flags), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if flags.len() == 6 && matches!(flags.as_bytes()[0], b'V' | b'A' | b'S') {
            out.insert(name.to_string());
        }
    }
    out
}

/// Analyse la sortie de `ffmpeg -hwaccels`.
pub fn parse_hwaccels(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|l| !l.starts_with("Hardware acceleration methods"))
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Le processus de décodage meurt avec le runtime (fenêtre tuée, plantage) :
/// jamais de ffmpeg orphelin qui continuerait à jouer du son.
fn die_with_parent(cmd: &mut Command) -> &mut Command {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        })
    }
}

fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Option<Vec<u8>> {
    die_with_parent(cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout
            .by_ref()
            .take(256 * 1024 * 1024)
            .read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let out = reader.join().ok()?;
    status.filter(|s| s.success()).map(|_| out)
}

pub fn caps() -> &'static Caps {
    static CAPS: OnceLock<Caps> = OnceLock::new();
    CAPS.get_or_init(|| {
        let mut c = Caps {
            ffmpeg: find_tool("ffmpeg"),
            ffprobe: find_tool("ffprobe"),
            ..Caps::default()
        };
        if let Some(ff) = &c.ffmpeg {
            let t = Duration::from_secs(5);
            if let Some(out) =
                run_with_timeout(Command::new(ff).args(["-hide_banner", "-decoders"]), t)
            {
                c.decoders = parse_decoders(&String::from_utf8_lossy(&out));
            }
            if let Some(out) =
                run_with_timeout(Command::new(ff).args(["-hide_banner", "-hwaccels"]), t)
            {
                c.hwaccels = parse_hwaccels(&String::from_utf8_lossy(&out));
            }
        }
        c.render_node = (128..136)
            .map(|n| PathBuf::from(format!("/dev/dri/renderD{n}")))
            .find(|p| p.exists());
        c.nvidia = Path::new("/dev/nvidia0").exists() || Path::new("/dev/nvidiactl").exists();
        c
    })
}

/// Rapport lisible par le shell : moteur, accélération et formats.
pub fn formats_report(c: &Caps) -> String {
    let mut s = String::new();
    s.push_str(&format!("engine\t{}\n", u8::from(c.engine())));
    s.push_str(&format!("hw\t{}\n", c.hw_label()));
    for (label, names) in VIDEO_CODECS {
        let ok = names.iter().any(|n| c.decoders.contains(*n));
        s.push_str(&format!("video\t{label}\t{}\n", u8::from(ok)));
    }
    for (label, names) in AUDIO_CODECS {
        let ok = names.iter().any(|n| c.decoders.contains(*n));
        s.push_str(&format!("audio\t{label}\t{}\n", u8::from(ok)));
    }
    s
}

// ---------------------------------------------------------------------------
// Sonde
// ---------------------------------------------------------------------------

/// Analyse le format `-of flat` de ffprobe (`clé="valeur"`).
pub fn parse_flat(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let value = if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
            let inner = &v[1..v.len() - 1];
            let mut out = String::with_capacity(inner.len());
            let mut chars = inner.chars();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    match chars.next() {
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some(o) => out.push(o),
                        None => {}
                    }
                } else {
                    out.push(ch);
                }
            }
            out
        } else {
            v.to_string()
        };
        map.insert(k.trim().to_string(), value);
    }
    map
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamInfo {
    pub index: u32,
    pub kind: char,
    /// Rang parmi les flux du même type (`0:s:N`, `si=N`).
    pub ordinal: u32,
    pub codec: String,
    pub lang: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Rapport d'aspect des pixels (numérateur, dénominateur).
    pub sar: (u32, u32),
    pub channels: u32,
    pub fps_x100: u32,
    pub attached_pic: bool,
    pub default: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaInfo {
    pub duration_ms: u64,
    pub container: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub streams: Vec<StreamInfo>,
    pub chapters: Vec<(u64, String)>,
}

fn secs_to_ms(s: &str) -> Option<u64> {
    let v: f64 = s.trim().parse().ok()?;
    (v.is_finite() && v >= 0.0).then(|| (v * 1000.0).round() as u64)
}

fn ratio(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once([':', '/'])?;
    let a: u32 = a.parse().ok()?;
    let b: u32 = b.parse().ok()?;
    (a > 0 && b > 0).then_some((a, b))
}

impl MediaInfo {
    pub fn from_flat(map: &HashMap<String, String>) -> Self {
        let get = |k: &str| map.get(k).cloned().unwrap_or_default();
        let tag = |prefix: &str, name: &str| {
            map.get(&format!("{prefix}.tags.{name}"))
                .or_else(|| map.get(&format!("{prefix}.tags.{}", name.to_ascii_uppercase())))
                .cloned()
                .unwrap_or_default()
        };
        let mut info = MediaInfo {
            duration_ms: secs_to_ms(&get("format.duration")).unwrap_or(0),
            container: get("format.format_name"),
            title: tag("format", "title"),
            artist: tag("format", "artist"),
            album: tag("format", "album"),
            ..Default::default()
        };
        let mut ordinals: HashMap<char, u32> = HashMap::new();
        for i in 0..256 {
            let p = format!("streams.stream.{i}");
            let Some(kind) = map.get(&format!("{p}.codec_type")) else {
                break;
            };
            let kind = match kind.as_str() {
                "video" => 'v',
                "audio" => 'a',
                "subtitle" => 's',
                _ => continue,
            };
            let ord = ordinals.entry(kind).or_insert(0);
            let ordinal = *ord;
            *ord += 1;
            let num = |k: &str| get(&format!("{p}.{k}")).parse::<u32>().unwrap_or(0);
            let fps = ratio(&get(&format!("{p}.avg_frame_rate")))
                .or_else(|| ratio(&get(&format!("{p}.r_frame_rate"))))
                .map(|(a, b)| (a as u64 * 100 / b as u64) as u32)
                .unwrap_or(0);
            info.streams.push(StreamInfo {
                index: get(&format!("{p}.index")).parse().unwrap_or(i),
                kind,
                ordinal,
                codec: get(&format!("{p}.codec_name")),
                lang: tag(&p, "language"),
                title: tag(&p, "title"),
                width: num("width"),
                height: num("height"),
                sar: ratio(&get(&format!("{p}.sample_aspect_ratio"))).unwrap_or((1, 1)),
                channels: num("channels"),
                fps_x100: fps,
                attached_pic: num("disposition.attached_pic") == 1,
                default: num("disposition.default") == 1,
            });
        }
        for i in 0..1024 {
            let p = format!("chapters.chapter.{i}");
            let Some(start) = map.get(&format!("{p}.start_time")) else {
                break;
            };
            let title = tag(&p, "title");
            info.chapters.push((secs_to_ms(start).unwrap_or(0), title));
        }
        info
    }

    /// Flux vidéo principal (hors pochette d'album).
    pub fn main_video(&self) -> Option<&StreamInfo> {
        self.streams
            .iter()
            .find(|s| s.kind == 'v' && !s.attached_pic)
    }
    pub fn cover(&self) -> Option<&StreamInfo> {
        self.streams
            .iter()
            .find(|s| s.kind == 'v' && s.attached_pic)
    }
    pub fn audio(&self) -> impl Iterator<Item = &StreamInfo> {
        self.streams.iter().filter(|s| s.kind == 'a')
    }
    pub fn subtitles(&self) -> impl Iterator<Item = &StreamInfo> {
        self.streams.iter().filter(|s| s.kind == 's')
    }
    pub fn default_audio(&self, caps: &Caps) -> Option<&StreamInfo> {
        let mut decodable = self.audio().filter(|s| caps.has_decoder(&s.codec));
        let first = decodable.next()?;
        Some(
            self.audio()
                .filter(|s| caps.has_decoder(&s.codec))
                .find(|s| s.default)
                .unwrap_or(first),
        )
    }
    /// Lisible si la vidéo principale ou au moins une piste audio se décode.
    pub fn playable(&self, caps: &Caps) -> bool {
        match self.main_video() {
            Some(v) => caps.has_decoder(&v.codec),
            None => self.audio().any(|a| caps.has_decoder(&a.codec)),
        }
    }
    /// Codecs présents mais sans décodeur sur cette machine.
    pub fn missing(&self, caps: &Caps) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.streams {
            if s.kind == 's' || s.attached_pic || caps.has_decoder(&s.codec) {
                continue;
            }
            let name = friendly_codec(&s.codec);
            if !out.contains(&name) {
                out.push(name);
            }
        }
        out
    }

    /// Texte tabulé pour le shell (une ligne par élément).
    pub fn report(&self, caps: &Caps) -> String {
        let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
        let mut s = String::new();
        s.push_str(&format!("duration\t{}\n", self.duration_ms));
        s.push_str(&format!("container\t{}\n", clean(&self.container)));
        s.push_str(&format!("title\t{}\n", clean(&self.title)));
        s.push_str(&format!("artist\t{}\n", clean(&self.artist)));
        s.push_str(&format!("album\t{}\n", clean(&self.album)));
        s.push_str(&format!("playable\t{}\n", u8::from(self.playable(caps))));
        s.push_str(&format!("missing\t{}\n", self.missing(caps).join(", ")));
        s.push_str(&format!("cover\t{}\n", u8::from(self.cover().is_some())));
        if let Some(v) = self.main_video() {
            s.push_str(&format!(
                "video\t{}\t{}\t{}\t{}\t{}\n",
                friendly_codec(&v.codec),
                v.width,
                v.height,
                v.fps_x100,
                u8::from(caps.has_decoder(&v.codec))
            ));
        }
        for a in self.audio() {
            s.push_str(&format!(
                "audio\t{}\t{}\t{}\t{}\t{}\t{}\n",
                a.ordinal,
                friendly_codec(&a.codec),
                a.channels,
                clean(&a.lang),
                u8::from(caps.has_decoder(&a.codec)),
                clean(&a.title)
            ));
        }
        for t in self.subtitles() {
            s.push_str(&format!(
                "sub\t{}\t{}\t{}\t{}\t{}\n",
                t.ordinal,
                friendly_codec(&t.codec),
                clean(&t.lang),
                u8::from(t.default),
                clean(&t.title)
            ));
        }
        for (start, title) in &self.chapters {
            s.push_str(&format!("chapter\t{start}\t{}\n", clean(title)));
        }
        s
    }
}

fn file_stamp(path: &Path) -> Option<(u64, i64, i64)> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).ok()?;
    m.is_file().then(|| (m.len(), m.mtime(), m.mtime_nsec()))
}

/// Sonde un fichier (avec cache mémoire par chemin, taille et date).
pub fn probe(path: &Path) -> Option<Arc<MediaInfo>> {
    type ProbeCache = Mutex<HashMap<(PathBuf, (u64, i64, i64)), Arc<MediaInfo>>>;
    static CACHE: OnceLock<ProbeCache> = OnceLock::new();
    let stamp = file_stamp(path)?;
    let key = (path.to_path_buf(), stamp);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok()?.get(&key) {
        return Some(hit.clone());
    }
    let ffprobe = caps().ffprobe.as_ref()?;
    let out = run_with_timeout(
        Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-of",
                "flat",
                "-show_format",
                "-show_streams",
                "-show_chapters",
                "-i",
            ])
            .arg(path),
        PROBE_TIMEOUT,
    )?;
    let info = MediaInfo::from_flat(&parse_flat(&String::from_utf8_lossy(&out)));
    if info.streams.is_empty() {
        return None;
    }
    let info = Arc::new(info);
    let mut c = cache.lock().ok()?;
    if c.len() > 256 {
        c.clear();
    }
    c.insert(key, info.clone());
    Some(info)
}

// ---------------------------------------------------------------------------
// Lecture
// ---------------------------------------------------------------------------

/// Piste de sous-titres demandée.
#[derive(Debug, Clone, PartialEq)]
pub enum Subtitle {
    None,
    /// Piste intégrée : rang parmi les sous-titres, index absolu, bitmap ?
    Embedded {
        ordinal: u32,
        index: u32,
        bitmap: bool,
    },
    /// Fichier externe (.srt, .ass, .vtt…), déjà relié sous un nom sûr.
    External(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayPlan {
    pub input: PathBuf,
    pub start_ms: u64,
    pub video: Option<(u32, u32, u32)>, // (index, largeur, hauteur)
    pub audio: Option<u32>,
    pub volume_pct: u32,
    pub speed_pct: u32,
    pub subtitle: Subtitle,
    pub hw: Vec<String>,
    /// Sortie audio FFmpeg (`-f <format> <périphérique>`).
    pub audio_out: (String, String),
}

/// Chaîne `atempo` couvrant 0,25× à 4× (chaque étage accepte 0,5 à 2).
pub fn atempo_chain(speed_pct: u32) -> Vec<String> {
    let mut s = speed_pct.clamp(25, 400) as f64 / 100.0;
    let mut out = Vec::new();
    while s >= 2.0 {
        out.push("atempo=2".to_string());
        s /= 2.0;
    }
    while s <= 0.5 {
        out.push("atempo=0.5".to_string());
        s /= 0.5;
    }
    if (s - 1.0).abs() > 1e-6 {
        out.push(format!("atempo={s:.4}"));
    }
    out
}

/// Ligne de commande ffmpeg d'une session de lecture.
pub fn build_play_args(p: &PlayPlan) -> Vec<String> {
    let speed = p.speed_pct.clamp(25, 400) as f64 / 100.0;
    let mut a: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-nostats",
        "-stats_period",
        "0.1",
        "-progress",
        "pipe:2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if p.video.is_some() {
        a.extend(p.hw.iter().cloned());
    }
    a.push("-readrate".into());
    a.push(format!("{speed:.3}"));
    if p.start_ms > 0 {
        a.push("-ss".into());
        a.push(format!("{:.3}", p.start_ms as f64 / 1000.0));
    }
    a.push("-copyts".into());
    a.push("-i".into());
    a.push(p.input.to_string_lossy().into_owned());

    if let Some((index, w, h)) = p.video {
        let mut chain = format!("[0:{index}]");
        match &p.subtitle {
            Subtitle::Embedded {
                index: si,
                bitmap: true,
                ..
            } => {
                chain.push_str(&format!(
                    "[0:{si}]overlay=(main_w-overlay_w)/2:main_h-overlay_h,"
                ));
            }
            Subtitle::Embedded {
                ordinal,
                bitmap: false,
                ..
            } => {
                chain.push_str(&format!(
                    "subtitles=filename={}:si={ordinal},",
                    p.input.to_string_lossy()
                ));
            }
            Subtitle::External(path) => {
                chain.push_str(&format!("subtitles=filename={},", path.to_string_lossy()));
            }
            Subtitle::None => {}
        }
        chain.push_str(&format!(
            "setpts=(PTS-STARTPTS)/{speed:.4},scale=w={w}:h={h}:force_original_aspect_ratio=decrease:flags=bicubic,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,format=bgra[vout]"
        ));
        a.push("-filter_complex".into());
        a.push(chain);
        a.extend(
            [
                "-map", "[vout]", "-f", "rawvideo", "-pix_fmt", "bgra", "pipe:1",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
    }
    if let Some(index) = p.audio {
        let mut filters = vec!["asetpts=PTS-STARTPTS".to_string()];
        filters.extend(atempo_chain(p.speed_pct));
        filters.push(format!(
            "volume={:.2}",
            p.volume_pct.min(200) as f64 / 100.0
        ));
        a.push("-map".into());
        a.push(format!("0:{index}"));
        a.push("-af".into());
        a.push(filters.join(","));
        a.push("-f".into());
        a.push(p.audio_out.0.clone());
        a.push(p.audio_out.1.clone());
    }
    a
}

fn audio_output() -> Option<(String, String)> {
    if let Ok(v) = std::env::var("WINLUX_MEDIA_AUDIO") {
        if v == "none" {
            return None;
        }
        if let Some((f, d)) = v.split_once(':') {
            return Some((f.to_string(), d.to_string()));
        }
    }
    let pulse = std::env::var_os("PULSE_SERVER").is_some()
        || std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .is_some_and(|d| d.join("pulse/native").exists());
    Some(if pulse {
        ("pulse".to_string(), "default".to_string())
    } else {
        ("alsa".to_string(), "default".to_string())
    })
}

/// Taille de décodage : taille d'affichage du flux tenant dans la cible.
pub fn fit_size(src: (u32, u32), sar: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    let sw = (src.0 as u64 * sar.0 as u64 / sar.1.max(1) as u64).max(1);
    let sh = (src.1 as u64).max(1);
    let tw = target.0.clamp(16, MAX_FRAME_W) as u64;
    let th = target.1.clamp(16, MAX_FRAME_H) as u64;
    // Jamais plus grand que la source.
    let (tw, th) = (tw.min(sw.max(16)), th.min(sh.max(16)));
    let (w, h) = if sw * th > sh * tw {
        (tw, (tw * sh / sw).max(2))
    } else {
        ((th * sw / sh).max(2), th)
    };
    ((w as u32) & !1, (h as u32) & !1)
}

/// Dossier temporaire où l'on relie les fichiers sous des noms sans
/// caractères spéciaux (les graphes de filtres FFmpeg imposent un échappement).
fn safe_link(target: &Path, tag: &str) -> Option<PathBuf> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let base = std::env::temp_dir().join(format!("winlux-media-{}", std::process::id()));
    std::fs::create_dir_all(&base).ok()?;
    let ext: String = target
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let link = base.join(format!("{tag}{n}.{ext}"));
    let ok = link
        .to_string_lossy()
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c));
    if !ok {
        return None;
    }
    let _ = std::fs::remove_file(&link);
    let abs = std::fs::canonicalize(target).ok()?;
    std::os::unix::fs::symlink(abs, &link).ok()?;
    Some(link)
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OpenOptions {
    pub start_ms: u32,
    /// Taille d'affichage souhaitée (0 = taille native plafonnée).
    pub width: u32,
    pub height: u32,
    /// 0 à 200 %.
    pub volume: u32,
    /// 25 à 400 %.
    pub speed: u32,
    /// Rang de la piste audio (-1 = piste par défaut, -2 = sans son).
    pub audio_track: i32,
    /// Rang de la piste de sous-titres intégrée (-1 = aucune).
    pub sub_track: i32,
    /// 1 = pas de vidéo, 2 = pas d'accélération matérielle.
    pub flags: u32,
    /// Sous-titres externes (UTF-16, peut être nul).
    pub sub_path: *const u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PlaybackState {
    pub position_ms: u32,
    /// 0 démarrage, 1 lecture, 2 terminé, 3 erreur.
    pub status: u32,
    pub frame_seq: u32,
    pub width: u32,
    pub height: u32,
    pub hw_active: u32,
    pub audio_active: u32,
    pub reserved: u32,
}

struct Shared {
    frame: Mutex<(Vec<u8>, u32)>,
    out_time_us: AtomicI64,
    progress_at: Mutex<Option<Instant>>,
    status: AtomicU32,
    stop: AtomicBool,
    pid: AtomicI32,
    hw_active: AtomicBool,
    audio_active: AtomicBool,
    frames: AtomicU64,
    error: Mutex<String>,
}

struct Session {
    shared: Arc<Shared>,
    start_ms: u64,
    speed_pct: u32,
    size: (u32, u32),
    supervisor: Option<std::thread::JoinHandle<()>>,
}

impl Session {
    fn state(&self) -> PlaybackState {
        let s = &self.shared;
        let out_us = s.out_time_us.load(Ordering::Relaxed).max(0) as u64;
        let extra_ms = s
            .progress_at
            .lock()
            .ok()
            .and_then(|g| *g)
            .map(|t| t.elapsed().as_millis().min(250) as u64)
            .unwrap_or(0);
        let status = s.status.load(Ordering::Relaxed);
        let played = if status == 1 {
            out_us / 1000 + extra_ms
        } else {
            out_us / 1000
        };
        let pos = self.start_ms + played * self.speed_pct as u64 / 100;
        PlaybackState {
            position_ms: pos.min(u32::MAX as u64) as u32,
            status,
            frame_seq: s.frame.lock().map(|f| f.1).unwrap_or(0),
            width: self.size.0,
            height: self.size.1,
            hw_active: u32::from(s.hw_active.load(Ordering::Relaxed)),
            audio_active: u32::from(s.audio_active.load(Ordering::Relaxed)),
            reserved: 0,
        }
    }
    fn close(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let pid = self.shared.pid.load(Ordering::SeqCst);
        if pid > 0 {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
        if let Some(h) = self.supervisor.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

/// Un essai de lancement : arguments, accélération et audio actifs.
struct Attempt {
    args: Vec<String>,
    hw: bool,
    audio: bool,
}

fn run_attempt(ffmpeg: &Path, at: &Attempt, frame_size: usize, sh: &Arc<Shared>) -> bool {
    let mut cmd = Command::new(ffmpeg);
    die_with_parent(&mut cmd)
        .args(&at.args)
        .stdin(Stdio::null())
        .stdout(if frame_size > 0 {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::piped());
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    sh.pid.store(child.id() as i32, Ordering::SeqCst);
    if sh.stop.load(Ordering::SeqCst) {
        let _ = child.kill();
        let _ = child.wait();
        return true;
    }
    let started = Arc::new(AtomicBool::new(false));
    let stderr = child.stderr.take();
    let sh2 = sh.clone();
    let started2 = started.clone();
    let err_thread = std::thread::spawn(move || {
        let Some(stderr) = stderr else { return };
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(v) = line.strip_prefix("out_time_us=") {
                if let Ok(us) = v.trim().parse::<i64>() {
                    if us > 0 {
                        started2.store(true, Ordering::SeqCst);
                        sh2.status.store(1, Ordering::SeqCst);
                    }
                    sh2.out_time_us.store(us, Ordering::Relaxed);
                    if let Ok(mut g) = sh2.progress_at.lock() {
                        *g = Some(Instant::now());
                    }
                }
            } else if !line.contains('=') && !line.trim().is_empty() {
                if let Ok(mut e) = sh2.error.lock() {
                    *e = line.chars().take(300).collect();
                }
            }
        }
    });
    let reader = child.stdout.take().map(|mut out| {
        let sh3 = sh.clone();
        let started3 = started.clone();
        std::thread::spawn(move || {
            let mut buf = vec![0u8; frame_size];
            while out.read_exact(&mut buf).is_ok() {
                started3.store(true, Ordering::SeqCst);
                sh3.status.store(1, Ordering::SeqCst);
                sh3.frames.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut f) = sh3.frame.lock() {
                    std::mem::swap(&mut f.0, &mut buf);
                    f.1 = f.1.wrapping_add(1);
                    if buf.len() != frame_size {
                        buf = vec![0u8; frame_size];
                    }
                }
            }
        })
    });
    let status = child.wait();
    sh.pid.store(0, Ordering::SeqCst);
    let _ = err_thread.join();
    if let Some(r) = reader {
        let _ = r.join();
    }
    let ok = status.map(|s| s.success()).unwrap_or(false);
    // Un essai qui échoue sans avoir rien produit doit être retenté autrement.
    ok || started.load(Ordering::SeqCst) || sh.stop.load(Ordering::SeqCst)
}

fn supervise(ffmpeg: PathBuf, attempts: Vec<Attempt>, frame_size: usize, sh: Arc<Shared>) {
    let mut last_ok = false;
    for at in &attempts {
        if sh.stop.load(Ordering::SeqCst) {
            return;
        }
        sh.hw_active.store(at.hw, Ordering::Relaxed);
        sh.audio_active.store(at.audio, Ordering::Relaxed);
        if run_attempt(&ffmpeg, at, frame_size, &sh) {
            last_ok = true;
            break;
        }
        crate::log_warn!(
            "media",
            "essai de lecture échoué (hw={}, audio={}) : {}",
            at.hw,
            at.audio,
            sh.error.lock().map(|e| e.clone()).unwrap_or_default()
        );
    }
    if sh.stop.load(Ordering::SeqCst) {
        return;
    }
    let status = if last_ok { 2 } else { 3 };
    sh.status.store(status, Ordering::SeqCst);
}

fn open_session(path: &Path, o: &OpenOptions, sub_path: Option<PathBuf>) -> Option<Session> {
    let c = caps();
    let ffmpeg = c.ffmpeg.clone()?;
    let info = probe(path)?;
    if !info.playable(c) {
        return None;
    }
    let want_video = o.flags & 1 == 0;
    let video = info
        .main_video()
        .filter(|v| want_video && c.has_decoder(&v.codec) && v.width > 0 && v.height > 0);
    let size = video.map(|v| {
        let target = if o.width == 0 || o.height == 0 {
            (1920, 1080)
        } else {
            (o.width, o.height)
        };
        fit_size((v.width, v.height), v.sar, target)
    });
    let audio = match o.audio_track {
        -2 => None,
        n if n >= 0 => info
            .audio()
            .find(|a| a.ordinal == n as u32 && c.has_decoder(&a.codec))
            .or_else(|| info.default_audio(c)),
        _ => info.default_audio(c),
    };
    if video.is_none() && audio.is_none() {
        return None;
    }
    // Les filtres de sous-titres demandent des chemins sans caractères spéciaux.
    let mut input = path.to_path_buf();
    let subtitle = match (sub_path, o.sub_track) {
        (Some(sp), _) if video.is_some() => match safe_link(&sp, "sub") {
            Some(l) => Subtitle::External(l),
            None => Subtitle::None,
        },
        (_, n) if n >= 0 && video.is_some() => {
            match info.subtitles().find(|s| s.ordinal == n as u32) {
                Some(s) if is_bitmap_subtitle(&s.codec) => Subtitle::Embedded {
                    ordinal: s.ordinal,
                    index: s.index,
                    bitmap: true,
                },
                Some(s) => match safe_link(path, "in") {
                    Some(l) => {
                        input = l;
                        Subtitle::Embedded {
                            ordinal: s.ordinal,
                            index: s.index,
                            bitmap: false,
                        }
                    }
                    None => Subtitle::None,
                },
                None => Subtitle::None,
            }
        }
        _ => Subtitle::None,
    };
    let plan = PlayPlan {
        input,
        start_ms: o.start_ms as u64,
        video: video.zip(size).map(|(v, (w, h))| (v.index, w, h)),
        audio: audio.map(|a| a.index),
        volume_pct: o.volume.min(200),
        speed_pct: o.speed.clamp(25, 400),
        subtitle,
        hw: Vec::new(),
        audio_out: ("alsa".into(), "default".into()),
    };
    let hw = if o.flags & 2 == 0 && video.is_some() {
        c.hw_args(&hw_policy())
    } else {
        Vec::new()
    };
    let out = audio_output();
    let mut attempts = Vec::new();
    let mk = |hw: Vec<String>, audio_out: Option<(String, String)>| {
        let mut p = plan.clone();
        let has_audio = audio_out.is_some() && p.audio.is_some();
        p.hw = hw.clone();
        match audio_out {
            Some(o) => p.audio_out = o,
            None => p.audio = None,
        }
        Attempt {
            args: build_play_args(&p),
            hw: !hw.is_empty(),
            audio: has_audio,
        }
    };
    if !hw.is_empty() {
        attempts.push(mk(hw, out.clone()));
    }
    attempts.push(mk(Vec::new(), out.clone()));
    // Repli : sortie audio indisponible → vidéo seule (si vidéo il y a).
    if plan.video.is_some() && plan.audio.is_some() {
        attempts.push(mk(Vec::new(), None));
    }
    let frame_size = size.map(|(w, h)| w as usize * h as usize * 4).unwrap_or(0);
    let shared = Arc::new(Shared {
        frame: Mutex::new((Vec::new(), 0)),
        out_time_us: AtomicI64::new(0),
        progress_at: Mutex::new(None),
        status: AtomicU32::new(0),
        stop: AtomicBool::new(false),
        pid: AtomicI32::new(0),
        hw_active: AtomicBool::new(false),
        audio_active: AtomicBool::new(false),
        frames: AtomicU64::new(0),
        error: Mutex::new(String::new()),
    });
    let sh = shared.clone();
    let supervisor = std::thread::Builder::new()
        .name("winlux-media".into())
        .spawn(move || supervise(ffmpeg, attempts, frame_size, sh))
        .ok()?;
    Some(Session {
        shared,
        start_ms: o.start_ms as u64,
        speed_pct: plan.speed_pct,
        size: size.unwrap_or((0, 0)),
        supervisor: Some(supervisor),
    })
}

type SessionMap = Mutex<HashMap<usize, Session>>;

fn sessions() -> &'static SessionMap {
    static S: OnceLock<SessionMap> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// Vignettes et aperçus
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct ThumbKey {
    path: PathBuf,
    w: u32,
    h: u32,
    /// Instant demandé ; `None` = image représentative (avec cache disque).
    at_ms: Option<u64>,
}

enum ThumbResult {
    Ready(Arc<Vec<u8>>),
    Pending,
    Failed,
}

#[derive(Default)]
struct ThumbState {
    done: HashMap<ThumbKey, Option<Arc<Vec<u8>>>>,
    pending: HashSet<ThumbKey>,
    queue: VecDeque<ThumbKey>,
    workers: usize,
}

fn thumbs() -> &'static Mutex<ThumbState> {
    static T: OnceLock<Mutex<ThumbState>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(ThumbState::default()))
}

fn cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("winlux/thumbnails"))
}

fn fnv(data: &[u8], mut h: u64) -> u64 {
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn disk_cache_path(key: &ThumbKey) -> Option<PathBuf> {
    let (len, sec, nsec) = file_stamp(&key.path)?;
    let mut h = fnv(
        key.path.as_os_str().as_encoded_bytes(),
        0xcbf2_9ce4_8422_2325,
    );
    h = fnv(&len.to_le_bytes(), h);
    h = fnv(&sec.to_le_bytes(), h);
    h = fnv(&nsec.to_le_bytes(), h);
    h = fnv(&key.w.to_le_bytes(), h);
    h = fnv(&key.h.to_le_bytes(), h);
    Some(cache_dir()?.join(format!("media-{h:016x}-{}x{}.bgra", key.w, key.h)))
}

/// Génère une image BGRA `w×h` (contenu centré, fond transparent).
fn render_thumb(key: &ThumbKey) -> Option<Vec<u8>> {
    let c = caps();
    let ffmpeg = c.ffmpeg.as_ref()?;
    let info = probe(&key.path)?;
    let (stream, seek) = match info.main_video().filter(|v| c.has_decoder(&v.codec)) {
        Some(v) => {
            let at = key.at_ms.unwrap_or_else(|| {
                if info.duration_ms > 0 {
                    (info.duration_ms / 10).min(60_000)
                } else {
                    0
                }
            });
            (v, at)
        }
        None => (info.cover().filter(|v| c.has_decoder(&v.codec))?, 0),
    };
    let (w, h) = fit_size(
        (stream.width.max(1), stream.height.max(1)),
        stream.sar,
        (key.w, key.h),
    );
    let (w, h) = (w.min(key.w).max(2), h.min(key.h).max(2));
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-loglevel", "error"]);
    if seek > 0 {
        cmd.args(["-ss", &format!("{:.3}", seek as f64 / 1000.0)]);
    }
    cmd.arg("-i")
        .arg(&key.path)
        .args(["-map", &format!("0:{}", stream.index)])
        .args(["-frames:v", "1", "-an", "-sn"])
        .args([
            "-vf",
            &format!("scale={w}:{h}:flags=bicubic,setsar=1,format=bgra"),
        ])
        .args(["-f", "rawvideo", "pipe:1"]);
    let mut raw = run_with_timeout(&mut cmd, THUMB_TIMEOUT)?;
    if raw.len() < (w * h * 4) as usize && seek > 0 {
        // Position au-delà de la fin réelle : on reprend au début.
        let k = ThumbKey {
            at_ms: Some(0),
            ..key.clone()
        };
        return render_thumb(&k);
    }
    raw.truncate((w * h * 4) as usize);
    if raw.len() != (w * h * 4) as usize {
        return None;
    }
    let mut out = vec![0u8; (key.w * key.h * 4) as usize];
    let ox = (key.w - w) / 2;
    let oy = (key.h - h) / 2;
    for y in 0..h {
        let src = (y * w * 4) as usize;
        let dst = (((y + oy) * key.w + ox) * 4) as usize;
        out[dst..dst + (w * 4) as usize].copy_from_slice(&raw[src..src + (w * 4) as usize]);
        for px in out[dst..dst + (w * 4) as usize].chunks_exact_mut(4) {
            px[3] = 255;
        }
    }
    Some(out)
}

fn thumb_worker() {
    loop {
        let key = {
            let Ok(mut st) = thumbs().lock() else { return };
            match st.queue.pop_back() {
                Some(k) => k,
                None => {
                    st.workers -= 1;
                    return;
                }
            }
        };
        let result = render_thumb(&key).map(Arc::new);
        if let (Some(img), None) = (&result, key.at_ms) {
            if let Some(p) = disk_cache_path(&key) {
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let tmp = p.with_extension(format!("tmp{}", std::process::id()));
                if std::fs::write(&tmp, img.as_slice()).is_ok() {
                    let _ = std::fs::rename(&tmp, &p);
                }
            }
        }
        if let Ok(mut st) = thumbs().lock() {
            st.pending.remove(&key);
            if st.done.len() >= THUMB_MEMORY_ENTRIES {
                st.done.clear();
            }
            st.done.insert(key, result);
        }
    }
}

fn request_thumb(key: ThumbKey) -> ThumbResult {
    let Ok(mut st) = thumbs().lock() else {
        return ThumbResult::Failed;
    };
    if let Some(r) = st.done.get(&key) {
        return match r {
            Some(img) => ThumbResult::Ready(img.clone()),
            None => ThumbResult::Failed,
        };
    }
    if key.at_ms.is_none() {
        if let Some(p) = disk_cache_path(&key) {
            if let Ok(data) = std::fs::read(&p) {
                if data.len() == (key.w * key.h * 4) as usize {
                    let img = Arc::new(data);
                    st.done.insert(key, Some(img.clone()));
                    return ThumbResult::Ready(img);
                }
            }
        }
    }
    if !caps().engine() {
        return ThumbResult::Failed;
    }
    if st.pending.insert(key.clone()) {
        // Les demandes les plus récentes passent d'abord (survol, défilement).
        st.queue.push_back(key);
        if st.queue.len() > 64 {
            if let Some(old) = st.queue.pop_front() {
                st.pending.remove(&old);
            }
        }
        if st.workers < THUMB_WORKERS {
            st.workers += 1;
            if std::thread::Builder::new()
                .name("winlux-thumb".into())
                .spawn(thumb_worker)
                .is_err()
            {
                st.workers -= 1;
            }
        }
    }
    ThumbResult::Pending
}

// ---------------------------------------------------------------------------
// Reprise de lecture
// ---------------------------------------------------------------------------

fn resume_file() -> Option<PathBuf> {
    std::env::var_os("WINLUX_MEDIA_RESUME")
        .map(PathBuf::from)
        .or_else(|| Some(crate::paths::data_dir()?.join("media-resume.tsv")))
}

fn resume_lock() -> &'static Mutex<()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
}

pub fn parse_resume(text: &str) -> Vec<(u64, String)> {
    text.lines()
        .filter_map(|l| {
            let (ms, path) = l.split_once('\t')?;
            Some((ms.parse().ok()?, path.to_string()))
        })
        .collect()
}

pub fn resume_get(path: &Path) -> u64 {
    let _g = resume_lock().lock();
    let Some(f) = resume_file() else { return 0 };
    let key = path.to_string_lossy();
    std::fs::read_to_string(f)
        .map(|t| {
            parse_resume(&t)
                .into_iter()
                .find(|(_, p)| *p == key)
                .map(|(ms, _)| ms)
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// Mémorise la position ; l'efface si la lecture est au début ou finie.
pub fn resume_set(path: &Path, ms: u64, duration_ms: u64) {
    let _g = resume_lock().lock();
    let Some(f) = resume_file() else { return };
    let key = path.to_string_lossy().replace(['\n', '\t'], " ");
    let mut list = std::fs::read_to_string(&f)
        .map(|t| parse_resume(&t))
        .unwrap_or_default();
    list.retain(|(_, p)| *p != key);
    let finished = duration_ms > 0 && ms + 15_000 >= duration_ms;
    if ms >= 5_000 && !finished {
        list.insert(0, (ms, key));
    }
    list.truncate(RESUME_MAX_ENTRIES);
    let text: String = list.iter().map(|(ms, p)| format!("{ms}\t{p}\n")).collect();
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = f.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &f);
    }
}

// ---------------------------------------------------------------------------
// Exports pour le code PE
// ---------------------------------------------------------------------------

unsafe fn wide_path(p: *const u16) -> Option<PathBuf> {
    if p.is_null() {
        return None;
    }
    let len = crate::printf::wide_len(p);
    if len == 0 || len > 32 * 1024 {
        return None;
    }
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
    Some(crate::paths::to_linux(&s))
}

/// UTF-8 → Windows-1252 (repli « ? »), comme le reste du shell.
pub fn to_cp1252(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c as u32 {
            0..=0x7F | 0xA0..=0xFF => c as u32 as u8,
            0x20AC => 0x80,
            0x2019 => 0x92,
            0x2018 => 0x91,
            0x201C => 0x93,
            0x201D => 0x94,
            0x2013 => 0x96,
            0x2014 => 0x97,
            0x2026 => 0x85,
            0x0153 => 0x9C,
            0x0152 => 0x8C,
            _ => b'?',
        })
        .collect()
}

unsafe fn copy_out(text: &str, out: *mut u8, cap: u32) -> i32 {
    let bytes = to_cp1252(text);
    if out.is_null() || cap == 0 {
        return bytes.len().min(i32::MAX as usize) as i32;
    }
    let n = bytes.len().min(cap as usize - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, n);
    *out.add(n) = 0;
    n as i32
}

/// Décrit un fichier (texte tabulé, Windows-1252). -1 si illisible.
pub unsafe extern "win64" fn winlux_media_probe(path: *const u16, out: *mut u8, cap: u32) -> i32 {
    let Some(p) = wide_path(path) else { return -1 };
    match probe(&p) {
        Some(info) => copy_out(&info.report(caps()), out, cap),
        None => -1,
    }
}

/// Liste les formats décodables sur cette machine.
pub unsafe extern "win64" fn winlux_media_formats(out: *mut u8, cap: u32) -> i32 {
    copy_out(&formats_report(caps()), out, cap)
}

/// Ouvre une session de lecture ; 0 en cas d'échec.
pub unsafe extern "win64" fn winlux_media_open(
    path: *const u16,
    opts: *const OpenOptions,
) -> usize {
    static NEXT: AtomicU64 = AtomicU64::new(0xB000_0000);
    let (Some(p), false) = (wide_path(path), opts.is_null()) else {
        return 0;
    };
    let o = std::ptr::read_unaligned(opts);
    let sub = wide_path(o.sub_path).filter(|p| p.is_file());
    let Some(session) = open_session(&p, &o, sub) else {
        return 0;
    };
    let h = NEXT.fetch_add(1, Ordering::Relaxed) as usize;
    if let Ok(mut m) = sessions().lock() {
        m.insert(h, session);
        h
    } else {
        0
    }
}

pub unsafe extern "win64" fn winlux_media_state(h: usize, out: *mut PlaybackState) -> i32 {
    if out.is_null() {
        return 0;
    }
    let Ok(m) = sessions().lock() else { return 0 };
    match m.get(&h) {
        Some(s) => {
            std::ptr::write_unaligned(out, s.state());
            1
        }
        None => 0,
    }
}

/// Copie la dernière image (BGRA, de haut en bas) ; renvoie son numéro, 0 sinon.
pub unsafe extern "win64" fn winlux_media_frame(h: usize, out: *mut u8, cap: u32) -> u32 {
    if out.is_null() {
        return 0;
    }
    let Ok(m) = sessions().lock() else { return 0 };
    let Some(s) = m.get(&h) else { return 0 };
    let Ok(f) = s.shared.frame.lock() else {
        return 0;
    };
    if f.0.is_empty() || f.0.len() > cap as usize {
        return 0;
    }
    std::ptr::copy_nonoverlapping(f.0.as_ptr(), out, f.0.len());
    f.1
}

/// Dernier message d'erreur du décodeur.
pub unsafe extern "win64" fn winlux_media_error(h: usize, out: *mut u8, cap: u32) -> i32 {
    let msg = sessions()
        .lock()
        .ok()
        .and_then(|m| {
            m.get(&h)
                .and_then(|s| s.shared.error.lock().ok().map(|e| e.clone()))
        })
        .unwrap_or_default();
    copy_out(&msg, out, cap)
}

pub unsafe extern "win64" fn winlux_media_close(h: usize) -> i32 {
    let s = sessions().lock().ok().and_then(|mut m| m.remove(&h));
    match s {
        Some(mut s) => {
            s.close();
            1
        }
        None => 0,
    }
}

unsafe fn thumb_export(key: ThumbKey, out: *mut u8) -> i32 {
    if out.is_null() || key.w == 0 || key.h == 0 || key.w > MAX_THUMB || key.h > MAX_THUMB {
        return -1;
    }
    match request_thumb(key) {
        ThumbResult::Ready(img) => {
            std::ptr::copy_nonoverlapping(img.as_ptr(), out, img.len());
            1
        }
        ThumbResult::Pending => 0,
        ThumbResult::Failed => -1,
    }
}

/// Vignette `size×size` BGRA d'une vidéo (ou pochette d'un fichier audio).
/// 1 = prête, 0 = génération en cours (redemander), -1 = pas de vignette.
pub unsafe extern "win64" fn winlux_media_thumbnail(
    path: *const u16,
    size: u32,
    out: *mut u8,
) -> i32 {
    let Some(p) = wide_path(path) else { return -1 };
    thumb_export(
        ThumbKey {
            path: p,
            w: size,
            h: size,
            at_ms: None,
        },
        out,
    )
}

/// Aperçu `w×h` à un instant donné (survol de la barre de progression).
pub unsafe extern "win64" fn winlux_media_preview(
    path: *const u16,
    at_ms: u32,
    w: u32,
    h: u32,
    out: *mut u8,
) -> i32 {
    let Some(p) = wide_path(path) else { return -1 };
    thumb_export(
        ThumbKey {
            path: p,
            w,
            h,
            at_ms: Some(at_ms as u64),
        },
        out,
    )
}

pub unsafe extern "win64" fn winlux_media_get_resume(path: *const u16) -> u32 {
    wide_path(path)
        .map(|p| resume_get(&p).min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}

pub unsafe extern "win64" fn winlux_media_set_resume(
    path: *const u16,
    ms: u32,
    duration_ms: u32,
) -> i32 {
    match wide_path(path) {
        Some(p) => {
            resume_set(&p, ms as u64, duration_ms as u64);
            1
        }
        None => 0,
    }
}

/// Forme Windows canonique d'un chemin (`/home/moi/a.mkv` → `C:\\a.mkv`), en UTF-16.
pub unsafe extern "win64" fn winlux_media_canonical(
    path: *const u16,
    out: *mut u16,
    cap: u32,
) -> i32 {
    match wide_path(path) {
        Some(p) => copy_out_wide(&crate::paths::to_windows(&p), out, cap),
        None => -1,
    }
}

/// 1 = audio, 2 = vidéo, 3 = sous-titres, 0 = autre (d'après l'extension).
pub unsafe extern "win64" fn winlux_media_kind(path: *const u16) -> u32 {
    wide_path(path).map(|p| media_kind(&p)).unwrap_or(0)
}

/// Copie en UTF-16 (les noms de fichiers ne tiennent pas tous en Windows-1252).
/// Ne coupe jamais au milieu d'une ligne : seules des lignes entières sont copiées.
unsafe fn copy_out_wide(text: &str, out: *mut u16, cap: u32) -> i32 {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if out.is_null() || cap == 0 {
        return wide.len().min(i32::MAX as usize) as i32;
    }
    let mut n = wide.len().min(cap as usize - 1);
    if n < wide.len() {
        n = wide[..n]
            .iter()
            .rposition(|&c| c == b'\n' as u16)
            .map_or(0, |i| i + 1);
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), out, n);
    *out.add(n) = 0;
    n as i32
}

unsafe fn list_out(paths: &[PathBuf], out: *mut u16, cap: u32) -> i32 {
    let text: String = paths
        .iter()
        .map(|p| format!("{}\n", crate::paths::to_windows(p)))
        .collect();
    copy_out_wide(&text, out, cap)
}

/// Fichiers audio/vidéo du dossier (chemins Windows UTF-16, un par ligne, tri naturel).
pub unsafe extern "win64" fn winlux_media_siblings(
    path: *const u16,
    out: *mut u16,
    cap: u32,
) -> i32 {
    match wide_path(path) {
        Some(p) => list_out(&siblings(&p), out, cap),
        None => -1,
    }
}

/// Sous-titres externes trouvés à côté de la vidéo (chemins Windows UTF-16).
pub unsafe extern "win64" fn winlux_media_subtitles(
    path: *const u16,
    out: *mut u16,
    cap: u32,
) -> i32 {
    match wide_path(path) {
        Some(p) => list_out(&sidecar_subtitles(&p), out, cap),
        None => -1,
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxMediaProbe" => fn winlux_media_probe,
        "WinluxMediaFormats" => fn winlux_media_formats,
        "WinluxMediaOpen" => fn winlux_media_open,
        "WinluxMediaState" => fn winlux_media_state,
        "WinluxMediaFrame" => fn winlux_media_frame,
        "WinluxMediaError" => fn winlux_media_error,
        "WinluxMediaClose" => fn winlux_media_close,
        "WinluxMediaThumbnail" => fn winlux_media_thumbnail,
        "WinluxMediaPreview" => fn winlux_media_preview,
        "WinluxMediaGetResume" => fn winlux_media_get_resume,
        "WinluxMediaSetResume" => fn winlux_media_set_resume,
        "WinluxMediaKind" => fn winlux_media_kind,
        "WinluxMediaCanonical" => fn winlux_media_canonical,
        "WinluxMediaSiblings" => fn winlux_media_siblings,
        "WinluxMediaSubtitles" => fn winlux_media_subtitles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps_with(dec: &[&str]) -> Caps {
        Caps {
            ffmpeg: Some("/x/ffmpeg".into()),
            ffprobe: Some("/x/ffprobe".into()),
            decoders: dec.iter().map(|s| s.to_string()).collect(),
            ..Caps::default()
        }
    }

    const FLAT: &str = r#"streams.stream.0.index=0
streams.stream.0.codec_name="hevc"
streams.stream.0.codec_type="video"
streams.stream.0.width=1920
streams.stream.0.height=1080
streams.stream.0.sample_aspect_ratio="1:1"
streams.stream.0.avg_frame_rate="24000/1001"
streams.stream.0.disposition.default=1
streams.stream.0.disposition.attached_pic=0
streams.stream.1.index=1
streams.stream.1.codec_name="eac3"
streams.stream.1.codec_type="audio"
streams.stream.1.channels=6
streams.stream.1.disposition.default=0
streams.stream.1.tags.language="eng"
streams.stream.2.index=2
streams.stream.2.codec_name="aac"
streams.stream.2.codec_type="audio"
streams.stream.2.channels=2
streams.stream.2.disposition.default=1
streams.stream.2.tags.language="fre"
streams.stream.2.tags.title="Français \"VF\""
streams.stream.3.index=3
streams.stream.3.codec_name="subrip"
streams.stream.3.codec_type="subtitle"
streams.stream.3.tags.language="fre"
streams.stream.4.index=4
streams.stream.4.codec_name="hdmv_pgs_subtitle"
streams.stream.4.codec_type="subtitle"
chapters.chapter.0.start_time="0.000000"
chapters.chapter.0.tags.title="Ouverture"
chapters.chapter.1.start_time="312.5"
chapters.chapter.1.tags.title="Acte 2"
format.format_name="matroska,webm"
format.duration="5400.250000"
format.tags.title="Film"
"#;

    #[test]
    fn sonde_flat_complete() {
        let info = MediaInfo::from_flat(&parse_flat(FLAT));
        assert_eq!(info.duration_ms, 5_400_250);
        assert_eq!(info.container, "matroska,webm");
        assert_eq!(info.title, "Film");
        let v = info.main_video().unwrap();
        assert_eq!((v.width, v.height, v.fps_x100), (1920, 1080, 2397));
        let audio: Vec<_> = info.audio().collect();
        assert_eq!(audio.len(), 2);
        assert_eq!(audio[1].ordinal, 1);
        assert_eq!(audio[1].title, "Français \"VF\"");
        let subs: Vec<_> = info.subtitles().collect();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[1].ordinal, 1);
        assert_eq!(
            info.chapters,
            vec![(0, "Ouverture".into()), (312_500, "Acte 2".into())]
        );
    }

    #[test]
    fn lisible_seulement_si_decodeur_present() {
        let info = MediaInfo::from_flat(&parse_flat(FLAT));
        let none = caps_with(&["aac"]);
        assert!(
            !info.playable(&none),
            "HEVC sans décodeur ne doit pas être annoncé"
        );
        assert_eq!(
            info.missing(&none),
            vec!["H.265 / HEVC", "Dolby Digital Plus (E-AC-3)"]
        );
        let full = caps_with(&["hevc", "aac"]);
        assert!(info.playable(&full));
        // Piste par défaut décodable : l'AAC (défaut), pas l'E-AC-3.
        assert_eq!(info.default_audio(&full).unwrap().index, 2);
        let report = info.report(&full);
        assert!(report.contains("playable\t1\n"));
        assert!(report.contains("video\tH.265 / HEVC\t1920\t1080\t2397\t1\n"));
        assert!(report.contains("audio\t0\tDolby Digital Plus (E-AC-3)\t6\teng\t0\t\n"));
        assert!(report.contains("sub\t1\tPGS (Blu-ray)\t\t0\t\n"));
        assert!(report.contains("chapter\t312500\tActe 2\n"));
    }

    #[test]
    fn decodeurs_et_acceleration() {
        let text = "Decoders:\n V..... = Video\n ------\n V....D h264                 H.264\n VFS..D hevc  HEVC\n A....D aac   AAC\n S..... subrip SubRip\n";
        let d = parse_decoders(text);
        assert!(
            d.contains("h264") && d.contains("hevc") && d.contains("aac") && d.contains("subrip")
        );
        assert!(!d.contains("V....."));
        let hw = parse_hwaccels("Hardware acceleration methods:\nvdpau\ncuda\nvaapi\n\n");
        assert_eq!(hw, vec!["vdpau", "cuda", "vaapi"]);

        let mut c = caps_with(&[]);
        c.hwaccels = hw;
        assert!(
            c.hw_args("auto").is_empty(),
            "pas de périphérique, pas de hwaccel"
        );
        c.render_node = Some("/dev/dri/renderD128".into());
        assert_eq!(c.hw_args("auto")[1], "vaapi");
        assert_eq!(c.hw_args("auto")[3], "/dev/dri/renderD128");
        c.nvidia = true;
        assert_eq!(c.hw_args("auto")[1], "cuda");
        assert_eq!(c.hw_args("vaapi")[1], "vaapi");
        assert!(c.hw_args("off").is_empty());
        // L'AV1 se reconnaît via libdav1d même si ffprobe dit « av1 ».
        let c = caps_with(&["libdav1d"]);
        assert!(c.has_decoder("av1"));
    }

    #[test]
    fn taille_de_decodage() {
        assert_eq!(fit_size((1920, 1080), (1, 1), (960, 960)), (960, 540));
        assert_eq!(fit_size((1920, 1080), (1, 1), (4000, 4000)), (1920, 1080));
        // DVD anamorphique 720×576 en 16:9 (SAR 64:45).
        assert_eq!(fit_size((720, 576), (64, 45), (1024, 576)), (1024, 576));
        let (w, h) = fit_size((1080, 1920), (1, 1), (800, 450));
        assert_eq!((w % 2, h % 2), (0, 0));
        assert!(h <= 450 && w < h);
    }

    #[test]
    fn vitesse_atempo() {
        assert!(atempo_chain(100).is_empty());
        assert_eq!(atempo_chain(150), vec!["atempo=1.5000"]);
        assert_eq!(atempo_chain(400), vec!["atempo=2", "atempo=2"]);
        assert_eq!(atempo_chain(25), vec!["atempo=0.5", "atempo=0.5"]);
        assert_eq!(atempo_chain(1000), vec!["atempo=2", "atempo=2"]);
    }

    #[test]
    fn ligne_de_commande_lecture() {
        let plan = PlayPlan {
            input: "/v/film.mkv".into(),
            start_ms: 61_500,
            video: Some((0, 1280, 720)),
            audio: Some(2),
            volume_pct: 80,
            speed_pct: 150,
            subtitle: Subtitle::Embedded {
                ordinal: 1,
                index: 4,
                bitmap: true,
            },
            hw: vec!["-hwaccel".into(), "vaapi".into()],
            audio_out: ("pulse".into(), "default".into()),
        };
        let a = build_play_args(&plan);
        let joined = a.join(" ");
        assert!(joined.contains("-hwaccel vaapi -readrate 1.500 -ss 61.500 -copyts -i /v/film.mkv"));
        assert!(joined.contains("[0:0][0:4]overlay="));
        assert!(joined.contains("setpts=(PTS-STARTPTS)/1.5000,scale=w=1280:h=720"));
        assert!(joined.contains("-map [vout] -f rawvideo -pix_fmt bgra pipe:1"));
        assert!(joined.contains(
            "-map 0:2 -af asetpts=PTS-STARTPTS,atempo=1.5000,volume=0.80 -f pulse default"
        ));

        let audio_only = PlayPlan {
            video: None,
            subtitle: Subtitle::External("/tmp/s.srt".into()),
            start_ms: 0,
            ..plan.clone()
        };
        let joined = build_play_args(&audio_only).join(" ");
        assert!(!joined.contains("-hwaccel"), "pas de hwaccel sans vidéo");
        assert!(!joined.contains("-ss"));
        assert!(!joined.contains("rawvideo"));

        let text_sub = PlayPlan {
            subtitle: Subtitle::Embedded {
                ordinal: 0,
                index: 3,
                bitmap: false,
            },
            ..plan
        };
        let joined = build_play_args(&text_sub).join(" ");
        assert!(joined.contains("[0:0]subtitles=filename=/v/film.mkv:si=0,setpts"));
    }

    #[test]
    fn reprise_de_lecture() {
        let dir = std::env::temp_dir().join(format!("winlux-resume-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("resume.tsv");
        std::env::set_var("WINLUX_MEDIA_RESUME", &file);
        let a = Path::new("/films/a.mkv");
        let b = Path::new("/films/b b.mp4");
        resume_set(a, 120_000, 5_000_000);
        resume_set(b, 3_000, 5_000_000); // trop tôt : pas mémorisé
        assert_eq!(resume_get(a), 120_000);
        assert_eq!(resume_get(b), 0);
        resume_set(b, 60_000, 5_000_000);
        assert_eq!(resume_get(b), 60_000);
        resume_set(a, 4_990_000, 5_000_000); // fini : effacé
        assert_eq!(resume_get(a), 0);
        assert_eq!(resume_get(b), 60_000);
        std::env::remove_var("WINLUX_MEDIA_RESUME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conversion_cp1252() {
        assert_eq!(
            to_cp1252("Vidéo – “ok” €"),
            b"Vid\xE9o \x96 \x93ok\x94 \x80"
        );
        assert_eq!(to_cp1252("日本"), b"??");
    }

    #[test]
    fn liste_de_lecture_et_sous_titres() {
        assert!(natural_cmp("Episode 2.mkv", "episode 10.mkv").is_lt());
        assert!(natural_cmp("a01", "a1").is_eq());
        let dir = std::env::temp_dir().join(format!("winlux-media-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "Ep 10.mkv",
            "Ep 2.mkv",
            "Ep 2.fr.srt",
            "Ep 2.en.ASS",
            "Ep 20.srt",
            "notes.txt",
            "chanson.flac",
        ] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let names = |v: Vec<PathBuf>| -> Vec<String> {
            v.iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            names(siblings(&dir.join("Ep 2.mkv"))),
            vec!["chanson.flac", "Ep 2.mkv", "Ep 10.mkv"]
        );
        assert_eq!(
            names(sidecar_subtitles(&dir.join("Ep 2.mkv"))),
            vec!["Ep 2.en.ASS", "Ep 2.fr.srt"]
        );
        assert_eq!(media_kind(Path::new("x.M2TS")), 2);
        assert_eq!(media_kind(Path::new("x.opus")), 1);
        assert_eq!(media_kind(Path::new("x.vtt")), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn noms_de_codecs() {
        assert_eq!(friendly_codec("h264"), "H.264 / AVC");
        assert_eq!(friendly_codec("dca"), "DTS");
        assert_eq!(friendly_codec("subrip"), "SRT");
        assert_eq!(friendly_codec("foo"), "FOO");
    }

    /// Bout en bout avec le vrai FFmpeg (installé en CI) : sonde, vignette
    /// et lecture d'une vidéo générée, sans sortie audio réelle.
    #[test]
    #[ignore = "nécessite ffmpeg/ffprobe"]
    fn media_bout_en_bout_ffmpeg() {
        let c = caps();
        assert!(c.engine(), "ffmpeg/ffprobe introuvables");
        let dir = std::env::temp_dir().join(format!("winlux-media-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("clip vidéo 'test'.mkv");
        let ok = Command::new(c.ffmpeg.as_ref().unwrap())
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=25:duration=3",
            ])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=3"])
            .args(["-c:v", "mpeg4", "-c:a", "flac"])
            .arg(&clip)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let info = probe(&clip).expect("sonde");
        assert!(info.playable(c));
        assert!(
            (2900..=3200).contains(&info.duration_ms),
            "{}",
            info.duration_ms
        );

        let key = ThumbKey {
            path: clip.clone(),
            w: 96,
            h: 96,
            at_ms: None,
        };
        let img = render_thumb(&key).expect("vignette");
        assert_eq!(img.len(), 96 * 96 * 4);
        // 320×240 → 96×72 centré : la première ligne est transparente.
        assert_eq!(img[3], 0);
        assert_eq!(img[(48 * 96 + 48) * 4 + 3], 255);

        std::env::set_var("WINLUX_MEDIA_AUDIO", "null:-");
        let o = OpenOptions {
            start_ms: 1000,
            width: 160,
            height: 160,
            volume: 100,
            speed: 200,
            audio_track: -1,
            sub_track: -1,
            flags: 2,
            sub_path: std::ptr::null(),
        };
        let mut s = open_session(&clip, &o, None).expect("session");
        let deadline = Instant::now() + Duration::from_secs(20);
        while s.state().status < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let st = s.state();
        assert_eq!(
            st.status,
            2,
            "erreur : {:?}",
            s.shared.error.lock().unwrap()
        );
        assert_eq!((st.width, st.height), (160, 120));
        assert!(s.shared.frames.load(Ordering::Relaxed) >= 10);
        assert!(st.position_ms >= 2000, "{}", st.position_ms);
        s.close();
        std::env::remove_var("WINLUX_MEDIA_AUDIO");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
