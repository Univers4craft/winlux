//! Décodeurs Linux optionnels pour PlaySound, sans DLL Windows.
//! ABI : mpg123.h et neaacdec.h (liens et périmètre dans docs/AUDIO-CODECS.md).
use super::{decode_wave, Wave};
use libc::{c_char, c_int, c_long, c_uchar, c_ulong, c_void};
use std::sync::OnceLock;

pub(super) const MAX_INPUT: usize = 32 * 1024 * 1024;
const MAX_SAMPLES: usize = 32 * 1024 * 1024; // 64 Mio de PCM par clip

struct Library(*mut c_void);
// dlopen est conservé pendant toute la vie des API ; seuls des pointeurs de
// fonctions immuables sont partagés, jamais les handles des décodeurs.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}
impl Library {
    unsafe fn open(name: &[u8]) -> Option<Self> {
        let p = super::dlopen(name.as_ptr().cast(), libc::RTLD_NOW | libc::RTLD_LOCAL);
        if p.is_null() {
            None
        } else {
            Some(Self(p))
        }
    }
    unsafe fn symbol(&self, name: &[u8]) -> Option<*mut c_void> {
        let p = super::dlsym(self.0, name.as_ptr().cast());
        if p.is_null() {
            None
        } else {
            Some(p)
        }
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            super::dlclose(self.0);
        }
    }
}
macro_rules! symbol {
    ($lib:expr, $name:literal, $ty:ty) => {
        std::mem::transmute::<*mut c_void, $ty>($lib.symbol(concat!($name, "\0").as_bytes())?)
    };
}

fn append_pcm(out: &mut Vec<i16>, input: &[i16], limit: usize) -> Option<()> {
    if out.len().checked_add(input.len())? > limit {
        return None;
    }
    out.try_reserve(input.len()).ok()?;
    out.extend_from_slice(input);
    Some(())
}
fn format_ok(rate: u32, channels: u16) -> bool {
    (8000..=192000).contains(&rate) && matches!(channels, 1 | 2)
}
fn finish(w: Wave) -> Option<Wave> {
    if !format_ok(w.rate, w.channels)
        || w.samples.is_empty()
        || w.samples.len() % w.channels as usize != 0
    {
        None
    } else {
        Some(w)
    }
}

pub(super) fn decode(raw: &[u8]) -> Option<Wave> {
    decode_with_limit(raw, MAX_SAMPLES)
}
fn decode_with_limit(raw: &[u8], limit: usize) -> Option<Wave> {
    if raw.len() > MAX_INPUT {
        return None;
    }
    if raw.starts_with(b"RIFF") {
        let wave = decode_wave(raw)?;
        return if wave.samples.len() <= limit {
            Some(wave)
        } else {
            None
        };
    }
    if raw.len() >= 2 && raw[0] == 0xff && raw[1] & 0xf6 == 0xf0 {
        decode_aac(raw, limit)
    } else if raw.starts_with(b"ID3") || (raw.len() >= 2 && raw[0] == 0xff && raw[1] & 0xe0 == 0xe0)
    {
        decode_mp3(raw, limit)
    } else {
        None
    }
}

type New = unsafe extern "C" fn(*const c_char, *mut c_int) -> *mut c_void;
type Delete = unsafe extern "C" fn(*mut c_void);
type Unary = unsafe extern "C" fn(*mut c_void) -> c_int;
type Feed = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> c_int;
type Read = unsafe extern "C" fn(*mut c_void, *mut u8, usize, *mut usize) -> c_int;
type GetFormat = unsafe extern "C" fn(*mut c_void, *mut c_long, *mut c_int, *mut c_int) -> c_int;
type Format = unsafe extern "C" fn(*mut c_void, c_long, c_int, c_int) -> c_int;
type Rates = unsafe extern "C" fn(*mut *const c_long, *mut usize);
struct Mpg123 {
    _lib: Library,
    new: New,
    delete: Delete,
    open: Unary,
    feed: Feed,
    read: Read,
    get_format: GetFormat,
    format_none: Unary,
    format: Format,
    rates: Rates,
}
impl Mpg123 {
    unsafe fn load() -> Option<Self> {
        let lib = Library::open(b"libmpg123.so.0\0")?;
        let api = Self {
            new: symbol!(lib, "mpg123_new", New),
            delete: symbol!(lib, "mpg123_delete", Delete),
            open: symbol!(lib, "mpg123_open_feed", Unary),
            feed: symbol!(lib, "mpg123_feed", Feed),
            read: symbol!(lib, "mpg123_read", Read),
            get_format: symbol!(lib, "mpg123_getformat", GetFormat),
            format_none: symbol!(lib, "mpg123_format_none", Unary),
            format: symbol!(lib, "mpg123_format", Format),
            rates: symbol!(lib, "mpg123_rates", Rates),
            _lib: lib,
        };
        let init = symbol!(api._lib, "mpg123_init", unsafe extern "C" fn() -> c_int);
        if init() != 0 {
            return None;
        }
        Some(api)
    }
}
// OnceLock initialise une seule fois, sans mpg123_exit concurrent ni dlclose
// pendant un décodage. La bibliothèque reste chargée jusqu'à la fin du processus.
static MPG: OnceLock<Option<Mpg123>> = OnceLock::new();
struct MpgHandle<'a> {
    ptr: *mut c_void,
    api: &'a Mpg123,
}
impl Drop for MpgHandle<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.api.delete)(self.ptr);
        }
    }
}
fn decode_mp3(raw: &[u8], limit: usize) -> Option<Wave> {
    let api = MPG.get_or_init(|| unsafe { Mpg123::load() }).as_ref()?;
    unsafe {
        let ptr = (api.new)(std::ptr::null(), std::ptr::null_mut());
        if ptr.is_null() {
            return None;
        }
        let _handle = MpgHandle { ptr, api };
        if (api.format_none)(ptr) != 0 {
            return None;
        }
        let mut rates = std::ptr::null();
        let mut count = 0;
        (api.rates)(&mut rates, &mut count);
        if rates.is_null() || count == 0 || count > 64 {
            return None;
        }
        for &rate in std::slice::from_raw_parts(rates, count) {
            // MPG123_ENC_SIGNED_16, PCM natif i16 (Linux x86-64 little endian).
            if (api.format)(ptr, rate, 3, 0xd0) != 0 {
                return None;
            }
        }
        if (api.open)(ptr) != 0 || (api.feed)(ptr, raw.as_ptr(), raw.len()) != 0 {
            return None;
        }
        let mut wave = Wave {
            rate: 0,
            channels: 0,
            samples: Vec::new(),
        };
        let mut buf = [0i16; 8192];
        let mut idle = 0;
        loop {
            let mut done = 0;
            let rc = (api.read)(
                ptr,
                buf.as_mut_ptr().cast(),
                std::mem::size_of_val(&buf),
                &mut done,
            );
            if done > std::mem::size_of_val(&buf) || done % 2 != 0 {
                return None;
            }
            if rc == -11 || wave.rate == 0 {
                let mut rate = 0;
                let mut channels = 0;
                let mut encoding = 0;
                if (api.get_format)(ptr, &mut rate, &mut channels, &mut encoding) != 0
                    || encoding != 0xd0
                    || rate <= 0
                    || rate > 192000
                    || !(1..=2).contains(&channels)
                {
                    return None;
                }
                let format = (rate as u32, channels as u16);
                if !format_ok(format.0, format.1) {
                    return None;
                }
                if wave.rate != 0 && (wave.rate, wave.channels) != format {
                    return None;
                }
                (wave.rate, wave.channels) = format;
            }
            append_pcm(&mut wave.samples, &buf[..done / 2], limit)?;
            if rc == -10 || rc == -12 {
                break;
            } // NEED_MORE / DONE : plus d'entrée
            if rc != 0 && rc != -11 {
                return None;
            }
            idle = if done == 0 { idle + 1 } else { 0 };
            if idle > 8 {
                return None;
            }
        }
        finish(wave)
    }
}

// Types C natifs, pas des tailles Windows : unsigned long vaut 8 octets sur LP64.
#[repr(C)]
struct FaadConfig {
    object_type: c_uchar,
    sample_rate: c_ulong,
    output_format: c_uchar,
    down_matrix: c_uchar,
    old_adts: c_uchar,
    no_implicit_sbr: c_uchar,
}
#[repr(C)]
struct FrameInfo {
    consumed: c_ulong,
    samples: c_ulong,
    channels: c_uchar,
    error: c_uchar,
    sample_rate: c_ulong,
    sbr: c_uchar,
    object_type: c_uchar,
    header_type: c_uchar,
    front: c_uchar,
    side: c_uchar,
    back: c_uchar,
    lfe: c_uchar,
    channel_position: [c_uchar; 64],
    ps: c_uchar,
}
type AacOpen = unsafe extern "C" fn() -> *mut c_void;
type AacConfig = unsafe extern "C" fn(*mut c_void) -> *mut FaadConfig;
type AacSetConfig = unsafe extern "C" fn(*mut c_void, *mut FaadConfig) -> c_uchar;
type AacInit =
    unsafe extern "C" fn(*mut c_void, *mut u8, c_ulong, *mut c_ulong, *mut c_uchar) -> c_long;
type AacDecode = unsafe extern "C" fn(*mut c_void, *mut FrameInfo, *mut u8, c_ulong) -> *mut c_void;
struct Faad {
    _lib: Library,
    open: AacOpen,
    close: Delete,
    config: AacConfig,
    set_config: AacSetConfig,
    init: AacInit,
    decode: AacDecode,
}
impl Faad {
    unsafe fn load() -> Option<Self> {
        let lib = Library::open(b"libfaad.so.2\0")?;
        Some(Self {
            open: symbol!(lib, "NeAACDecOpen", AacOpen),
            close: symbol!(lib, "NeAACDecClose", Delete),
            config: symbol!(lib, "NeAACDecGetCurrentConfiguration", AacConfig),
            set_config: symbol!(lib, "NeAACDecSetConfiguration", AacSetConfig),
            init: symbol!(lib, "NeAACDecInit", AacInit),
            decode: symbol!(lib, "NeAACDecDecode", AacDecode),
            _lib: lib,
        })
    }
}
static AAC: OnceLock<Option<Faad>> = OnceLock::new();
struct AacHandle<'a> {
    ptr: *mut c_void,
    api: &'a Faad,
}
impl Drop for AacHandle<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.api.close)(self.ptr);
        }
    }
}

fn validate_adts(mut raw: &[u8]) -> Option<usize> {
    let mut frames = 0;
    let mut expected_format = None;
    while !raw.is_empty() {
        if raw.len() < 7 || raw[0] != 0xff || raw[1] & 0xf6 != 0xf0 {
            return None;
        }
        let channels = ((raw[2] & 1) << 2) | (raw[3] >> 6);
        let format = (raw[2] & 0xfd, channels); // profil, fréquence, canaux ; ignorer private_bit
        if expected_format.is_some_and(|expected| expected != format) {
            return None;
        }
        expected_format = Some(format);
        if !(1..=2).contains(&channels) || (raw[2] >> 2) & 15 >= 13 {
            return None;
        }
        let len = ((raw[3] as usize & 3) << 11) | ((raw[4] as usize) << 3) | (raw[5] as usize >> 5);
        let header_len = if raw[1] & 1 == 0 { 9 } else { 7 };
        if len <= header_len || len > raw.len() {
            return None;
        }
        raw = &raw[len..];
        frames += 1;
        if frames > 32768 {
            return None;
        }
    }
    if frames == 0 {
        None
    } else {
        Some(frames)
    }
}
fn decode_aac(raw: &[u8], limit: usize) -> Option<Wave> {
    let frames = validate_adts(raw)?;
    let source_channels = ((raw[2] & 1) << 2) | (raw[3] >> 6);
    let api = AAC.get_or_init(|| unsafe { Faad::load() }).as_ref()?;
    // Marge lisible de 768 octets par canal, même sur la dernière trame.
    let mut input = raw.to_vec();
    input.resize(raw.len() + 1536, 0);
    unsafe {
        let ptr = (api.open)();
        if ptr.is_null() {
            return None;
        }
        let _handle = AacHandle { ptr, api };
        let config = (api.config)(ptr);
        if config.is_null() {
            return None;
        }
        (*config).output_format = 1; // FAAD_FMT_16BIT
                                     // Ne pas inventer du SBR/PS pour un flux AAC-LC basse fréquence.
        (*config).no_implicit_sbr = 1;
        if (api.set_config)(ptr, config) == 0 {
            return None;
        }
        let mut rate = 0;
        let mut channels = 0;
        let consumed = (api.init)(
            ptr,
            input.as_mut_ptr(),
            raw.len() as c_ulong,
            &mut rate,
            &mut channels,
        );
        if consumed < 0 || consumed as usize > raw.len() {
            return None;
        }
        let mut offset = consumed as usize;
        let mut wave = Wave {
            rate: 0,
            channels: 0,
            samples: Vec::new(),
        };
        for _ in 0..=frames {
            if offset == raw.len() {
                return finish(wave);
            }
            let mut info: FrameInfo = std::mem::zeroed();
            let out = (api.decode)(
                ptr,
                &mut info,
                input.as_mut_ptr().add(offset),
                (raw.len() - offset) as c_ulong,
            );
            if info.error != 0 || info.consumed == 0 || info.consumed as usize > raw.len() - offset
            {
                return None;
            }
            offset += info.consumed as usize;
            if info.samples == 0 {
                continue;
            }
            if out.is_null()
                || info.sample_rate > 192000
                || !format_ok(info.sample_rate as u32, info.channels as u16)
                || info.samples > 4096 * 2
            {
                return None;
            }
            // Certaines versions de FAAD dupliquent le mono en stéréo même
            // sans parametric stereo. Restituer alors le mono déclaré ADTS.
            let mono = source_channels == 1 && info.channels == 2 && info.ps == 0;
            let format = (
                info.sample_rate as u32,
                if mono { 1 } else { info.channels as u16 },
            );
            if wave.rate != 0 && (wave.rate, wave.channels) != format {
                return None;
            }
            (wave.rate, wave.channels) = format;
            let samples = std::slice::from_raw_parts(out.cast::<i16>(), info.samples as usize);
            if samples.len() % info.channels as usize != 0 {
                return None;
            }
            if mono {
                let mono_samples: Vec<i16> = samples
                    .chunks_exact(2)
                    .map(|pair| ((pair[0] as i32 + pair[1] as i32) / 2) as i16)
                    .collect();
                append_pcm(&mut wave.samples, &mono_samples, limit)?;
            } else {
                append_pcm(&mut wave.samples, samples, limit)?;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi_faad_et_bornes_pcm() {
        assert_eq!(std::mem::size_of::<FrameInfo>(), 104);
        assert_eq!(std::mem::offset_of!(FrameInfo, channel_position), 39);
        assert_eq!(std::mem::size_of::<FaadConfig>(), 24);
        let mut samples = vec![1, 2];
        assert!(append_pcm(&mut samples, &[3], 2).is_none());
        assert_eq!(samples, [1, 2]);
        assert!(decode(b"pas du son").is_none());
        let mut wav = Vec::from(&b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xac\0\0\x88\x58\x01\0\x02\0\x10\0data\x04\0\0\0\x00\x80\xff\x7f"[..]);
        wav[24..28].copy_from_slice(&4000u32.to_le_bytes());
        assert_eq!(decode(&wav).unwrap().rate, 4000); // garder le périmètre WAVE existant
        assert!(decode_with_limit(&wav, 1).is_none());
        assert!(unsafe { Library::open(b"libwinlux-codec-inexistant.so\0") }.is_none());
        assert!(validate_adts(&[0xff, 0xf1, 0, 0, 0, 0, 0]).is_none());
    }
    #[test]
    #[ignore = "nécessite libmpg123.so.0 et libfaad.so.2 ; exécuté explicitement en CI"]
    fn audio_codecs_fichiers_reels_limites_et_concurrence() {
        let mp3 = include_bytes!("../../tests/audio/tone.mp3");
        let aac = include_bytes!("../../tests/audio/tone.aac");
        let mono_mp3 = include_bytes!("../../tests/audio/mono.mp3");
        let mono_aac = include_bytes!("../../tests/audio/mono.aac");
        for (data, rate, channels) in [
            (mp3.as_slice(), 44100, 2),
            (aac.as_slice(), 44100, 2),
            (mono_mp3.as_slice(), 22050, 1),
            (mono_aac.as_slice(), 22050, 1),
        ] {
            let w = decode(data).expect("bibliothèque absente ou échec de décodage");
            assert_eq!((w.rate, w.channels), (rate, channels));
            assert!(
                w.samples.len() > rate as usize * channels as usize / 12
                    && w.samples.len() < rate as usize * channels as usize / 2
            );
            assert!(w.samples.iter().any(|s| s.unsigned_abs() > 100));
            assert!(decode_with_limit(data, 16).is_none());
            assert!(decode(&data[..7]).is_none());
        }
        assert!(decode(&aac[..aac.len() - 1]).is_none());
        let changed_format = [aac.as_slice(), mono_aac.as_slice()].concat();
        assert!(decode(&changed_format).is_none());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    assert!(decode(mp3).is_some());
                    assert!(decode(aac).is_some());
                });
            }
        });
    }

    #[test]
    #[ignore = "nécessite les codecs et ALSA_CONFIG_PATH=runtime/tests/audio/alsa-null.conf"]
    fn audio_codecs_play_sound_a_w_vers_alsa_null() {
        // Test explicite du chemin public Win64, sans haut-parleurs ni carte son.
        let expected =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/audio/alsa-null.conf");
        assert_eq!(
            std::env::var_os("ALSA_CONFIG_PATH").map(std::path::PathBuf::from),
            Some(expected)
        );
        for extension in ["mp3", "aac"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/audio/tone.{extension}"));
            let name = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
            let wide: Vec<u16> = path
                .to_str()
                .unwrap()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            unsafe {
                assert_eq!(
                    super::super::play_sound_a(name.as_ptr().cast(), 0, 0x20000),
                    1
                );
                assert_eq!(super::super::play_sound_w(wide.as_ptr(), 0, 0x20000), 1);
            }
        }
    }
}
