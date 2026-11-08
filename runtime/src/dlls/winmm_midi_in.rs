//! winmm, entrée MIDI (#237) : `midiIn*` lit les claviers et interfaces
//! MIDI branchés, à travers les périphériques bruts du noyau
//! (/dev/snd/midiC<carte>D<périphérique>), sans bibliothèque.
//!
//! - Un périphérique par fichier midiC*D*, dans l'ordre carte puis numéro ;
//!   le nom vient de /proc/asound/card<C>/midi<D>. `WINLUX_MIDI_IN_DIR`
//!   remplace /dev/snd (tests sans matériel : un simple fichier suffit).
//! - Un fil de lecture par périphérique ouvert. Rien n'est livré avant
//!   midiInStart ; ensuite chaque message court part en MIM_DATA (statut
//!   courant, messages temps réel intercalés), avec l'heure en
//!   millisecondes depuis midiInStart.
//! - Le système exclusif remplit les tampons donnés par midiInAddBuffer et
//!   part en MIM_LONGDATA quand le tampon est plein ou à F7 ; un système
//!   exclusif coupé par un autre statut part en MIM_LONGERROR.
//! - midiInStop rend le tampon entamé, midiInReset rend tous les tampons.
//! - Les rappels sont ceux de midiOut (fonction, événement, fenêtre,
//!   thread), avec la passerelle wow32 pour les programmes 32 bits.

use super::winmm_midi::{
    self as out, Callback, MHDR_DONE, MHDR_INQUEUE, MHDR_PREPARED, MIDIERR_STILLPLAYING,
    MIDIERR_UNPREPARED, MMSYSERR_BADDEVICEID, MMSYSERR_INVALHANDLE, MMSYSERR_INVALPARAM,
    MMSYSERR_NOERROR,
};
use crate::dlls::lock;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

const MMSYSERR_ALLOCATED: u32 = 4;

const MIM_OPEN: u32 = 0x3C1;
const MIM_CLOSE: u32 = 0x3C2;
const MIM_DATA: u32 = 0x3C3;
const MIM_LONGDATA: u32 = 0x3C4;
const MIM_LONGERROR: u32 = 0x3C6;

/// Un périphérique d'entrée trouvé.
struct Device {
    path: PathBuf,
    name: String,
}

fn fallback_name(index: usize) -> String {
    let base = match crate::intl::locale::current().lang.iso639 {
        "fr" => "Entrée MIDI",
        "de" => "MIDI-Eingang",
        "es" => "Entrada MIDI",
        _ => "MIDI input",
    };
    format!("{base} {}", index + 1)
}

/// « midiC1D0 » → (1, 0).
fn parse_name(name: &str) -> Option<(u32, u32)> {
    let rest = name.strip_prefix("midiC")?;
    let (c, d) = rest.split_once('D')?;
    Some((c.parse().ok()?, d.parse().ok()?))
}

fn devices() -> Vec<Device> {
    let custom = std::env::var_os("WINLUX_MIDI_IN_DIR").map(PathBuf::from);
    let dir = custom.clone().unwrap_or_else(|| PathBuf::from("/dev/snd"));
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<(u32, u32, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let (c, d) = parse_name(e.file_name().to_str()?)?;
            Some((c, d, e.path()))
        })
        .collect();
    found.sort();
    found
        .into_iter()
        .enumerate()
        .map(|(i, (c, d, path))| {
            let proc_name = if custom.is_none() {
                std::fs::read_to_string(format!("/proc/asound/card{c}/midi{d}"))
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
                    .filter(|s| !s.is_empty())
            } else {
                None
            };
            Device {
                path,
                name: proc_name.unwrap_or_else(|| fallback_name(i)),
            }
        })
        .collect()
}

pub extern "win64" fn midi_in_get_num_devs() -> u32 {
    devices().len() as u32
}

/// MIDIINCAPS(A/W) : wMid, wPid, vDriverVersion, szPname[32], dwSupport.
unsafe fn fill_caps(name: &str, caps: *mut u8, size: u32, wide: bool) -> u32 {
    if caps.is_null() || size == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let need = if wide { 76 } else { 44 };
    let mut buf = vec![0u8; need];
    buf[0..2].copy_from_slice(&0xFFu16.to_le_bytes()); // MM_UNMAPPED
    buf[2..4].copy_from_slice(&0xFFFFu16.to_le_bytes()); // MM_PID_UNMAPPED
    buf[4..8].copy_from_slice(&0x0100u32.to_le_bytes());
    if wide {
        for (i, c) in name.encode_utf16().take(31).enumerate() {
            buf[8 + 2 * i..10 + 2 * i].copy_from_slice(&c.to_le_bytes());
        }
    } else {
        for (i, c) in super::encode_ansi(name).into_iter().take(31).enumerate() {
            buf[8 + i] = c;
        }
    }
    let n = (size as usize).min(need);
    std::ptr::copy_nonoverlapping(buf.as_ptr(), caps, n);
    MMSYSERR_NOERROR
}

/// Nom du périphérique `id`, qui peut aussi être un handle ouvert.
fn device_for(id: usize) -> Option<String> {
    if let Some(m) = get(id) {
        return Some(m.name.clone());
    }
    devices().into_iter().nth(id).map(|d| d.name)
}

pub unsafe extern "win64" fn midi_in_get_dev_caps_a(id: usize, caps: *mut u8, size: u32) -> u32 {
    match device_for(id) {
        Some(name) => fill_caps(&name, caps, size, false),
        None => MMSYSERR_BADDEVICEID,
    }
}

pub unsafe extern "win64" fn midi_in_get_dev_caps_w(id: usize, caps: *mut u8, size: u32) -> u32 {
    match device_for(id) {
        Some(name) => fill_caps(&name, caps, size, true),
        None => MMSYSERR_BADDEVICEID,
    }
}

/// Ce que le découpeur tire d'un octet reçu.
#[derive(Debug, PartialEq)]
enum Event {
    Short(u32),
    Sysex(u8),
    /// Fin du système exclusif : `true` à F7, `false` s'il est coupé.
    SysexEnd(bool),
}

/// Découpe le flux d'octets MIDI en messages (statut courant compris).
#[derive(Default)]
struct Parser {
    status: u8,
    data: [u8; 2],
    have: usize,
    sysex: bool,
}

fn data_len(status: u8) -> usize {
    match status {
        0x80..=0xBF | 0xE0..=0xEF | 0xF2 => 2,
        0xC0..=0xDF | 0xF1 | 0xF3 => 1,
        _ => 0,
    }
}

impl Parser {
    fn feed(&mut self, b: u8, out: &mut Vec<Event>) {
        if b >= 0xF8 {
            // Temps réel : passe tout de suite, même au milieu d'un message.
            out.push(Event::Short(b as u32));
            return;
        }
        if self.sysex {
            if b < 0x80 {
                out.push(Event::Sysex(b));
                return;
            }
            self.sysex = false;
            if b == 0xF7 {
                out.push(Event::Sysex(b));
                out.push(Event::SysexEnd(true));
                return;
            }
            out.push(Event::SysexEnd(false));
        }
        if b == 0xF0 {
            self.sysex = true;
            self.status = 0;
            out.push(Event::Sysex(b));
            return;
        }
        if b >= 0x80 {
            self.have = 0;
            if b >= 0xF0 {
                // Messages système communs : pas de statut courant.
                self.status = 0;
                if data_len(b) == 0 {
                    if b != 0xF7 {
                        out.push(Event::Short(b as u32));
                    }
                } else {
                    self.status = b;
                }
            } else {
                self.status = b;
            }
            return;
        }
        if self.status == 0 {
            return; // octet de données orphelin
        }
        self.data[self.have] = b;
        self.have += 1;
        if self.have == data_len(self.status) {
            let msg = self.status as u32
                | (self.data[0] as u32) << 8
                | if self.have == 2 {
                    (self.data[1] as u32) << 16
                } else {
                    0
                };
            out.push(Event::Short(msg));
            self.have = 0;
            if self.status >= 0xF0 {
                self.status = 0;
            }
        }
    }
}

struct InState {
    started: bool,
    start: Instant,
    queue: VecDeque<usize>,
    parser: Parser,
}

struct MidiIn {
    handle: usize,
    name: String,
    callback: Callback,
    fd: i32,
    st: Mutex<InState>,
    stop: AtomicBool,
    reader: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// Message prêt à partir, envoyé hors verrou.
type Note = (u32, usize, usize);

impl MidiIn {
    fn send(&self, notes: Vec<Note>) {
        for (msg, p1, p2) in notes {
            out::send(self.callback, self.handle, msg, p1, p2);
        }
    }

    fn elapsed(st: &InState) -> usize {
        st.start.elapsed().as_millis() as usize
    }

    /// Rend le tampon de tête (s'il y en a un) avec `msg`.
    fn finish_head(st: &mut InState, msg: u32, notes: &mut Vec<Note>) {
        if let Some(h) = st.queue.pop_front() {
            unsafe {
                let f = out::hdr_flags(h);
                *f = (*f & !MHDR_INQUEUE) | MHDR_DONE;
            }
            notes.push((msg, h, Self::elapsed(st)));
        }
    }

    fn receive(&self, bytes: &[u8]) {
        let mut notes = Vec::new();
        {
            let mut st = lock(&self.st);
            if !st.started {
                return;
            }
            let mut events = Vec::new();
            for &b in bytes {
                st.parser.feed(b, &mut events);
            }
            for e in events {
                match e {
                    Event::Short(m) => notes.push((MIM_DATA, m as usize, Self::elapsed(&st))),
                    Event::Sysex(b) => {
                        let Some(&h) = st.queue.front() else {
                            continue; // pas de tampon : l'octet est perdu
                        };
                        unsafe {
                            let n = out::hdr_recorded(h);
                            *(out::hdr_data(h) as *mut u8).add(n) = b;
                            *recorded(h) = n as u32 + 1;
                            if n + 1 >= out::hdr_len(h) {
                                Self::finish_head(&mut st, MIM_LONGDATA, &mut notes);
                            }
                        }
                    }
                    Event::SysexEnd(complete) => {
                        let started = st
                            .queue
                            .front()
                            .is_some_and(|&h| unsafe { out::hdr_recorded(h) } > 0);
                        if started {
                            let msg = if complete {
                                MIM_LONGDATA
                            } else {
                                MIM_LONGERROR
                            };
                            Self::finish_head(&mut st, msg, &mut notes);
                        }
                    }
                }
            }
        }
        self.send(notes);
    }
}

unsafe fn recorded(h: usize) -> *mut u32 {
    (h + if out::x86() { 8 } else { 12 }) as *mut u32
}

fn reader(m: Arc<MidiIn>) {
    let mut buf = [0u8; 256];
    while !m.stop.load(Ordering::Relaxed) {
        if !lock(&m.st).started {
            std::thread::sleep(std::time::Duration::from_millis(10));
            continue;
        }
        let mut pfd = libc::pollfd {
            fd: m.fd,
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut pfd, 1, 50) } <= 0 {
            continue;
        }
        let n = unsafe { libc::read(m.fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n > 0 {
            m.receive(&buf[..n as usize]);
        } else {
            // Fin d'un fichier de test, ou clavier débranché : on attend.
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

fn table() -> &'static Mutex<HashMap<usize, Arc<MidiIn>>> {
    static T: OnceLock<Mutex<HashMap<usize, Arc<MidiIn>>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get(h: usize) -> Option<Arc<MidiIn>> {
    lock(table()).get(&h).cloned()
}

/// midiInOpen(phmi, uDeviceID, dwCallback, dwInstance, fdwOpen).
pub unsafe extern "win64" fn midi_in_open(
    phmi: *mut usize,
    device: u32,
    callback: usize,
    instance: usize,
    flags: u32,
) -> u32 {
    if phmi.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let Some(dev) = devices().into_iter().nth(device as usize) else {
        return MMSYSERR_BADDEVICEID;
    };
    let callback = match out::callback(callback, instance, flags) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let Ok(path) = std::ffi::CString::new(dev.path.as_os_str().as_encoded_bytes()) else {
        return MMSYSERR_BADDEVICEID;
    };
    let fd = libc::open(
        path.as_ptr(),
        libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
    );
    if fd < 0 {
        // Déjà pris par un autre programme, ou pas le droit de le lire.
        return MMSYSERR_ALLOCATED;
    }
    static NEXT: AtomicUsize = AtomicUsize::new(0x7742_0000);
    let handle = NEXT.fetch_add(4, Ordering::Relaxed);
    let m = Arc::new(MidiIn {
        handle,
        name: dev.name,
        callback,
        fd,
        st: Mutex::new(InState {
            started: false,
            start: Instant::now(),
            queue: VecDeque::new(),
            parser: Parser::default(),
        }),
        stop: AtomicBool::new(false),
        reader: Mutex::new(None),
    });
    let r = m.clone();
    *lock(&m.reader) = std::thread::Builder::new()
        .name("winlux-midi-in".into())
        .spawn(move || reader(r))
        .ok();
    lock(table()).insert(handle, m.clone());
    *phmi = handle;
    m.send(vec![(MIM_OPEN, 0, 0)]);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_in_close(h: usize) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if !lock(&m.st).queue.is_empty() {
        return MIDIERR_STILLPLAYING;
    }
    lock(table()).remove(&h);
    m.stop.store(true, Ordering::Relaxed);
    let reader = lock(&m.reader).take();
    if let Some(t) = reader {
        // Fermé depuis son propre rappel : le fil s'arrête tout seul.
        if t.thread().id() != std::thread::current().id() {
            let _ = t.join();
        }
    }
    libc::close(m.fd);
    m.send(vec![(MIM_CLOSE, 0, 0)]);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_in_prepare_header(h: usize, hdr: usize, size: u32) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    if hdr == 0 || size < out::min_hdr() || out::hdr_data(hdr).is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let f = out::hdr_flags(hdr);
    if *f & MHDR_INQUEUE != 0 {
        return MIDIERR_STILLPLAYING;
    }
    *f = (*f & !MHDR_DONE) | MHDR_PREPARED;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_in_unprepare_header(h: usize, hdr: usize, size: u32) -> u32 {
    if get(h).is_none() {
        return MMSYSERR_INVALHANDLE;
    }
    if hdr == 0 || size < out::min_hdr() {
        return MMSYSERR_INVALPARAM;
    }
    let f = out::hdr_flags(hdr);
    if *f & MHDR_INQUEUE != 0 {
        return MIDIERR_STILLPLAYING;
    }
    *f &= !MHDR_PREPARED;
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_in_add_buffer(h: usize, hdr: usize, size: u32) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if hdr == 0 || size < out::min_hdr() || out::hdr_len(hdr) == 0 {
        return MMSYSERR_INVALPARAM;
    }
    let f = out::hdr_flags(hdr);
    if *f & MHDR_PREPARED == 0 {
        return MIDIERR_UNPREPARED;
    }
    if *f & MHDR_INQUEUE != 0 {
        return MIDIERR_STILLPLAYING;
    }
    *recorded(hdr) = 0;
    *f = (*f & !MHDR_DONE) | MHDR_INQUEUE;
    lock(&m.st).queue.push_back(hdr);
    MMSYSERR_NOERROR
}

pub extern "win64" fn midi_in_start(h: usize) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    let mut st = lock(&m.st);
    if !st.started {
        // Ce qu'un clavier a envoyé avant midiInStart n'est pas livré.
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(m.fd, &mut stat) } == 0
            && stat.st_mode & libc::S_IFMT == libc::S_IFCHR
        {
            let mut buf = [0u8; 256];
            while unsafe { libc::read(m.fd, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
        }
        st.started = true;
        st.start = Instant::now();
        st.parser = Parser::default();
    }
    MMSYSERR_NOERROR
}

/// midiInStop : rend le tampon entamé, garde les autres en file.
pub extern "win64" fn midi_in_stop(h: usize) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    let mut notes = Vec::new();
    {
        let mut st = lock(&m.st);
        st.started = false;
        let partial = st
            .queue
            .front()
            .is_some_and(|&h| unsafe { out::hdr_recorded(h) } > 0);
        if partial {
            MidiIn::finish_head(&mut st, MIM_LONGDATA, &mut notes);
        }
    }
    m.send(notes);
    MMSYSERR_NOERROR
}

/// midiInReset : arrête et rend tous les tampons.
pub extern "win64" fn midi_in_reset(h: usize) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    let mut notes = Vec::new();
    {
        let mut st = lock(&m.st);
        st.started = false;
        while !st.queue.is_empty() {
            MidiIn::finish_head(&mut st, MIM_LONGDATA, &mut notes);
        }
    }
    m.send(notes);
    MMSYSERR_NOERROR
}

pub unsafe extern "win64" fn midi_in_get_id(h: usize, out: *mut u32) -> u32 {
    let Some(m) = get(h) else {
        return MMSYSERR_INVALHANDLE;
    };
    if out.is_null() {
        return MMSYSERR_INVALPARAM;
    }
    let id = devices().iter().position(|d| d.name == m.name).unwrap_or(0);
    *out = id as u32;
    MMSYSERR_NOERROR
}

pub extern "win64" fn midi_in_message(_h: usize, _msg: u32, _p1: usize, _p2: usize) -> u32 {
    8 // MMSYSERR_NOTSUPPORTED
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "midiInGetNumDevs" => fn midi_in_get_num_devs,
        "midiInGetDevCapsA" => fn midi_in_get_dev_caps_a,
        "midiInGetDevCapsW" => fn midi_in_get_dev_caps_w,
        "midiInOpen" => fn midi_in_open,
        "midiInClose" => fn midi_in_close,
        "midiInPrepareHeader" => fn midi_in_prepare_header,
        "midiInUnprepareHeader" => fn midi_in_unprepare_header,
        "midiInAddBuffer" => fn midi_in_add_buffer,
        "midiInStart" => fn midi_in_start,
        "midiInStop" => fn midi_in_stop,
        "midiInReset" => fn midi_in_reset,
        "midiInGetID" => fn midi_in_get_id,
        "midiInMessage" => fn midi_in_message,
        "midiInGetErrorTextA" => fn super::winmm_midi::midi_out_get_error_text_a,
        "midiInGetErrorTextW" => fn super::winmm_midi::midi_out_get_error_text_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Vec<Event> {
        let mut p = Parser::default();
        let mut out = Vec::new();
        for &b in bytes {
            p.feed(b, &mut out);
        }
        out
    }

    #[test]
    fn statut_courant_et_temps_reel() {
        assert_eq!(
            parse(&[0x90, 0x3C, 0xF8, 0x7F, 0x3C, 0x00, 0xC0, 0x05, 0xF8]),
            vec![
                Event::Short(0xF8),
                Event::Short(0x7F3C90),
                Event::Short(0x003C90),
                Event::Short(0x05C0),
                Event::Short(0xF8),
            ]
        );
    }

    #[test]
    fn systeme_exclusif_complet_ou_coupe() {
        assert_eq!(
            parse(&[0xF0, 0x7E, 0xF7]),
            vec![
                Event::Sysex(0xF0),
                Event::Sysex(0x7E),
                Event::Sysex(0xF7),
                Event::SysexEnd(true)
            ]
        );
        assert_eq!(
            parse(&[0xF0, 0x01, 0x90, 0x3C, 0x40]),
            vec![
                Event::Sysex(0xF0),
                Event::Sysex(0x01),
                Event::SysexEnd(false),
                Event::Short(0x403C90)
            ]
        );
        // Un octet de données sans statut est ignoré.
        assert_eq!(parse(&[0x12, 0xF3, 0x02]), vec![Event::Short(0x02F3)]);
    }

    #[test]
    fn noms_des_peripheriques() {
        assert_eq!(parse_name("midiC1D0"), Some((1, 0)));
        assert_eq!(parse_name("midiC12D3"), Some((12, 3)));
        assert_eq!(parse_name("pcmC0D0p"), None);
        assert_eq!(parse_name("midiC1"), None);
    }
}
