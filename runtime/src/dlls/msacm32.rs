//! msacm32.dll (#237) : le gestionnaire de compression audio, réduit à ce
//! dont les jeux se servent pour préparer leurs sons : les conversions PCM.
//!
//! - `acmStream*` convertit entre PCM entier (8 bits non signé, 16, 24 et
//!   32 bits) et flottant 32 bits, WAVEFORMATEXTENSIBLE compris : format
//!   des échantillons, nombre de canaux (mono ↔ stéréo, plus de canaux en
//!   gardant les premiers) et fréquence (interpolation linéaire, continue
//!   d'un tampon à l'autre).
//! - `acmFormatSuggest` propose le PCM correspondant, `acmStreamSize`
//!   donne la taille de sortie, `acmMetrics` et `acmFormatTagDetails`
//!   décrivent le format PCM.
//! - Pas de codec compressé : un format source ou cible autre que PCM ou
//!   flottant répond ACMERR_NOTPOSSIBLE, comme un Windows sans le codec.
//! - ACM_STREAMOPENF_ASYNC : la conversion reste immédiate, puis
//!   MM_ACM_DONE part par le rappel (fonction, événement, fenêtre), avec
//!   MM_ACM_OPEN et MM_ACM_CLOSE ; passerelle wow32 pour le 32 bits.
//! - ACMSTREAMHEADER est lu dans sa disposition x86 ou x64.

use super::winmm_midi::{self as mm, Callback};
use crate::dlls::lock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const MMSYSERR_NOERROR: u32 = 0;
const MMSYSERR_INVALHANDLE: u32 = 5;
const MMSYSERR_NOTSUPPORTED: u32 = 8;
const MMSYSERR_INVALFLAG: u32 = 10;
const MMSYSERR_INVALPARAM: u32 = 11;
const ACMERR_NOTPOSSIBLE: u32 = 512;
const ACMERR_BUSY: u32 = 513;
const ACMERR_UNPREPARED: u32 = 514;

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

const ACM_STREAMOPENF_QUERY: u32 = 0x1;
const ACM_STREAMOPENF_ASYNC: u32 = 0x2;
const ACM_STREAMOPENF_NONREALTIME: u32 = 0x4;
const ACM_STREAMSIZEF_DESTINATION: u32 = 0x1;
const ACM_STREAMCONVERTF_BLOCKALIGN: u32 = 0x4;
const ACM_STREAMCONVERTF_START: u32 = 0x10;
const ACM_STREAMCONVERTF_END: u32 = 0x20;

const STATUSF_DONE: u32 = 0x0001_0000;
const STATUSF_PREPARED: u32 = 0x0002_0000;
const STATUSF_INQUEUE: u32 = 0x0010_0000;

const ACM_FORMATSUGGESTF_WFORMATTAG: u32 = 0x0001_0000;
const ACM_FORMATSUGGESTF_NCHANNELS: u32 = 0x0002_0000;
const ACM_FORMATSUGGESTF_NSAMPLESPERSEC: u32 = 0x0004_0000;
const ACM_FORMATSUGGESTF_WBITSPERSAMPLE: u32 = 0x0008_0000;

const MM_ACM_OPEN: u32 = 0x3D4;
const MM_ACM_CLOSE: u32 = 0x3D5;
const MM_ACM_DONE: u32 = 0x3D6;

/// Format d'échantillon lu dans un WAVEFORMATEX.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sample {
    U8,
    I16,
    I24,
    I32,
    F32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Format {
    sample: Sample,
    channels: usize,
    rate: u32,
    /// nBlockAlign : octets par trame.
    block: usize,
}

impl Format {
    fn width(self) -> usize {
        match self.sample {
            Sample::U8 => 1,
            Sample::I16 => 2,
            Sample::I24 => 3,
            Sample::I32 | Sample::F32 => 4,
        }
    }
}

unsafe fn rd16(p: *const u8, off: usize) -> u16 {
    std::ptr::read_unaligned(p.add(off) as *const u16)
}
unsafe fn rd32(p: *const u8, off: usize) -> u32 {
    std::ptr::read_unaligned(p.add(off) as *const u32)
}

/// Lit un WAVEFORMATEX : `None` si ce n'est ni du PCM ni du flottant.
unsafe fn read_format(p: *const u8) -> Option<Format> {
    if p.is_null() {
        return None;
    }
    let mut tag = rd16(p, 0);
    let channels = rd16(p, 2) as usize;
    let rate = rd32(p, 4);
    let block = rd16(p, 12) as usize;
    let bits = rd16(p, 14);
    if tag == WAVE_FORMAT_EXTENSIBLE {
        // SubFormat (GUID) à l'octet 24 ; ses deux premiers octets portent
        // l'étiquette du format.
        if rd16(p, 16) < 22 {
            return None;
        }
        tag = rd16(p, 24);
    }
    let sample = match (tag, bits) {
        (WAVE_FORMAT_PCM, 8) => Sample::U8,
        (WAVE_FORMAT_PCM, 16) => Sample::I16,
        (WAVE_FORMAT_PCM, 24) => Sample::I24,
        (WAVE_FORMAT_PCM, 32) => Sample::I32,
        (WAVE_FORMAT_IEEE_FLOAT, 32) => Sample::F32,
        _ => return None,
    };
    let f = Format {
        sample,
        channels,
        rate,
        block,
    };
    if channels == 0 || channels > 8 || !(100..=384_000).contains(&rate) {
        return None;
    }
    if block != channels * f.width() {
        return None;
    }
    Some(f)
}

fn decode(f: Format, b: &[u8]) -> f32 {
    match f.sample {
        Sample::U8 => (b[0] as f32 - 128.0) / 128.0,
        Sample::I16 => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
        Sample::I24 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
        Sample::I32 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
        Sample::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    }
}

fn encode(f: Format, v: f32, out: &mut Vec<u8>) {
    let v = if v.is_nan() { 0.0 } else { v };
    match f.sample {
        Sample::U8 => out.push((v * 128.0 + 128.0).round().clamp(0.0, 255.0) as u8),
        Sample::I16 => out.extend_from_slice(
            &((v * 32768.0).round().clamp(-32768.0, 32767.0) as i16).to_le_bytes(),
        ),
        Sample::I24 => {
            let s = (v as f64 * 8_388_608.0)
                .round()
                .clamp(-8_388_608.0, 8_388_607.0) as i32;
            out.extend_from_slice(&s.to_le_bytes()[..3]);
        }
        Sample::I32 => {
            let s = (v as f64 * 2_147_483_648.0)
                .round()
                .clamp(-2_147_483_648.0, 2_147_483_647.0) as i32;
            out.extend_from_slice(&s.to_le_bytes());
        }
        Sample::F32 => out.extend_from_slice(&v.to_le_bytes()),
    }
}

/// Une trame source passée aux canaux de la cible.
fn remap(src: &[f32], channels: usize) -> Vec<f32> {
    match (src.len(), channels) {
        (a, b) if a == b => src.to_vec(),
        (1, n) => vec![src[0]; n],
        (n, 1) => vec![src.iter().sum::<f32>() / n as f32],
        (_, n) => (0..n).map(|i| src.get(i).copied().unwrap_or(0.0)).collect(),
    }
}

/// État d'une conversion, gardé d'un tampon à l'autre.
struct Converter {
    src: Format,
    dst: Format,
    /// Position de la prochaine trame de sortie, en trames source, par
    /// rapport au début du tampon courant (-1 = `prev`).
    t: f64,
    prev: Option<Vec<f32>>,
}

impl Converter {
    fn new(src: Format, dst: Format) -> Converter {
        Converter {
            src,
            dst,
            t: 0.0,
            prev: None,
        }
    }

    fn reset(&mut self) {
        self.t = 0.0;
        self.prev = None;
    }

    fn frame(&self, input: &[u8], i: usize) -> Vec<f32> {
        let w = self.src.width();
        let base = i * self.src.block;
        let f: Vec<f32> = (0..self.src.channels)
            .map(|c| decode(self.src, &input[base + c * w..]))
            .collect();
        remap(&f, self.dst.channels)
    }

    /// Convertit `input` (trames entières) ; au plus `room` trames de sortie.
    /// Rend (octets source consommés, octets produits).
    fn convert(&mut self, input: &[u8], room: usize, end: bool) -> (usize, Vec<u8>) {
        let n = input.len() / self.src.block;
        let mut out = Vec::with_capacity(room * self.dst.block);
        if self.src.rate == self.dst.rate {
            let k = n.min(room);
            for i in 0..k {
                for v in self.frame(input, i) {
                    encode(self.dst, v, &mut out);
                }
            }
            return (k * self.src.block, out);
        }
        let step = self.src.rate as f64 / self.dst.rate as f64;
        let mut produced = 0;
        let mut exhausted = false;
        let has = |j: isize, prev: bool| (j == -1 && prev) || (j >= 0 && (j as usize) < n);
        loop {
            let i = self.t.floor();
            let (ii, next) = (i as isize, i as isize + 1);
            let prev = self.prev.is_some();
            if !has(ii, prev) || (!has(next, prev) && !end) {
                exhausted = true;
                break;
            }
            if produced == room {
                break;
            }
            let a = match (ii, &self.prev) {
                (-1, Some(p)) => p.clone(),
                _ => self.frame(input, ii as usize),
            };
            let b = if has(next, prev) {
                self.frame(input, next as usize)
            } else {
                a.clone() // fin du flux : dernière trame tenue
            };
            let x = (self.t - i) as f32;
            for (va, vb) in a.iter().zip(&b) {
                encode(self.dst, va + (vb - va) * x, &mut out);
            }
            produced += 1;
            self.t += step;
        }
        let consumed = if exhausted {
            n
        } else {
            (self.t.floor().max(0.0) as usize).min(n)
        };
        if consumed > 0 {
            self.prev = Some(self.frame(input, consumed - 1));
            self.t -= consumed as f64;
        }
        (consumed * self.src.block, out)
    }

    /// Octets de sortie pour `bytes` octets source (ou l'inverse).
    fn size(&self, bytes: usize, from_dst: bool) -> usize {
        let (a, b) = if from_dst {
            (self.dst, self.src)
        } else {
            (self.src, self.dst)
        };
        let frames = bytes / a.block;
        let out = (frames as u64 * b.rate as u64).div_ceil(a.rate as u64) as usize;
        let extra = usize::from(self.src.rate != self.dst.rate);
        (out + extra) * b.block
    }
}

struct AcmStream {
    handle: usize,
    conv: Mutex<Converter>,
    callback: Option<Callback>,
}

impl AcmStream {
    fn notify(&self, msg: u32, p1: usize) {
        if let Some(cb) = self.callback {
            mm::send(cb, self.handle, msg, p1, 0);
        }
    }
}

fn table() -> &'static Mutex<HashMap<usize, Arc<AcmStream>>> {
    static T: OnceLock<Mutex<HashMap<usize, Arc<AcmStream>>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get(h: usize) -> Option<Arc<AcmStream>> {
    lock(table()).get(&h).cloned()
}

pub extern "win64" fn acm_get_version() -> u32 {
    0x0402_0000
}

const ACM_METRIC_COUNT_DRIVERS: u32 = 1;
const ACM_METRIC_COUNT_LOCAL_DISABLED: u32 = 25;
const ACM_METRIC_MAX_SIZE_FORMAT: u32 = 50;
const ACM_METRIC_MAX_SIZE_FILTER: u32 = 51;
const ACM_METRIC_DRIVER_SUPPORT: u32 = 100;
const ACM_METRIC_DRIVER_PRIORITY: u32 = 101;

/// acmMetrics(hao, uMetric, pMetric) : aucun pilote installé (les
/// conversions PCM sont intégrées), formats jusqu'à WAVEFORMATEXTENSIBLE.
pub unsafe extern "win64" fn acm_metrics(_hao: usize, metric: u32, out: *mut u32) -> u32 {
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    *out = match metric {
        ACM_METRIC_COUNT_DRIVERS..=6 | 20..=ACM_METRIC_COUNT_LOCAL_DISABLED => 0,
        ACM_METRIC_MAX_SIZE_FORMAT => 40,
        ACM_METRIC_MAX_SIZE_FILTER => 8,
        ACM_METRIC_DRIVER_SUPPORT | ACM_METRIC_DRIVER_PRIORITY => return MMSYSERR_INVALHANDLE,
        _ => return MMSYSERR_NOTSUPPORTED,
    };
    MMSYSERR_NOERROR
}

/// acmDriverEnum : aucun pilote à énumérer.
pub extern "win64" fn acm_driver_enum(_cb: usize, _instance: usize, flags: u32) -> u32 {
    // ACM_DRIVERENUMF_NOLOCAL | ACM_DRIVERENUMF_DISABLED
    if flags & !0xC000_0000 != 0 {
        return MMSYSERR_INVALFLAG;
    }
    MMSYSERR_NOERROR
}

unsafe fn write_pcm(p: *mut u8, tag: u16, channels: u16, rate: u32, bits: u16) {
    let block = channels * bits / 8;
    std::ptr::write_unaligned(p as *mut u16, tag);
    std::ptr::write_unaligned(p.add(2) as *mut u16, channels);
    std::ptr::write_unaligned(p.add(4) as *mut u32, rate);
    std::ptr::write_unaligned(p.add(8) as *mut u32, rate * block as u32);
    std::ptr::write_unaligned(p.add(12) as *mut u16, block);
    std::ptr::write_unaligned(p.add(14) as *mut u16, bits);
    std::ptr::write_unaligned(p.add(16) as *mut u16, 0);
}

/// acmFormatSuggest(had, pwfxSrc, pwfxDst, cbwfxDst, fdwSuggest) : du PCM
/// avec les champs imposés par les drapeaux, les autres repris de la source.
pub unsafe extern "win64" fn acm_format_suggest(
    _had: usize,
    src: *const u8,
    dst: *mut u8,
    size: u32,
    flags: u32,
) -> u32 {
    if src.is_null() || dst.is_null() || size < 16 {
        return MMSYSERR_INVALPARAM;
    }
    if flags & !0x000F_0000 != 0 {
        return MMSYSERR_INVALFLAG;
    }
    let Some(s) = read_format(src) else {
        return ACMERR_NOTPOSSIBLE;
    };
    let pick = |f: u32, off: usize, bytes: usize, default: u32| -> u32 {
        if flags & f == 0 {
            return default;
        }
        if bytes == 2 {
            rd16(dst, off) as u32
        } else {
            rd32(dst, off)
        }
    };
    let tag = pick(ACM_FORMATSUGGESTF_WFORMATTAG, 0, 2, WAVE_FORMAT_PCM as u32) as u16;
    let channels = pick(ACM_FORMATSUGGESTF_NCHANNELS, 2, 2, s.channels as u32) as u16;
    let rate = pick(ACM_FORMATSUGGESTF_NSAMPLESPERSEC, 4, 4, s.rate);
    let default_bits = match (tag, s.sample) {
        (WAVE_FORMAT_IEEE_FLOAT, _) => 32,
        (_, Sample::U8) => 8,
        (_, Sample::I24) => 24,
        (_, Sample::I32) => 32,
        _ => 16,
    };
    let bits = pick(ACM_FORMATSUGGESTF_WBITSPERSAMPLE, 14, 2, default_bits) as u16;
    let mut probe = [0u8; 18];
    write_pcm(probe.as_mut_ptr(), tag, channels, rate, bits);
    if read_format(probe.as_ptr()).is_none() {
        return ACMERR_NOTPOSSIBLE;
    }
    let n = (size as usize).min(18);
    std::ptr::copy_nonoverlapping(probe.as_ptr(), dst, n);
    MMSYSERR_NOERROR
}

/// acmFormatTagDetails(A/W)(had, padftd, fdwDetails) : seul PCM est décrit.
/// ACMFORMATTAGDETAILS : cbStruct, dwFormatTagIndex, dwFormatTag,
/// cbFormatSize, fdwSupport, cStandardFormats, szFormatTag[48].
unsafe fn format_tag_details(d: *mut u8, flags: u32, wide: bool) -> u32 {
    const INDEX: u32 = 0;
    const FORMATTAG: u32 = 1;
    const LARGESTSIZE: u32 = 2;
    if d.is_null() || rd32(d, 0) < 24 {
        return MMSYSERR_INVALPARAM;
    }
    let ok = match flags & 0xF {
        INDEX => rd32(d, 4) == 0,
        FORMATTAG => rd32(d, 8) == WAVE_FORMAT_PCM as u32,
        LARGESTSIZE => matches!(rd32(d, 8), 0 | 1),
        _ => return MMSYSERR_INVALFLAG,
    };
    if !ok {
        return ACMERR_NOTPOSSIBLE;
    }
    let size = rd32(d, 0) as usize;
    let fields = [0u32, WAVE_FORMAT_PCM as u32, 16, 0x8, 20];
    for (i, v) in fields.iter().enumerate() {
        std::ptr::write_unaligned(d.add(4 + 4 * i) as *mut u32, *v);
    }
    let name = "PCM";
    if wide {
        let mut buf = [0u16; 48];
        for (i, c) in name.encode_utf16().enumerate() {
            buf[i] = c;
        }
        let n = size.saturating_sub(24).min(96);
        std::ptr::copy_nonoverlapping(buf.as_ptr() as *const u8, d.add(24), n);
    } else {
        let mut buf = [0u8; 48];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        let n = size.saturating_sub(24).min(48);
        std::ptr::copy_nonoverlapping(buf.as_ptr(), d.add(24), n);
    }
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn acm_format_tag_details_a(_had: usize, d: *mut u8, flags: u32) -> u32 {
    format_tag_details(d, flags, false)
}

pub unsafe extern "win64" fn acm_format_tag_details_w(_had: usize, d: *mut u8, flags: u32) -> u32 {
    format_tag_details(d, flags, true)
}

/// acmStreamOpen(phas, had, pwfxSrc, pwfxDst, pwfltr, dwCallback,
/// dwInstance, fdwOpen).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn acm_stream_open(
    phas: *mut usize,
    _had: usize,
    src: *const u8,
    dst: *const u8,
    filter: *const u8,
    callback: usize,
    instance: usize,
    flags: u32,
) -> u32 {
    let query = flags & ACM_STREAMOPENF_QUERY != 0;
    if src.is_null() || dst.is_null() || (!query && phas.is_null()) {
        return MMSYSERR_INVALPARAM;
    }
    let known = ACM_STREAMOPENF_QUERY | ACM_STREAMOPENF_ASYNC | ACM_STREAMOPENF_NONREALTIME;
    if flags & !(known | 0x7_0000) != 0 {
        return MMSYSERR_INVALFLAG;
    }
    if !phas.is_null() {
        *phas = 0;
    }
    let (Some(s), Some(d)) = (read_format(src), read_format(dst)) else {
        return ACMERR_NOTPOSSIBLE;
    };
    if !filter.is_null() {
        return ACMERR_NOTPOSSIBLE; // pas de filtre
    }
    if query {
        return MMSYSERR_NOERROR;
    }
    let cb = if flags & ACM_STREAMOPENF_ASYNC != 0 {
        match mm::callback(callback, instance, flags) {
            Ok(c) => Some(c),
            Err(e) => return e,
        }
    } else {
        None
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0x7743_0000);
    let handle = NEXT.fetch_add(4, Ordering::Relaxed);
    let st = Arc::new(AcmStream {
        handle,
        conv: Mutex::new(Converter::new(s, d)),
        callback: cb,
    });
    lock(table()).insert(handle, st.clone());
    *phas = handle;
    st.notify(MM_ACM_OPEN, 0);
    MMSYSERR_NOERROR
}

pub extern "win64" fn acm_stream_close(h: usize, flags: u32) -> u32 {
    if flags != 0 {
        return MMSYSERR_INVALFLAG;
    }
    let Some(st) = lock(table()).remove(&h) else {
        return MMSYSERR_INVALHANDLE;
    };
    st.notify(MM_ACM_CLOSE, 0);
    MMSYSERR_NOERROR
}

/// acmStreamSize(has, cbInput, pdwOutputBytes, fdwSize).
pub unsafe extern "win64" fn acm_stream_size(
    h: usize,
    input: u32,
    out: *mut u32,
    flags: u32,
) -> u32 {
    let Some(st) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if out.is_null() || flags > ACM_STREAMSIZEF_DESTINATION {
        return MMSYSERR_INVALPARAM;
    }
    let c = lock(&st.conv);
    let from_dst = flags == ACM_STREAMSIZEF_DESTINATION;
    let block = if from_dst { c.dst.block } else { c.src.block };
    if (input as usize) < block {
        *out = 0;
        return ACMERR_NOTPOSSIBLE;
    }
    *out = c.size(input as usize, from_dst).min(u32::MAX as usize) as u32;
    MMSYSERR_NOERROR
}

// ACMSTREAMHEADER x64 : cbStruct @0, fdwStatus @4, dwUser @8, pbSrc @16,
// cbSrcLength @24, cbSrcLengthUsed @28, dwSrcUser @32, pbDst @40,
// cbDstLength @48, cbDstLengthUsed @52, dwDstUser @56, réservé @64.
// x86 : cbStruct @0, fdwStatus @4, dwUser @8, pbSrc @12, cbSrcLength @16,
// cbSrcLengthUsed @20, dwSrcUser @24, pbDst @28, cbDstLength @32,
// cbDstLengthUsed @36, dwDstUser @40, réservé @44.
struct Header {
    base: *mut u8,
    x86: bool,
}

impl Header {
    fn new(p: *mut u8) -> Header {
        Header {
            base: p,
            x86: mm::x86(),
        }
    }
    fn min_size(&self) -> u32 {
        if self.x86 {
            84
        } else {
            124
        }
    }
    unsafe fn u32_at(&self, x86: usize, x64: usize) -> *mut u32 {
        self.base.add(if self.x86 { x86 } else { x64 }) as *mut u32
    }
    unsafe fn ptr_at(&self, x86: usize, x64: usize) -> *mut u8 {
        if self.x86 {
            rd32(self.base, x86) as usize as *mut u8
        } else {
            std::ptr::read_unaligned(self.base.add(x64) as *const *mut u8)
        }
    }
    unsafe fn size(&self) -> u32 {
        rd32(self.base, 0)
    }
    unsafe fn status(&self) -> *mut u32 {
        self.base.add(4) as *mut u32
    }
    unsafe fn src(&self) -> *mut u8 {
        self.ptr_at(12, 16)
    }
    unsafe fn src_len(&self) -> *mut u32 {
        self.u32_at(16, 24)
    }
    unsafe fn src_used(&self) -> *mut u32 {
        self.u32_at(20, 28)
    }
    unsafe fn dst(&self) -> *mut u8 {
        self.ptr_at(28, 40)
    }
    unsafe fn dst_len(&self) -> *mut u32 {
        self.u32_at(32, 48)
    }
    unsafe fn dst_used(&self) -> *mut u32 {
        self.u32_at(36, 52)
    }
}

unsafe fn check_header(h: usize, hdr: *mut u8) -> Result<(Arc<AcmStream>, Header), u32> {
    let Some(st) = get(h) else {
        return Err(MMSYSERR_INVALHANDLE);
    };
    if hdr.is_null() {
        return Err(MMSYSERR_INVALPARAM);
    }
    let hd = Header::new(hdr);
    if hd.size() < hd.min_size() {
        return Err(MMSYSERR_INVALPARAM);
    }
    Ok((st, hd))
}

pub unsafe extern "win64" fn acm_stream_prepare_header(h: usize, hdr: *mut u8, flags: u32) -> u32 {
    if flags != 0 {
        return MMSYSERR_INVALFLAG;
    }
    let hd = match check_header(h, hdr) {
        Ok((_, hd)) => hd,
        Err(e) => return e,
    };
    if hd.src().is_null() || hd.dst().is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let s = hd.status();
    if *s & STATUSF_INQUEUE != 0 {
        return ACMERR_BUSY;
    }
    *s = (*s & !STATUSF_DONE) | STATUSF_PREPARED;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn acm_stream_unprepare_header(
    h: usize,
    hdr: *mut u8,
    flags: u32,
) -> u32 {
    if flags != 0 {
        return MMSYSERR_INVALFLAG;
    }
    let hd = match check_header(h, hdr) {
        Ok((_, hd)) => hd,
        Err(e) => return e,
    };
    let s = hd.status();
    if *s & STATUSF_PREPARED == 0 {
        return ACMERR_UNPREPARED;
    }
    if *s & STATUSF_INQUEUE != 0 {
        return ACMERR_BUSY;
    }
    *s &= !STATUSF_PREPARED;
    MMSYSERR_NOERROR
}

/// acmStreamConvert(has, pash, fdwConvert).
pub unsafe extern "win64" fn acm_stream_convert(h: usize, hdr: *mut u8, flags: u32) -> u32 {
    let known = ACM_STREAMCONVERTF_BLOCKALIGN | ACM_STREAMCONVERTF_START | ACM_STREAMCONVERTF_END;
    if flags & !known != 0 {
        return MMSYSERR_INVALFLAG;
    }
    let (st, hd) = match check_header(h, hdr) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let s = hd.status();
    if *s & STATUSF_PREPARED == 0 {
        return ACMERR_UNPREPARED;
    }
    if *s & STATUSF_INQUEUE != 0 {
        return ACMERR_BUSY;
    }
    *s &= !STATUSF_DONE;
    {
        let mut c = lock(&st.conv);
        if flags & ACM_STREAMCONVERTF_START != 0 {
            c.reset();
        }
        let src_len = *hd.src_len() as usize;
        let whole = src_len - src_len % c.src.block;
        let input = std::slice::from_raw_parts(hd.src() as *const u8, whole);
        let room = *hd.dst_len() as usize / c.dst.block;
        let end = flags & ACM_STREAMCONVERTF_END != 0;
        let (used, out) = c.convert(input, room, end);
        std::ptr::copy_nonoverlapping(out.as_ptr(), hd.dst(), out.len());
        *hd.src_used() = used as u32;
        *hd.dst_used() = out.len() as u32;
    }
    *s |= STATUSF_DONE;
    st.notify(MM_ACM_DONE, hdr as usize);
    MMSYSERR_NOERROR
}

/// acmStreamReset : rien n'est jamais en attente, la conversion étant
/// immédiate.
pub extern "win64" fn acm_stream_reset(h: usize, flags: u32) -> u32 {
    if flags != 0 {
        return MMSYSERR_INVALFLAG;
    }
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    MMSYSERR_NOERROR
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "acmGetVersion" => fn acm_get_version,
        "acmMetrics" => fn acm_metrics,
        "acmDriverEnum" => fn acm_driver_enum,
        "acmFormatSuggest" => fn acm_format_suggest,
        "acmFormatTagDetailsA" => fn acm_format_tag_details_a,
        "acmFormatTagDetailsW" => fn acm_format_tag_details_w,
        "acmStreamOpen" => fn acm_stream_open,
        "acmStreamClose" => fn acm_stream_close,
        "acmStreamSize" => fn acm_stream_size,
        "acmStreamPrepareHeader" => fn acm_stream_prepare_header,
        "acmStreamUnprepareHeader" => fn acm_stream_unprepare_header,
        "acmStreamConvert" => fn acm_stream_convert,
        "acmStreamReset" => fn acm_stream_reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm(sample: Sample, channels: usize, rate: u32) -> Format {
        let mut f = Format {
            sample,
            channels,
            rate,
            block: 0,
        };
        f.block = channels * f.width();
        f
    }

    #[test]
    fn huit_bits_mono_vers_seize_bits_stereo() {
        let mut c = Converter::new(pcm(Sample::U8, 1, 8000), pcm(Sample::I16, 2, 8000));
        let (used, out) = c.convert(&[128, 255, 0], 10, false);
        assert_eq!(used, 3);
        let v: Vec<i16> = out
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(v, vec![0, 0, 32512, 32512, -32768, -32768]);
    }

    #[test]
    fn stereo_vers_mono_et_flottant() {
        let mut c = Converter::new(pcm(Sample::I16, 2, 44100), pcm(Sample::F32, 1, 44100));
        let mut input = Vec::new();
        for s in [16384i16, -16384, 16384, 16384] {
            input.extend_from_slice(&s.to_le_bytes());
        }
        let (used, out) = c.convert(&input, 10, false);
        assert_eq!(used, 8);
        let v: Vec<f32> = out
            .chunks(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        assert_eq!(v, vec![0.0, 0.5]);
    }

    #[test]
    fn frequence_doublee_continue_entre_tampons() {
        let src = pcm(Sample::I16, 1, 11025);
        let dst = pcm(Sample::I16, 1, 22050);
        let mut c = Converter::new(src, dst);
        let ramp: Vec<u8> = (0..8i16).flat_map(|i| (i * 1000).to_le_bytes()).collect();
        let mut all = Vec::new();
        for chunk in ramp.chunks(8) {
            let (used, out) = c.convert(chunk, 100, false);
            assert_eq!(used, chunk.len());
            all.extend(out);
        }
        let (_, tail) = c.convert(&[], 100, true);
        all.extend(tail);
        let v: Vec<i16> = all
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        // Une rampe reste une rampe, demi-pas compris, sans saut entre tampons.
        let want: Vec<i16> = (0..14).map(|i| i * 500).collect();
        assert_eq!(&v[..14], &want[..]);
        assert_eq!(c.size(8, false), 18);
    }

    #[test]
    fn sortie_limitee_ne_consomme_que_le_necessaire() {
        let mut c = Converter::new(pcm(Sample::I16, 1, 48000), pcm(Sample::I16, 1, 24000));
        let input = vec![0u8; 200]; // 100 trames
        let (used, out) = c.convert(&input, 10, false);
        assert_eq!(out.len(), 20);
        assert_eq!(used, 40); // 20 trames source pour 10 trames de sortie
    }

    #[test]
    fn formats_refuses() {
        let mut adpcm = [0u8; 20];
        unsafe {
            write_pcm(adpcm.as_mut_ptr(), 2, 1, 22050, 4);
            assert!(read_format(adpcm.as_ptr()).is_none());
            write_pcm(adpcm.as_mut_ptr(), 1, 2, 44100, 16);
            assert_eq!(
                read_format(adpcm.as_ptr()),
                Some(pcm(Sample::I16, 2, 44100))
            );
        }
    }
}
