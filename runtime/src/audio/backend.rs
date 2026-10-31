//! Sorties et entrées audio Linux, choisies à l'exécution :
//! PipeWire (API native), PulseAudio (libpulse-simple), ALSA, puis une
//! sortie nulle cadencée en temps réel quand aucun périphérique n'existe.
//!
//! Les bibliothèques sont chargées avec `dlopen` : le runtime démarre même
//! si aucune n'est installée.

use super::{PERIOD, RATE};
use libc::{c_char, c_int, c_void};
use std::ffi::CString;
use std::time::{Duration, Instant};

/// Bibliothèque partagée ouverte avec dlopen et jamais déchargée tant que
/// l'objet vit.
pub(super) struct Lib(*mut c_void);
unsafe impl Send for Lib {}
unsafe impl Sync for Lib {}

impl Lib {
    pub(super) fn open(name: &str) -> Option<Lib> {
        let c = CString::new(name).ok()?;
        let p = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if p.is_null() {
            None
        } else {
            Some(Lib(p))
        }
    }
    pub(super) fn sym(&self, name: &str) -> Option<*mut c_void> {
        let c = CString::new(name).ok()?;
        let p = unsafe { libc::dlsym(self.0, c.as_ptr()) };
        if p.is_null() {
            None
        } else {
            Some(p)
        }
    }
}

impl Drop for Lib {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.0);
        }
    }
}

/// Résout un symbole et le convertit en pointeur de fonction de type `T`.
///
/// # Safety
/// `T` doit être le type exact de la fonction C exportée.
pub(super) unsafe fn func<T: Copy>(lib: &Lib, name: &str) -> Option<T> {
    let p = lib.sym(name)?;
    Some(std::mem::transmute_copy::<*mut c_void, T>(&p))
}

macro_rules! load {
    ($lib:expr, $name:literal) => {
        $crate::audio::backend::func(&$lib, $name)?
    };
}
pub(super) use load;

/// Pilote audio Linux utilisé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    PipeWire,
    Pulse,
    Alsa,
    Null,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::PipeWire => "pipewire",
            Kind::Pulse => "pulseaudio",
            Kind::Alsa => "alsa",
            Kind::Null => "null",
        }
    }
    fn parse(s: &str) -> Option<Kind> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "pipewire" | "pw" => Kind::PipeWire,
            "pulse" | "pulseaudio" => Kind::Pulse,
            "alsa" => Kind::Alsa,
            "null" | "none" | "off" => Kind::Null,
            _ => return None,
        })
    }
}

fn runtime_dir_has(name: &str) -> bool {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|d| std::path::Path::new(&d).join(name).exists())
        .unwrap_or(false)
}

/// Ordre d'essai des pilotes. `WINLUX_AUDIO=pipewire|pulse|alsa|null`
/// force un choix ; sinon on suit les serveurs réellement présents.
pub fn candidates() -> Vec<Kind> {
    candidates_from(
        std::env::var("WINLUX_AUDIO").ok().as_deref(),
        std::env::var_os("PIPEWIRE_REMOTE").is_some() || runtime_dir_has("pipewire-0"),
        std::env::var_os("PULSE_SERVER").is_some() || runtime_dir_has("pulse/native"),
    )
}

fn candidates_from(forced: Option<&str>, pipewire: bool, pulse: bool) -> Vec<Kind> {
    if let Some(k) = forced.and_then(Kind::parse) {
        return if k == Kind::Null {
            vec![Kind::Null]
        } else {
            vec![k, Kind::Null]
        };
    }
    let mut v = Vec::new();
    if pipewire {
        v.push(Kind::PipeWire);
    }
    if pulse {
        v.push(Kind::Pulse);
    }
    v.push(Kind::Alsa);
    v.push(Kind::Null);
    v
}

/// Flux de sortie : reçoit des périodes stéréo f32 à `RATE` Hz et bloque au
/// rythme du périphérique.
pub trait Output: Send {
    fn kind(&self) -> Kind;
    /// Renvoie `false` si le périphérique a disparu.
    fn write(&mut self, stereo: &[f32]) -> bool;
}

/// Flux d'entrée (micro) : remplit des périodes stéréo f32 à `RATE` Hz.
pub trait Input: Send {
    fn read(&mut self, stereo: &mut [f32]) -> bool;
}

/// Nom d'application présenté au serveur son (visible dans le mixeur par
/// application) : le nom du .exe.
pub fn app_name() -> String {
    let name = crate::loader::main_module()
        .and_then(|m| {
            std::path::Path::new(&m.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "winlux".into());
    name.chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\')
        .collect()
}

pub fn open_output() -> Box<dyn Output> {
    let app = app_name();
    for k in candidates() {
        let out: Option<Box<dyn Output>> = match k {
            Kind::PipeWire => {
                super::pipewire::Stream::open(&app, false).map(|s| Box::new(s) as Box<dyn Output>)
            }
            Kind::Pulse => Pulse::open(&app, false).map(|s| Box::new(s) as Box<dyn Output>),
            Kind::Alsa => Alsa::open(false).map(|s| Box::new(s) as Box<dyn Output>),
            Kind::Null => Some(Box::new(Null::new())),
        };
        if let Some(o) = out {
            crate::log_info!("audio", "sortie {} pour {app}", k.name());
            return o;
        }
    }
    Box::new(Null::new())
}

pub fn open_input() -> Box<dyn Input> {
    let app = app_name();
    for k in candidates() {
        let inp: Option<Box<dyn Input>> = match k {
            Kind::PipeWire => {
                super::pipewire::Stream::open(&app, true).map(|s| Box::new(s) as Box<dyn Input>)
            }
            Kind::Pulse => Pulse::open(&app, true).map(|s| Box::new(s) as Box<dyn Input>),
            Kind::Alsa => Alsa::open(true).map(|s| Box::new(s) as Box<dyn Input>),
            Kind::Null => Some(Box::new(Null::new())),
        };
        if let Some(i) = inp {
            crate::log_info!("audio", "entrée {} pour {app}", k.name());
            return i;
        }
    }
    Box::new(Null::new())
}

// ---------------------------------------------------------------------------
// Sortie nulle : silence, mais au rythme réel pour que les positions,
// événements et rappels des programmes avancent normalement.
// ---------------------------------------------------------------------------

pub struct Null {
    next: Instant,
}

impl Null {
    pub fn new() -> Self {
        Null {
            next: Instant::now(),
        }
    }
    fn pace(&mut self, frames: usize) {
        let now = Instant::now();
        if self.next + Duration::from_millis(200) < now {
            self.next = now;
        }
        self.next += Duration::from_nanos(frames as u64 * 1_000_000_000 / RATE as u64);
        if let Some(wait) = self.next.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
}

impl Output for Null {
    fn kind(&self) -> Kind {
        Kind::Null
    }
    fn write(&mut self, stereo: &[f32]) -> bool {
        self.pace(stereo.len() / 2);
        true
    }
}

impl Input for Null {
    fn read(&mut self, stereo: &mut [f32]) -> bool {
        stereo.fill(0.0);
        self.pace(stereo.len() / 2);
        true
    }
}

// ---------------------------------------------------------------------------
// PulseAudio (et pipewire-pulse) via l'API « simple » bloquante.
// ---------------------------------------------------------------------------

#[repr(C)]
struct PaSampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}
#[repr(C)]
struct PaBufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

type PaNew = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    c_int,
    *const c_char,
    *const c_char,
    *const PaSampleSpec,
    *const c_void,
    *const PaBufferAttr,
    *mut c_int,
) -> *mut c_void;
type PaIo = unsafe extern "C" fn(*mut c_void, *mut c_void, usize, *mut c_int) -> c_int;
type PaFree = unsafe extern "C" fn(*mut c_void);

pub struct Pulse {
    _lib: Lib,
    handle: *mut c_void,
    io: PaIo,
    free: PaFree,
}
unsafe impl Send for Pulse {}

impl Pulse {
    fn open(app: &str, capture: bool) -> Option<Pulse> {
        let lib = Lib::open("libpulse-simple.so.0")?;
        unsafe {
            let new: PaNew = load!(lib, "pa_simple_new");
            let io: PaIo = if capture {
                load!(lib, "pa_simple_read")
            } else {
                load!(lib, "pa_simple_write")
            };
            let free: PaFree = load!(lib, "pa_simple_free");
            let app_c = CString::new(app).ok()?;
            let stream = if capture {
                c"Capture Winlux"
            } else {
                c"Lecture Winlux"
            };
            // PA_SAMPLE_FLOAT32LE = 5 ; PA_STREAM_PLAYBACK = 1, RECORD = 2.
            let spec = PaSampleSpec {
                format: 5,
                rate: RATE,
                channels: 2,
            };
            let period_bytes = (PERIOD * 8) as u32;
            let attr = PaBufferAttr {
                maxlength: u32::MAX,
                tlength: if capture { u32::MAX } else { period_bytes * 4 },
                prebuf: u32::MAX,
                minreq: if capture { u32::MAX } else { period_bytes },
                fragsize: if capture { period_bytes } else { u32::MAX },
            };
            let mut err = 0;
            let handle = new(
                std::ptr::null(),
                app_c.as_ptr(),
                if capture { 2 } else { 1 },
                std::ptr::null(),
                stream.as_ptr(),
                &spec,
                std::ptr::null(),
                &attr,
                &mut err,
            );
            if handle.is_null() {
                return None;
            }
            Some(Pulse {
                _lib: lib,
                handle,
                io,
                free,
            })
        }
    }
}

impl Drop for Pulse {
    fn drop(&mut self) {
        unsafe { (self.free)(self.handle) }
    }
}

impl Output for Pulse {
    fn kind(&self) -> Kind {
        Kind::Pulse
    }
    fn write(&mut self, stereo: &[f32]) -> bool {
        let mut err = 0;
        unsafe {
            (self.io)(
                self.handle,
                stereo.as_ptr() as *mut c_void,
                std::mem::size_of_val(stereo),
                &mut err,
            ) >= 0
        }
    }
}

impl Input for Pulse {
    fn read(&mut self, stereo: &mut [f32]) -> bool {
        let mut err = 0;
        unsafe {
            (self.io)(
                self.handle,
                stereo.as_mut_ptr() as *mut c_void,
                std::mem::size_of_val(stereo),
                &mut err,
            ) >= 0
        }
    }
}

// ---------------------------------------------------------------------------
// ALSA (périphérique « default », qui passe par PipeWire ou PulseAudio s'ils
// fournissent leur greffon ALSA).
// ---------------------------------------------------------------------------

type SndOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type SndSetParams = unsafe extern "C" fn(*mut c_void, c_int, c_int, u32, u32, c_int, u32) -> c_int;
type SndIo = unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> isize;
type SndRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type SndClose = unsafe extern "C" fn(*mut c_void) -> c_int;

pub struct Alsa {
    _lib: Lib,
    pcm: *mut c_void,
    io: SndIo,
    recover: SndRecover,
    close: SndClose,
}
unsafe impl Send for Alsa {}

impl Alsa {
    fn open(capture: bool) -> Option<Alsa> {
        let lib = Lib::open("libasound.so.2")?;
        unsafe {
            let open: SndOpen = load!(lib, "snd_pcm_open");
            let set: SndSetParams = load!(lib, "snd_pcm_set_params");
            let io: SndIo = if capture {
                load!(lib, "snd_pcm_readi")
            } else {
                load!(lib, "snd_pcm_writei")
            };
            let recover: SndRecover = load!(lib, "snd_pcm_recover");
            let close: SndClose = load!(lib, "snd_pcm_close");
            let mut pcm = std::ptr::null_mut();
            if open(&mut pcm, c"default".as_ptr(), c_int::from(capture), 0) < 0 {
                return None;
            }
            // SND_PCM_FORMAT_FLOAT_LE = 14, accès entrelacé = 3, 50 ms de latence.
            if set(pcm, 14, 3, 2, RATE, 1, 50_000) < 0 {
                close(pcm);
                return None;
            }
            Some(Alsa {
                _lib: lib,
                pcm,
                io,
                recover,
                close,
            })
        }
    }

    fn transfer(&mut self, ptr: *mut f32, frames: usize) -> bool {
        let mut done = 0usize;
        let mut failures = 0;
        while done < frames {
            let rc =
                unsafe { (self.io)(self.pcm, ptr.add(done * 2) as *mut c_void, frames - done) };
            if rc > 0 {
                done += rc as usize;
                continue;
            }
            if rc == -(libc::EAGAIN as isize) {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            failures += 1;
            if failures > 8 || unsafe { (self.recover)(self.pcm, rc as c_int, 1) } < 0 {
                return false;
            }
        }
        true
    }
}

impl Drop for Alsa {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.pcm);
        }
    }
}

impl Output for Alsa {
    fn kind(&self) -> Kind {
        Kind::Alsa
    }
    fn write(&mut self, stereo: &[f32]) -> bool {
        self.transfer(stereo.as_ptr() as *mut f32, stereo.len() / 2)
    }
}

impl Input for Alsa {
    fn read(&mut self, stereo: &mut [f32]) -> bool {
        self.transfer(stereo.as_mut_ptr(), stereo.len() / 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_suit_les_serveurs_presents() {
        assert_eq!(
            candidates_from(None, true, true),
            [Kind::PipeWire, Kind::Pulse, Kind::Alsa, Kind::Null]
        );
        assert_eq!(
            candidates_from(None, false, true),
            [Kind::Pulse, Kind::Alsa, Kind::Null]
        );
        assert_eq!(
            candidates_from(None, false, false),
            [Kind::Alsa, Kind::Null]
        );
        assert_eq!(
            candidates_from(Some("pulse"), true, true),
            [Kind::Pulse, Kind::Null]
        );
        assert_eq!(candidates_from(Some("null"), true, true), [Kind::Null]);
        assert_eq!(
            candidates_from(Some("inconnu"), false, false),
            [Kind::Alsa, Kind::Null]
        );
    }

    #[test]
    fn sortie_nulle_avance_en_temps_reel() {
        let mut n = Null::new();
        let start = Instant::now();
        let buf = vec![0.0f32; PERIOD * 2];
        for _ in 0..5 {
            assert!(Output::write(&mut n, &buf));
        }
        let ms = start.elapsed().as_millis();
        assert!((35..400).contains(&ms), "{ms} ms");
    }
}
