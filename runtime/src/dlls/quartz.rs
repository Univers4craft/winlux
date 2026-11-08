//! quartz.dll : DirectShow (#835, suite de #237).
//!
//! Le graphe de filtres (CLSID_FilterGraph) lit les vidéos d'introduction et
//! les cinématiques des jeux : IGraphBuilder::RenderFile, IMediaControl,
//! IMediaEvent(Ex), IVideoWindow, IBasicVideo, IBasicAudio, IMediaSeeking,
//! IMediaPosition et IMediaFilter. Le décodage est celui du moteur
//! multimédia de Winlux (`winlux_media`, FFmpeg du système dans un processus
//! séparé, qui joue aussi le son) ; les images sont dessinées dans la
//! fenêtre vidéo (fenêtre propriétaire du jeu, rectangle demandé, ou plein
//! écran avec bandes noires) par un fil dédié. Les filtres ajoutés par le
//! programme (rendus personnalisés, VMR) ne sont pas branchés.
//!
//! Comme DirectDraw et Direct3D 9, les tables de méthodes sont celles de
//! l'ABI du programme : un jeu 32 bits appelle directement ces objets.

#![allow(clippy::too_many_arguments)]

use super::comobj::{guid, Guid};
use super::legacy_com::{put_ptr, vtable_sig};
use super::winlux_media as media;
use crate::dlls::lock;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_ABORT: i32 = 0x8000_4004u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const HR_FILE_NOT_FOUND: i32 = 0x8007_0002u32 as i32;
const VFW_E_CANNOT_RENDER: i32 = 0x8004_0218u32 as i32;
const VFW_E_NOT_CONNECTED: i32 = 0x8004_0209u32 as i32;
const VFW_E_NO_VIDEO: i32 = 0x8004_025Bu32 as i32; // pas d'IBasicVideo utile sans image

const EC_COMPLETE: u32 = 0x01;
const EC_ERRORABORT: u32 = 0x03;

/// 100 ns par milliseconde.
const UNITS_PER_MS: i64 = 10_000;

const fn ds(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    guid(d1, d2, d3, d4)
}
const B03A: [u8; 8] = [0xb0, 0x3a, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70];
const C2C8: [u8; 8] = [0x8b, 0x46, 0x00, 0x80, 0x5f, 0x6c, 0xef, 0x60];
const CLSID_FILTER_GRAPH: Guid = ds(
    0xe436ebb3,
    0x524f,
    0x11ce,
    [0x9f, 0x53, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70],
);
const CLSID_FILTER_GRAPH_NO_THREAD: Guid = ds(
    0xe436ebb8,
    0x524f,
    0x11ce,
    [0x9f, 0x53, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70],
);
const IID_IUNKNOWN: Guid = ds(0, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IDISPATCH: Guid = ds(0x0002_0400, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IPERSIST: Guid = ds(0x0000_010c, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_IFILTERGRAPH: Guid = ds(0x56a8689f, 0x0ad4, 0x11ce, B03A);
const IID_IGRAPHBUILDER: Guid = ds(0x56a868a9, 0x0ad4, 0x11ce, B03A);
const IID_IFILTERGRAPH2: Guid = ds(0x36b73882, 0xc2c8, 0x11cf, C2C8);
const IID_IMEDIACONTROL: Guid = ds(0x56a868b1, 0x0ad4, 0x11ce, B03A);
const IID_IMEDIAEVENT: Guid = ds(0x56a868b6, 0x0ad4, 0x11ce, B03A);
const IID_IMEDIAEVENTEX: Guid = ds(0x56a868c0, 0x0ad4, 0x11ce, B03A);
const IID_IVIDEOWINDOW: Guid = ds(0x56a868b4, 0x0ad4, 0x11ce, B03A);
const IID_IBASICAUDIO: Guid = ds(0x56a868b3, 0x0ad4, 0x11ce, B03A);
const IID_IBASICVIDEO: Guid = ds(0x56a868b5, 0x0ad4, 0x11ce, B03A);
const IID_IBASICVIDEO2: Guid = ds(
    0x329bb360,
    0xf6ea,
    0x11d1,
    [0x90, 0x38, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x98],
);
const IID_IMEDIASEEKING: Guid = ds(0x36b73880, 0xc2c8, 0x11cf, C2C8);
const IID_IMEDIAPOSITION: Guid = ds(0x56a868b2, 0x0ad4, 0x11ce, B03A);
const IID_IMEDIAFILTER: Guid = ds(0x56a86899, 0x0ad4, 0x11ce, B03A);
const TIME_FORMAT_MEDIA_TIME: Guid = ds(
    0x7b785574,
    0x8c82,
    0x11cf,
    [0xbc, 0x0c, 0x00, 0xaa, 0x00, 0xac, 0x74, 0xf6],
);

macro_rules! api {
    ($dll:literal, $name:literal, fn($($t:ty),*) -> $r:ty $(, $arg:expr)* $(,)?) => {{
        match crate::dlls::lookup($dll, $name) {
            Some(addr) => {
                let f: unsafe extern "win64" fn($($t),*) -> $r = std::mem::transmute(addr);
                Some(f($($arg),*))
            }
            None => None,
        }
    }};
}

// ═══════════════════════════════════════════════════════════════════════
// État du graphe
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Run {
    Stopped = 0,
    Paused = 1,
    Running = 2,
}

struct State {
    file: Option<PathBuf>,
    duration_ms: u64,
    video: Option<(u32, u32)>,
    audio: bool,
    run: Run,
    /// Session du moteur (0 : aucune) et position de départ de la session.
    session: usize,
    base_ms: u64,
    /// Position quand rien ne joue (pause, arrêt, avant Run).
    pos_ms: u64,
    stop_ms: Option<u64>,
    rate: f64,
    ended: bool,
    events: VecDeque<(u32, isize, isize)>,
    notify: Option<(usize, u32, isize)>,
    notify_flags: u32,
    // Fenêtre vidéo.
    owner: usize,
    visible: bool,
    fullscreen: bool,
    rect: Option<[i32; 4]>,
    dest: Option<[i32; 4]>,
    style: u32,
    style_ex: u32,
    drain: usize,
    // Son : centièmes de dB (-10000..0) et balance.
    volume: i32,
    balance: i32,
}

struct Shared {
    st: Mutex<State>,
    event: usize,
    stop_render: AtomicBool,
    renderer: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// Interface détachée : sa table et l'objet graphe.
#[repr(C)]
struct Face {
    vtbl: usize,
    graph: *const Graph,
}

#[repr(C)]
struct Graph {
    /// IGraphBuilder (identité COM).
    vtbl: usize,
    refs: AtomicU32,
    sh: Arc<Shared>,
    faces: [Face; 8],
}

const F_CONTROL: usize = 0;
const F_EVENT: usize = 1;
const F_VIDEO: usize = 2;
const F_AUDIO: usize = 3;
const F_BASICVIDEO: usize = 4;
const F_SEEKING: usize = 5;
const F_POSITION: usize = 6;
const F_FILTER: usize = 7;

unsafe impl Send for Graph {}
unsafe impl Sync for Graph {}

/// Graphe d'un pointeur d'interface (le graphe lui-même ou une interface).
unsafe fn graph<'a>(this: usize) -> &'a Graph {
    let first = this as *const usize;
    // Une Face commence par sa table puis le pointeur du graphe ; le graphe
    // commence par sa table puis son compteur (< 2^32).
    for g in GRAPHS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        if *g == this {
            return &*(this as *const Graph);
        }
    }
    let _ = first;
    &*((*(this as *const Face)).graph)
}
static GRAPHS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

fn sh(this: usize) -> Arc<Shared> {
    unsafe { graph(this).sh.clone() }
}

impl Shared {
    fn push_event(&self, code: u32, p1: isize, p2: isize) {
        let notify = {
            let mut st = lock(&self.st);
            st.events.push_back((code, p1, p2));
            if st.notify_flags & 1 == 0 {
                st.notify
            } else {
                None
            }
        };
        unsafe {
            api!("kernel32", "SetEvent", fn(usize) -> i32, self.event);
            if let Some((hwnd, msg, lp)) = notify {
                api!(
                    "user32",
                    "PostMessageW",
                    fn(usize, u32, usize, isize) -> i32,
                    hwnd,
                    msg,
                    0,
                    lp
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Lecture : sessions du moteur et fil de rendu
// ═══════════════════════════════════════════════════════════════════════

fn path_w(p: &std::path::Path) -> Vec<u16> {
    crate::paths::to_windows(p)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

/// Volume DirectShow (centièmes de dB) → pourcentage du moteur.
fn volume_pct(v: i32) -> u32 {
    if v <= -10000 {
        return 0;
    }
    (100.0 * 10f64.powf(v.min(0) as f64 / 2000.0)).round() as u32
}

/// Position courante (ms).
fn position(st: &State) -> u64 {
    if st.session == 0 {
        return st.pos_ms;
    }
    let mut ps = media::PlaybackState::default();
    unsafe { media::winlux_media_state(st.session, &mut ps) };
    (st.base_ms + ps.position_ms as u64).min(st.duration_ms.max(1))
}

fn close_session(st: &mut State) {
    if st.session != 0 {
        st.pos_ms = position(st);
        unsafe { media::winlux_media_close(st.session) };
        st.session = 0;
    }
}

fn open_session(st: &mut State) -> bool {
    let Some(file) = st.file.clone() else {
        return false;
    };
    let w = path_w(&file);
    let opts = media::OpenOptions {
        start_ms: st.pos_ms.min(u32::MAX as u64) as u32,
        width: 0,
        height: 0,
        volume: volume_pct(st.volume),
        speed: (st.rate * 100.0).clamp(25.0, 400.0) as u32,
        audio_track: if st.audio { -1 } else { -2 },
        sub_track: -1,
        flags: if st.video.is_none() { 1 } else { 0 },
        sub_path: std::ptr::null(),
    };
    let h = unsafe { media::winlux_media_open(w.as_ptr(), &opts) };
    if h == 0 {
        return false;
    }
    st.session = h;
    st.base_ms = st.pos_ms;
    true
}

/// Rectangle client d'une fenêtre (largeur, hauteur).
unsafe fn client_size(hwnd: usize) -> (i32, i32) {
    let mut r = [0i32; 4];
    api!(
        "user32",
        "GetClientRect",
        fn(usize, *mut i32) -> i32,
        hwnd,
        r.as_mut_ptr()
    );
    (r[2] - r[0], r[3] - r[1])
}

/// Où dessiner l'image : fenêtre et rectangle.
unsafe fn target(st: &State, vw: u32, vh: u32) -> Option<(usize, [i32; 4])> {
    let hwnd = if st.owner != 0 && !st.fullscreen {
        st.owner
    } else {
        let a = api!("user32", "GetActiveWindow", fn() -> usize).unwrap_or(0);
        if a != 0 {
            a
        } else {
            api!("user32", "GetForegroundWindow", fn() -> usize).unwrap_or(0)
        }
    };
    if hwnd == 0 {
        return None;
    }
    let (cw, ch) = client_size(hwnd);
    if cw <= 0 || ch <= 0 {
        return None;
    }
    let explicit = if st.fullscreen { None } else { st.rect };
    let mut r = match explicit {
        Some([x, y, w, h]) => [x, y, w, h],
        None => {
            // Bandes noires : l'image garde ses proportions.
            let s = (cw as f64 / vw as f64).min(ch as f64 / vh as f64);
            let (w, h) = ((vw as f64 * s) as i32, (vh as f64 * s) as i32);
            [(cw - w) / 2, (ch - h) / 2, w, h]
        }
    };
    if let Some([dx, dy, dw, dh]) = st.dest {
        r = [r[0] + dx, r[1] + dy, dw, dh];
    }
    Some((hwnd, r))
}

unsafe fn draw(hwnd: usize, r: [i32; 4], frame: &[u8], w: u32, h: u32, clear: bool) {
    let hdc = api!("user32", "GetDC", fn(usize) -> usize, hwnd).unwrap_or(0);
    if hdc == 0 {
        return;
    }
    if clear {
        // Bandes noires une fois, au début.
        let (cw, ch) = client_size(hwnd);
        let full = [0i32, 0, cw, ch];
        let brush = api!("gdi32", "GetStockObject", fn(i32) -> usize, 4).unwrap_or(0);
        api!(
            "user32",
            "FillRect",
            fn(usize, *const i32, usize) -> i32,
            hdc,
            full.as_ptr(),
            brush
        );
    }
    let mut bmi = [0u8; 44];
    bmi[0..4].copy_from_slice(&40u32.to_le_bytes());
    bmi[4..8].copy_from_slice(&(w as i32).to_le_bytes());
    bmi[8..12].copy_from_slice(&(-(h as i32)).to_le_bytes());
    bmi[12..14].copy_from_slice(&1u16.to_le_bytes());
    bmi[14..16].copy_from_slice(&32u16.to_le_bytes());
    api!(
        "gdi32",
        "StretchDIBits",
        fn(usize, i32, i32, i32, i32, i32, i32, i32, i32, *const u8, *const u8, u32, u32) -> i32,
        hdc,
        r[0],
        r[1],
        r[2],
        r[3],
        0,
        0,
        w as i32,
        h as i32,
        frame.as_ptr(),
        bmi.as_ptr(),
        0,
        0x00CC_0020
    );
    api!("user32", "ReleaseDC", fn(usize, usize) -> i32, hwnd, hdc);
}

/// Fil de rendu : images vers la fenêtre, fin de lecture vers les
/// événements.
fn render_loop(sh: Arc<Shared>) {
    let mut last_seq = 0u32;
    let mut cleared = false;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        std::thread::sleep(Duration::from_millis(8));
        if sh.stop_render.load(Ordering::Acquire) {
            return;
        }
        let (session, video, visible, stop_ms) = {
            let st = lock(&sh.st);
            (st.session, st.video, st.visible, st.stop_ms)
        };
        if session == 0 {
            continue;
        }
        let mut ps = media::PlaybackState::default();
        if unsafe { media::winlux_media_state(session, &mut ps) } == 0 {
            continue;
        }
        let pos = {
            let st = lock(&sh.st);
            st.base_ms + ps.position_ms as u64
        };
        let stop_reached = stop_ms.is_some_and(|s| pos >= s);
        if ps.status >= 2 || stop_reached {
            let ok = ps.status != 3 || stop_reached;
            {
                let mut st = lock(&sh.st);
                if st.ended || st.session != session {
                    continue;
                }
                st.ended = true;
                st.pos_ms = if ok { st.duration_ms } else { pos };
                unsafe { media::winlux_media_close(st.session) };
                st.session = 0;
            }
            if ok {
                sh.push_event(EC_COMPLETE, 0, 0);
            } else {
                crate::log_warn!("quartz", "lecture interrompue par le décodeur");
                sh.push_event(EC_ERRORABORT, VFW_E_CANNOT_RENDER as isize, 0);
            }
            continue;
        }
        if video.is_none() || !visible || ps.frame_seq == last_seq || ps.width == 0 {
            continue;
        }
        let size = ps.width as usize * ps.height as usize * 4;
        buf.resize(size, 0);
        let seq = unsafe { media::winlux_media_frame(session, buf.as_mut_ptr(), size as u32) };
        if seq == 0 {
            continue;
        }
        last_seq = seq;
        let t = {
            let st = lock(&sh.st);
            unsafe { target(&st, ps.width, ps.height) }
        };
        if let Some((hwnd, r)) = t {
            unsafe { draw(hwnd, r, &buf, ps.width, ps.height, !cleared) };
            cleared = true;
        }
    }
}

fn ensure_renderer(sh: &Arc<Shared>) {
    let mut r = lock(&sh.renderer);
    if r.is_none() {
        let s = sh.clone();
        *r = std::thread::Builder::new()
            .name("quartz-rendu".into())
            .spawn(move || render_loop(s))
            .ok();
    }
}

fn run(sh: &Arc<Shared>) -> i32 {
    let mut st = lock(&sh.st);
    if st.file.is_none() {
        return VFW_E_NOT_CONNECTED;
    }
    if st.run == Run::Running && (st.session != 0 || st.ended) {
        return S_OK;
    }
    if st.ended || st.pos_ms >= st.duration_ms && st.duration_ms > 0 {
        // Déjà au bout : EC_COMPLETE à nouveau, comme DirectShow.
        st.run = Run::Running;
        st.ended = true;
        drop(st);
        sh.push_event(EC_COMPLETE, 0, 0);
        return S_OK;
    }
    if !open_session(&mut st) {
        return VFW_E_CANNOT_RENDER;
    }
    st.run = Run::Running;
    st.ended = false;
    drop(st);
    ensure_renderer(sh);
    S_OK
}
fn pause(sh: &Arc<Shared>) -> i32 {
    let mut st = lock(&sh.st);
    close_session(&mut st);
    st.run = Run::Paused;
    S_OK
}
fn stop(sh: &Arc<Shared>) -> i32 {
    let mut st = lock(&sh.st);
    close_session(&mut st);
    st.run = Run::Stopped;
    S_OK
}
/// Nouvelle position : la session reprend de là si la lecture tourne.
fn seek(sh: &Arc<Shared>, ms: u64) {
    let mut st = lock(&sh.st);
    let running = st.session != 0;
    close_session(&mut st);
    st.pos_ms = ms.min(st.duration_ms);
    st.ended = false;
    if running {
        open_session(&mut st);
    }
}

unsafe fn render_file(sh: &Arc<Shared>, path: *const u16) -> i32 {
    if path.is_null() {
        return E_POINTER;
    }
    let p = crate::paths::to_linux(&super::wstr(path));
    if !p.is_file() {
        return HR_FILE_NOT_FOUND;
    }
    let Some(info) = media::probe(&p) else {
        crate::log_warn!(
            "quartz",
            "vidéo non lisible par le moteur multimédia : {}",
            p.display()
        );
        return VFW_E_CANNOT_RENDER;
    };
    let video = info.main_video().map(|v| (v.width.max(1), v.height.max(1)));
    let audio = info.streams.iter().any(|s| s.kind == 'a');
    if video.is_none() && !audio {
        return VFW_E_CANNOT_RENDER;
    }
    let mut st = lock(&sh.st);
    close_session(&mut st);
    st.file = Some(p);
    st.duration_ms = info.duration_ms;
    st.video = video;
    st.audio = audio;
    st.pos_ms = 0;
    st.ended = false;
    S_OK
}

// ═══════════════════════════════════════════════════════════════════════
// IUnknown commun
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn qi(this: usize, iid: *const Guid, out: usize) -> i32 {
    if out == 0 {
        return E_POINTER;
    }
    put_ptr(out, 0);
    if iid.is_null() {
        return E_INVALIDARG;
    }
    let g = graph(this);
    let iid = *iid;
    let p = if [
        IID_IUNKNOWN,
        IID_IFILTERGRAPH,
        IID_IGRAPHBUILDER,
        IID_IFILTERGRAPH2,
    ]
    .contains(&iid)
    {
        g as *const Graph as usize
    } else {
        let f = match iid {
            x if x == IID_IMEDIACONTROL || x == IID_IDISPATCH => F_CONTROL,
            x if x == IID_IMEDIAEVENT || x == IID_IMEDIAEVENTEX => F_EVENT,
            x if x == IID_IVIDEOWINDOW => F_VIDEO,
            x if x == IID_IBASICAUDIO => F_AUDIO,
            x if x == IID_IBASICVIDEO || x == IID_IBASICVIDEO2 => F_BASICVIDEO,
            x if x == IID_IMEDIASEEKING => F_SEEKING,
            x if x == IID_IMEDIAPOSITION => F_POSITION,
            x if x == IID_IMEDIAFILTER || x == IID_IPERSIST => F_FILTER,
            _ => return E_NOINTERFACE,
        };
        &g.faces[f] as *const Face as usize
    };
    addref(this);
    put_ptr(out, p);
    S_OK
}
unsafe extern "win64" fn addref(this: usize) -> u32 {
    graph(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn release(this: usize) -> u32 {
    let g = graph(this);
    let n = g.refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let gp = g as *const Graph as usize;
        GRAPHS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|&x| x != gp);
        let sh = g.sh.clone();
        stop(&sh);
        sh.stop_render.store(true, Ordering::Release);
        if let Some(t) = lock(&sh.renderer).take() {
            let _ = t.join();
        }
        api!("kernel32", "CloseHandle", fn(usize) -> i32, sh.event);
        drop(Box::from_raw(gp as *mut Graph));
    }
    n
}

// IDispatch : pas d'automatisation.
unsafe extern "win64" fn disp_count(_this: usize, out: *mut u32) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    S_OK
}
unsafe extern "win64" fn notimpl3(_this: usize, _a: usize, _b: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    E_NOTIMPL
}
unsafe extern "win64" fn notimpl(_this: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn ok(_this: usize) -> i32 {
    S_OK
}

/// Les 3 méthodes d'IUnknown puis les 4 d'IDispatch.
fn dispatch_head() -> Vec<(usize, &'static str)> {
    vec![
        (qi as *const () as usize, "ip"),
        (addref as *const () as usize, ""),
        (release as *const () as usize, ""),
        (disp_count as *const () as usize, "i"),
        (notimpl3 as *const () as usize, "iip"),
        (notimpl as *const () as usize, "iiiii"),
        (notimpl as *const () as usize, "iiiiiiii"),
    ]
}
fn unknown_head() -> Vec<(usize, &'static str)> {
    dispatch_head()[..3].to_vec()
}
fn f(p: *const ()) -> usize {
    p as usize
}

// ═══════════════════════════════════════════════════════════════════════
// IGraphBuilder / IFilterGraph2
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn gb_add_filter(_this: usize, filter: usize, _name: usize) -> i32 {
    crate::log_warn!(
        "quartz",
        "AddFilter : filtre du programme 0x{filter:x} ignoré (rendu par Winlux)"
    );
    S_OK
}
unsafe extern "win64" fn gb_enum_filters(_this: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    E_NOTIMPL
}
unsafe extern "win64" fn gb_find(_this: usize, _name: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    0x8004_0216u32 as i32 // VFW_E_NOT_FOUND
}
unsafe extern "win64" fn gb_render_file(this: usize, file: *const u16, _playlist: usize) -> i32 {
    render_file(&sh(this), file)
}
unsafe extern "win64" fn gb_add_source(
    this: usize,
    file: *const u16,
    _name: usize,
    out: usize,
) -> i32 {
    put_ptr(out, 0);
    // Le fichier est retenu ; Render(pin) n'est pas branché.
    let hr = render_file(&sh(this), file);
    if hr < 0 {
        hr
    } else {
        E_NOTIMPL
    }
}
unsafe extern "win64" fn gb_ok1(_this: usize, _a: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn gb_notimpl1(_this: usize, _a: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn gb_notimpl2(_this: usize, _a: usize, _b: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn gb_notimpl3(_this: usize, _a: usize, _b: usize, _c: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn gb_add_source_moniker(
    _this: usize,
    _m: usize,
    _ctx: usize,
    _name: usize,
    out: usize,
) -> i32 {
    put_ptr(out, 0);
    E_NOTIMPL
}

fn graph_vtable() -> usize {
    let mut m = unknown_head();
    m.extend([
        (f(gb_add_filter as *const ()), "ii"),
        (f(gb_ok1 as *const ()), "i"), // RemoveFilter
        (f(gb_enum_filters as *const ()), "p"),
        (f(gb_find as *const ()), "ip"),
        (f(gb_notimpl3 as *const ()), "iii"), // ConnectDirect
        (f(gb_notimpl1 as *const ()), "i"),   // Reconnect
        (f(gb_notimpl1 as *const ()), "i"),   // Disconnect
        (f(ok as *const ()), ""),             // SetDefaultSyncSource
        (f(gb_notimpl2 as *const ()), "ii"),  // Connect
        (f(gb_notimpl1 as *const ()), "i"),   // Render
        (f(gb_render_file as *const ()), "ii"),
        (f(gb_add_source as *const ()), "iip"),
        (f(gb_ok1 as *const ()), "i"), // SetLogFile
        (f(ok as *const ()), ""),      // Abort
        (f(ok as *const ()), ""),      // ShouldOperationContinue
        (f(gb_add_source_moniker as *const ()), "iiip"),
        (f(gb_notimpl2 as *const ()), "ii"),  // ReconnectEx
        (f(gb_notimpl3 as *const ()), "iii"), // RenderEx
    ]);
    vtable_sig("IFilterGraph2", &m)
}

// ═══════════════════════════════════════════════════════════════════════
// IMediaControl
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn mc_run(this: usize) -> i32 {
    run(&sh(this))
}
unsafe extern "win64" fn mc_pause(this: usize) -> i32 {
    pause(&sh(this))
}
unsafe extern "win64" fn mc_stop(this: usize) -> i32 {
    stop(&sh(this))
}
unsafe extern "win64" fn mc_get_state(this: usize, _timeout: u32, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&sh(this).st).run as u32;
    S_OK
}
unsafe extern "win64" fn mc_render_file(this: usize, bstr: *const u16) -> i32 {
    render_file(&sh(this), bstr)
}
unsafe extern "win64" fn mc_add_source(_this: usize, _bstr: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    E_NOTIMPL
}
unsafe extern "win64" fn mc_collection(_this: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    E_NOTIMPL
}
unsafe extern "win64" fn mc_stop_when_ready(this: usize) -> i32 {
    let s = sh(this);
    let mut st = lock(&s.st);
    close_session(&mut st);
    st.run = Run::Stopped;
    S_OK
}

fn control_vtable() -> usize {
    let mut m = dispatch_head();
    m.extend([
        (f(mc_run as *const ()), ""),
        (f(mc_pause as *const ()), ""),
        (f(mc_stop as *const ()), ""),
        (f(mc_get_state as *const ()), "ii"),
        (f(mc_render_file as *const ()), "i"),
        (f(mc_add_source as *const ()), "ip"),
        (f(mc_collection as *const ()), "p"),
        (f(mc_collection as *const ()), "p"),
        (f(mc_stop_when_ready as *const ()), ""),
    ]);
    vtable_sig("IMediaControl", &m)
}

// ═══════════════════════════════════════════════════════════════════════
// IMediaEventEx
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn me_handle(this: usize, out: usize) -> i32 {
    if out == 0 {
        return E_POINTER;
    }
    put_ptr(out, sh(this).event);
    S_OK
}
unsafe extern "win64" fn me_get_event(
    this: usize,
    code: *mut u32,
    p1: *mut isize,
    p2: *mut isize,
    timeout: u32,
) -> i32 {
    if code.is_null() || p1.is_null() || p2.is_null() {
        return E_POINTER;
    }
    let s = sh(this);
    let start = Instant::now();
    loop {
        {
            let mut st = lock(&s.st);
            if let Some((c, a, b)) = st.events.pop_front() {
                if st.events.is_empty() {
                    api!("kernel32", "ResetEvent", fn(usize) -> i32, s.event);
                }
                *code = c;
                *p1 = a;
                *p2 = b;
                return S_OK;
            }
        }
        if timeout != u32::MAX && start.elapsed() >= Duration::from_millis(timeout as u64) {
            *code = 0;
            *p1 = 0;
            *p2 = 0;
            return E_ABORT;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
unsafe extern "win64" fn me_wait(this: usize, timeout: u32, code: *mut u32) -> i32 {
    let s = sh(this);
    let start = Instant::now();
    loop {
        {
            let st = lock(&s.st);
            if st.run != Run::Running {
                if !code.is_null() {
                    *code = 0;
                }
                return VFW_E_NOT_CONNECTED;
            }
            if st.ended {
                if !code.is_null() {
                    *code = EC_COMPLETE;
                }
                return S_OK;
            }
        }
        if timeout != u32::MAX && start.elapsed() >= Duration::from_millis(timeout as u64) {
            if !code.is_null() {
                *code = 0;
            }
            return E_ABORT;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
unsafe extern "win64" fn me_ok1(_this: usize, _a: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn me_free(_this: usize, _c: u32, _a: usize, _b: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn me_notify_window(this: usize, hwnd: usize, msg: u32, lp: isize) -> i32 {
    lock(&sh(this).st).notify = (hwnd != 0).then_some((hwnd, msg, lp));
    S_OK
}
unsafe extern "win64" fn me_set_flags(this: usize, flags: u32) -> i32 {
    lock(&sh(this).st).notify_flags = flags;
    S_OK
}
unsafe extern "win64" fn me_get_flags(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = lock(&sh(this).st).notify_flags;
    S_OK
}

fn event_vtable() -> usize {
    let mut m = dispatch_head();
    m.extend([
        (f(me_handle as *const ()), "p"),
        (f(me_get_event as *const ()), "ippi"),
        (f(me_wait as *const ()), "ii"),
        (f(me_ok1 as *const ()), "i"),
        (f(me_ok1 as *const ()), "i"),
        (f(me_free as *const ()), "iii"),
        (f(me_notify_window as *const ()), "iii"),
        (f(me_set_flags as *const ()), "i"),
        (f(me_get_flags as *const ()), "i"),
    ]);
    vtable_sig("IMediaEventEx", &m)
}

// ═══════════════════════════════════════════════════════════════════════
// IVideoWindow
// ═══════════════════════════════════════════════════════════════════════

unsafe fn with_st<R>(this: usize, f: impl FnOnce(&mut State) -> R) -> R {
    let s = sh(this);
    let mut st = lock(&s.st);
    f(&mut st)
}
unsafe fn out32(p: *mut i32, v: i32) -> i32 {
    if p.is_null() {
        return E_POINTER;
    }
    *p = v;
    S_OK
}
fn rect_field(st: &mut State, i: usize, v: i32) {
    let (vw, vh) = st.video.unwrap_or((320, 240));
    let mut r = st.rect.unwrap_or([0, 0, vw as i32, vh as i32]);
    r[i] = v;
    st.rect = Some(r);
}
fn rect_get(st: &State, i: usize) -> i32 {
    let (vw, vh) = st.video.unwrap_or((0, 0));
    st.rect.unwrap_or([0, 0, vw as i32, vh as i32])[i]
}
unsafe extern "win64" fn vw_caption(_this: usize, _s: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn vw_get_caption(_this: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    E_NOTIMPL
}
unsafe extern "win64" fn vw_put_style(this: usize, v: i32) -> i32 {
    with_st(this, |st| st.style = v as u32);
    S_OK
}
unsafe extern "win64" fn vw_get_style(this: usize, out: *mut i32) -> i32 {
    out32(out, with_st(this, |st| st.style as i32))
}
unsafe extern "win64" fn vw_put_style_ex(this: usize, v: i32) -> i32 {
    with_st(this, |st| st.style_ex = v as u32);
    S_OK
}
unsafe extern "win64" fn vw_get_style_ex(this: usize, out: *mut i32) -> i32 {
    out32(out, with_st(this, |st| st.style_ex as i32))
}
unsafe extern "win64" fn vw_put_ignored(_this: usize, _v: i32) -> i32 {
    S_OK
}
unsafe extern "win64" fn vw_get_true(_this: usize, out: *mut i32) -> i32 {
    out32(out, -1)
}
unsafe extern "win64" fn vw_get_zero(_this: usize, out: *mut i32) -> i32 {
    out32(out, 0)
}
unsafe extern "win64" fn vw_put_visible(this: usize, v: i32) -> i32 {
    with_st(this, |st| st.visible = v != 0);
    S_OK
}
unsafe extern "win64" fn vw_get_visible(this: usize, out: *mut i32) -> i32 {
    out32(
        out,
        if with_st(this, |st| st.visible) {
            -1
        } else {
            0
        },
    )
}
macro_rules! rect_accessors {
    ($put:ident, $get:ident, $i:expr) => {
        unsafe extern "win64" fn $put(this: usize, v: i32) -> i32 {
            with_st(this, |st| rect_field(st, $i, v));
            S_OK
        }
        unsafe extern "win64" fn $get(this: usize, out: *mut i32) -> i32 {
            out32(out, with_st(this, |st| rect_get(st, $i)))
        }
    };
}
rect_accessors!(vw_put_left, vw_get_left, 0);
rect_accessors!(vw_put_top, vw_get_top, 1);
rect_accessors!(vw_put_width, vw_get_width, 2);
rect_accessors!(vw_put_height, vw_get_height, 3);
unsafe extern "win64" fn vw_put_owner(this: usize, hwnd: usize) -> i32 {
    with_st(this, |st| st.owner = hwnd);
    S_OK
}
unsafe extern "win64" fn vw_get_owner(this: usize, out: usize) -> i32 {
    if out == 0 {
        return E_POINTER;
    }
    put_ptr(out, with_st(this, |st| st.owner));
    S_OK
}
unsafe extern "win64" fn vw_put_drain(this: usize, hwnd: usize) -> i32 {
    with_st(this, |st| st.drain = hwnd);
    S_OK
}
unsafe extern "win64" fn vw_get_drain(this: usize, out: usize) -> i32 {
    if out == 0 {
        return E_POINTER;
    }
    put_ptr(out, with_st(this, |st| st.drain));
    S_OK
}
unsafe extern "win64" fn vw_put_fullscreen(this: usize, v: i32) -> i32 {
    with_st(this, |st| st.fullscreen = v != 0);
    S_OK
}
unsafe extern "win64" fn vw_get_fullscreen(this: usize, out: *mut i32) -> i32 {
    out32(
        out,
        if with_st(this, |st| st.fullscreen) {
            -1
        } else {
            0
        },
    )
}
unsafe extern "win64" fn vw_notify_owner(
    _this: usize,
    _h: usize,
    _m: i32,
    _w: usize,
    _l: usize,
) -> i32 {
    S_OK
}
unsafe extern "win64" fn vw_set_pos(this: usize, x: i32, y: i32, w: i32, h: i32) -> i32 {
    with_st(this, |st| st.rect = Some([x, y, w, h]));
    S_OK
}
unsafe extern "win64" fn vw_get_pos(
    this: usize,
    x: *mut i32,
    y: *mut i32,
    w: *mut i32,
    h: *mut i32,
) -> i32 {
    let r = with_st(this, |st| {
        [
            rect_get(st, 0),
            rect_get(st, 1),
            rect_get(st, 2),
            rect_get(st, 3),
        ]
    });
    for (p, v) in [(x, r[0]), (y, r[1]), (w, r[2]), (h, r[3])] {
        if !p.is_null() {
            *p = v;
        }
    }
    S_OK
}
unsafe extern "win64" fn vw_ideal(this: usize, w: *mut i32, h: *mut i32) -> i32 {
    let (vw, vh) = with_st(this, |st| st.video.unwrap_or((0, 0)));
    out32(w, vw as i32);
    out32(h, vh as i32);
    S_OK
}

fn video_window_vtable() -> usize {
    let mut m = dispatch_head();
    m.extend([
        (f(vw_caption as *const ()), "i"),
        (f(vw_get_caption as *const ()), "p"),
        (f(vw_put_style as *const ()), "i"),
        (f(vw_get_style as *const ()), "i"),
        (f(vw_put_style_ex as *const ()), "i"),
        (f(vw_get_style_ex as *const ()), "i"),
        (f(vw_put_ignored as *const ()), "i"), // AutoShow
        (f(vw_get_true as *const ()), "i"),
        (f(vw_put_ignored as *const ()), "i"), // WindowState
        (f(vw_get_zero as *const ()), "i"),
        (f(vw_put_ignored as *const ()), "i"), // BackgroundPalette
        (f(vw_get_zero as *const ()), "i"),
        (f(vw_put_visible as *const ()), "i"),
        (f(vw_get_visible as *const ()), "i"),
        (f(vw_put_left as *const ()), "i"),
        (f(vw_get_left as *const ()), "i"),
        (f(vw_put_width as *const ()), "i"),
        (f(vw_get_width as *const ()), "i"),
        (f(vw_put_top as *const ()), "i"),
        (f(vw_get_top as *const ()), "i"),
        (f(vw_put_height as *const ()), "i"),
        (f(vw_get_height as *const ()), "i"),
        (f(vw_put_owner as *const ()), "i"),
        (f(vw_get_owner as *const ()), "p"),
        (f(vw_put_drain as *const ()), "i"),
        (f(vw_get_drain as *const ()), "p"),
        (f(vw_get_zero as *const ()), "i"),    // get_BorderColor
        (f(vw_put_ignored as *const ()), "i"), // put_BorderColor
        (f(vw_get_fullscreen as *const ()), "i"),
        (f(vw_put_fullscreen as *const ()), "i"),
        (f(vw_put_ignored as *const ()), "i"), // SetWindowForeground
        (f(vw_notify_owner as *const ()), "iiii"),
        (f(vw_set_pos as *const ()), "iiii"),
        (f(vw_get_pos as *const ()), "iiii"),
        (f(vw_ideal as *const ()), "ii"),     // GetMinIdealImageSize
        (f(vw_ideal as *const ()), "ii"),     // GetMaxIdealImageSize
        (f(vw_get_pos as *const ()), "iiii"), // GetRestorePosition
        (f(vw_put_ignored as *const ()), "i"), // HideCursor
        (f(vw_get_zero as *const ()), "i"),   // IsCursorHidden
    ]);
    vtable_sig("IVideoWindow", &m)
}

// ═══════════════════════════════════════════════════════════════════════
// IBasicAudio et IBasicVideo
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn ba_put_volume(this: usize, v: i32) -> i32 {
    if !(-10000..=0).contains(&v) {
        return E_INVALIDARG;
    }
    with_st(this, |st| st.volume = v);
    S_OK
}
unsafe extern "win64" fn ba_get_volume(this: usize, out: *mut i32) -> i32 {
    out32(out, with_st(this, |st| st.volume))
}
unsafe extern "win64" fn ba_put_balance(this: usize, v: i32) -> i32 {
    if !(-10000..=10000).contains(&v) {
        return E_INVALIDARG;
    }
    with_st(this, |st| st.balance = v);
    S_OK
}
unsafe extern "win64" fn ba_get_balance(this: usize, out: *mut i32) -> i32 {
    out32(out, with_st(this, |st| st.balance))
}

fn audio_vtable() -> usize {
    let mut m = dispatch_head();
    m.extend([
        (f(ba_put_volume as *const ()), "i"),
        (f(ba_get_volume as *const ()), "i"),
        (f(ba_put_balance as *const ()), "i"),
        (f(ba_get_balance as *const ()), "i"),
    ]);
    vtable_sig("IBasicAudio", &m)
}

unsafe fn video_size(this: usize) -> Option<(u32, u32)> {
    with_st(this, |st| st.video)
}
unsafe extern "win64" fn bv_avg_time(this: usize, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 1.0 / 30.0;
    if video_size(this).is_none() {
        return VFW_E_NO_VIDEO;
    }
    S_OK
}
unsafe extern "win64" fn bv_get_zero(_this: usize, out: *mut i32) -> i32 {
    out32(out, 0)
}
unsafe extern "win64" fn bv_width(this: usize, out: *mut i32) -> i32 {
    match video_size(this) {
        Some((w, _)) => out32(out, w as i32),
        None => VFW_E_NO_VIDEO,
    }
}
unsafe extern "win64" fn bv_height(this: usize, out: *mut i32) -> i32 {
    match video_size(this) {
        Some((_, h)) => out32(out, h as i32),
        None => VFW_E_NO_VIDEO,
    }
}
unsafe extern "win64" fn bv_put(_this: usize, _v: i32) -> i32 {
    S_OK
}
unsafe extern "win64" fn bv_src_pos(
    this: usize,
    x: *mut i32,
    y: *mut i32,
    w: *mut i32,
    h: *mut i32,
) -> i32 {
    let (vw, vh) = video_size(this).unwrap_or((0, 0));
    for (p, v) in [(x, 0), (y, 0), (w, vw as i32), (h, vh as i32)] {
        if !p.is_null() {
            *p = v;
        }
    }
    S_OK
}
unsafe extern "win64" fn bv_set4(_this: usize, _x: i32, _y: i32, _w: i32, _h: i32) -> i32 {
    S_OK
}
unsafe extern "win64" fn bv_set_dest(this: usize, x: i32, y: i32, w: i32, h: i32) -> i32 {
    with_st(this, |st| st.dest = Some([x, y, w, h]));
    S_OK
}
unsafe extern "win64" fn bv_get_dest(
    this: usize,
    x: *mut i32,
    y: *mut i32,
    w: *mut i32,
    h: *mut i32,
) -> i32 {
    let r = with_st(this, |st| {
        st.dest.unwrap_or([0, 0, rect_get(st, 2), rect_get(st, 3)])
    });
    for (p, v) in [(x, r[0]), (y, r[1]), (w, r[2]), (h, r[3])] {
        if !p.is_null() {
            *p = v;
        }
    }
    S_OK
}
unsafe extern "win64" fn bv_default_dest(this: usize) -> i32 {
    with_st(this, |st| st.dest = None);
    S_OK
}
unsafe extern "win64" fn bv_video_size(this: usize, w: *mut i32, h: *mut i32) -> i32 {
    match video_size(this) {
        Some((vw, vh)) => {
            out32(w, vw as i32);
            out32(h, vh as i32)
        }
        None => VFW_E_NO_VIDEO,
    }
}
unsafe extern "win64" fn bv_palette(_this: usize, _a: i32, _b: i32, _c: usize, _d: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn bv_current_image(_this: usize, _size: usize, _img: usize) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn bv_is_default(_this: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn bv_dest_field_get(this: usize, out: *mut i32) -> i32 {
    out32(out, with_st(this, |st| st.dest.map_or(0, |d| d[0])))
}

fn basic_video_vtable() -> usize {
    let mut m = dispatch_head();
    m.extend([
        (f(bv_avg_time as *const ()), "i"),
        (f(bv_get_zero as *const ()), "i"), // BitRate
        (f(bv_get_zero as *const ()), "i"), // BitErrorRate
        (f(bv_width as *const ()), "i"),
        (f(bv_height as *const ()), "i"),
    ]);
    // Source : Left, Width, Top, Height (put/get).
    for get in [
        bv_get_zero as *const (),
        bv_width as *const (),
        bv_get_zero as *const (),
        bv_height as *const (),
    ] {
        m.push((f(bv_put as *const ()), "i"));
        m.push((f(get), "i"));
    }
    // Destination : Left, Width, Top, Height (put/get).
    for _ in 0..4 {
        m.push((f(bv_put as *const ()), "i"));
        m.push((f(bv_dest_field_get as *const ()), "i"));
    }
    m.extend([
        (f(bv_set4 as *const ()), "iiii"), // SetSourcePosition
        (f(bv_src_pos as *const ()), "iiii"),
        (f(ok as *const ()), ""), // SetDefaultSourcePosition
        (f(bv_set_dest as *const ()), "iiii"),
        (f(bv_get_dest as *const ()), "iiii"),
        (f(bv_default_dest as *const ()), ""),
        (f(bv_video_size as *const ()), "ii"),
        (f(bv_palette as *const ()), "iiii"),
        (f(bv_current_image as *const ()), "ii"),
        (f(bv_is_default as *const ()), ""),
        (f(bv_is_default as *const ()), ""),
        // IBasicVideo2::GetPreferredAspectRatio
        (f(bv_video_size as *const ()), "ii"),
    ]);
    vtable_sig("IBasicVideo2", &m)
}

// ═══════════════════════════════════════════════════════════════════════
// IMediaSeeking et IMediaPosition
// ═══════════════════════════════════════════════════════════════════════

unsafe fn put64(p: *mut i64, v: i64) -> i32 {
    if p.is_null() {
        return E_POINTER;
    }
    p.write_unaligned(v);
    S_OK
}
unsafe extern "win64" fn ms_caps(_this: usize, out: *mut u32) -> i32 {
    // CanSeekAbsolute | Forwards | Backwards | GetCurrentPos | GetStopPos |
    // GetDuration | CanPlayBackwards non.
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0x1 | 0x2 | 0x4 | 0x8 | 0x10 | 0x20 | 0x100;
    S_OK
}
unsafe extern "win64" fn ms_check_caps(_this: usize, caps: *mut u32) -> i32 {
    if caps.is_null() {
        return E_POINTER;
    }
    let want = *caps;
    let have = 0x13F;
    *caps = want & have;
    if want & !have == 0 {
        S_OK
    } else if want & have != 0 {
        S_FALSE
    } else {
        E_NOTIMPL
    }
}
unsafe extern "win64" fn ms_format_ok(_this: usize, g: *const Guid) -> i32 {
    if !g.is_null() && *g == TIME_FORMAT_MEDIA_TIME {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn ms_get_format(_this: usize, g: *mut Guid) -> i32 {
    if g.is_null() {
        return E_POINTER;
    }
    *g = TIME_FORMAT_MEDIA_TIME;
    S_OK
}
unsafe extern "win64" fn ms_set_format(_this: usize, g: *const Guid) -> i32 {
    if !g.is_null() && *g == TIME_FORMAT_MEDIA_TIME {
        S_OK
    } else {
        E_INVALIDARG
    }
}
unsafe extern "win64" fn ms_duration(this: usize, out: *mut i64) -> i32 {
    put64(
        out,
        with_st(this, |st| st.duration_ms as i64) * UNITS_PER_MS,
    )
}
unsafe extern "win64" fn ms_stop_pos(this: usize, out: *mut i64) -> i32 {
    put64(
        out,
        with_st(this, |st| st.stop_ms.unwrap_or(st.duration_ms) as i64) * UNITS_PER_MS,
    )
}
unsafe extern "win64" fn ms_current(this: usize, out: *mut i64) -> i32 {
    put64(out, with_st(this, |st| position(st) as i64) * UNITS_PER_MS)
}
unsafe extern "win64" fn ms_convert(
    _this: usize,
    target: *mut i64,
    tf: *const Guid,
    source: i64,
    sf: *const Guid,
) -> i32 {
    let ok = |g: *const Guid| g.is_null() || *g == TIME_FORMAT_MEDIA_TIME;
    if !ok(tf) || !ok(sf) {
        return E_INVALIDARG;
    }
    put64(target, source)
}
unsafe extern "win64" fn ms_set_positions(
    this: usize,
    cur: *mut i64,
    cur_flags: u32,
    stop_p: *mut i64,
    stop_flags: u32,
) -> i32 {
    let s = sh(this);
    if !stop_p.is_null() && stop_flags & 3 != 0 {
        let v = stop_p.read_unaligned() / UNITS_PER_MS;
        let mut st = lock(&s.st);
        let base = st.stop_ms.unwrap_or(st.duration_ms) as i64;
        let ms = if stop_flags & 3 == 2 { base + v } else { v };
        st.stop_ms = Some(ms.max(0) as u64);
    }
    if !cur.is_null() && cur_flags & 3 != 0 {
        let v = cur.read_unaligned() / UNITS_PER_MS;
        let now = with_st(this, |st| position(st)) as i64;
        let ms = if cur_flags & 3 == 2 { now + v } else { v };
        seek(&s, ms.max(0) as u64);
    }
    S_OK
}
unsafe extern "win64" fn ms_get_positions(this: usize, cur: *mut i64, stop_p: *mut i64) -> i32 {
    if !cur.is_null() {
        ms_current(this, cur);
    }
    if !stop_p.is_null() {
        ms_stop_pos(this, stop_p);
    }
    S_OK
}
unsafe extern "win64" fn ms_available(this: usize, a: *mut i64, b: *mut i64) -> i32 {
    if !a.is_null() {
        a.write_unaligned(0);
    }
    if !b.is_null() {
        ms_duration(this, b);
    }
    S_OK
}
unsafe extern "win64" fn ms_set_rate(this: usize, rate: f64) -> i32 {
    if rate <= 0.0 {
        return E_INVALIDARG;
    }
    with_st(this, |st| st.rate = rate);
    S_OK
}
unsafe extern "win64" fn ms_get_rate(this: usize, out: *mut f64) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    out.write_unaligned(with_st(this, |st| st.rate));
    S_OK
}
unsafe extern "win64" fn ms_preroll(_this: usize, out: *mut i64) -> i32 {
    put64(out, 0)
}

fn seeking_vtable() -> usize {
    let mut m = unknown_head();
    m.extend([
        (f(ms_caps as *const ()), "i"),
        (f(ms_check_caps as *const ()), "i"),
        (f(ms_format_ok as *const ()), "i"),  // IsFormatSupported
        (f(ms_get_format as *const ()), "i"), // QueryPreferredFormat
        (f(ms_get_format as *const ()), "i"), // GetTimeFormat
        (f(ms_format_ok as *const ()), "i"),  // IsUsingTimeFormat
        (f(ms_set_format as *const ()), "i"),
        (f(ms_duration as *const ()), "i"),
        (f(ms_stop_pos as *const ()), "i"),
        (f(ms_current as *const ()), "i"),
        (f(ms_convert as *const ()), "iiqi"),
        (f(ms_set_positions as *const ()), "iiii"),
        (f(ms_get_positions as *const ()), "ii"),
        (f(ms_available as *const ()), "ii"),
        (f(ms_set_rate as *const ()), "d"),
        (f(ms_get_rate as *const ()), "i"),
        (f(ms_preroll as *const ()), "i"),
    ]);
    vtable_sig("IMediaSeeking", &m)
}

unsafe fn put_f64(p: *mut f64, v: f64) -> i32 {
    if p.is_null() {
        return E_POINTER;
    }
    p.write_unaligned(v);
    S_OK
}
unsafe extern "win64" fn mp_duration(this: usize, out: *mut f64) -> i32 {
    put_f64(out, with_st(this, |st| st.duration_ms) as f64 / 1000.0)
}
unsafe extern "win64" fn mp_put_current(this: usize, t: f64) -> i32 {
    seek(&sh(this), (t.max(0.0) * 1000.0) as u64);
    S_OK
}
unsafe extern "win64" fn mp_get_current(this: usize, out: *mut f64) -> i32 {
    put_f64(out, with_st(this, |st| position(st)) as f64 / 1000.0)
}
unsafe extern "win64" fn mp_get_stop(this: usize, out: *mut f64) -> i32 {
    put_f64(
        out,
        with_st(this, |st| st.stop_ms.unwrap_or(st.duration_ms)) as f64 / 1000.0,
    )
}
unsafe extern "win64" fn mp_put_stop(this: usize, t: f64) -> i32 {
    with_st(this, |st| st.stop_ms = Some((t.max(0.0) * 1000.0) as u64));
    S_OK
}
unsafe extern "win64" fn mp_get_zero(_this: usize, out: *mut f64) -> i32 {
    put_f64(out, 0.0)
}
unsafe extern "win64" fn mp_put_ignored(_this: usize, _t: f64) -> i32 {
    S_OK
}
unsafe extern "win64" fn mp_put_rate(this: usize, r: f64) -> i32 {
    ms_set_rate(this, r)
}
unsafe extern "win64" fn mp_get_rate(this: usize, out: *mut f64) -> i32 {
    ms_get_rate(this, out)
}
unsafe extern "win64" fn mp_can_seek(_this: usize, out: *mut i32) -> i32 {
    out32(out, -1)
}

fn position_vtable() -> usize {
    let mut m = dispatch_head();
    m.extend([
        (f(mp_duration as *const ()), "i"),
        (f(mp_put_current as *const ()), "d"),
        (f(mp_get_current as *const ()), "i"),
        (f(mp_get_stop as *const ()), "i"),
        (f(mp_put_stop as *const ()), "d"),
        (f(mp_get_zero as *const ()), "i"), // get_PrerollTime
        (f(mp_put_ignored as *const ()), "d"),
        (f(mp_put_rate as *const ()), "d"),
        (f(mp_get_rate as *const ()), "i"),
        (f(mp_can_seek as *const ()), "i"),
        (f(mp_can_seek as *const ()), "i"),
    ]);
    vtable_sig("IMediaPosition", &m)
}

// ═══════════════════════════════════════════════════════════════════════
// IMediaFilter (et IPersist)
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn mf_class(_this: usize, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = CLSID_FILTER_GRAPH;
    S_OK
}
unsafe extern "win64" fn mf_run(this: usize, _start: i64) -> i32 {
    run(&sh(this))
}
unsafe extern "win64" fn mf_sync(_this: usize, out: usize) -> i32 {
    put_ptr(out, 0);
    S_OK
}

fn filter_vtable() -> usize {
    let mut m = unknown_head();
    m.extend([
        (f(mf_class as *const ()), "i"),
        (f(mc_stop as *const ()), ""),
        (f(mc_pause as *const ()), ""),
        (f(mf_run as *const ()), "q"),
        (f(mc_get_state as *const ()), "ii"),
        (f(gb_ok1 as *const ()), "i"), // SetSyncSource
        (f(mf_sync as *const ()), "p"),
    ]);
    vtable_sig("IMediaFilter", &m)
}

// ═══════════════════════════════════════════════════════════════════════
// Création
// ═══════════════════════════════════════════════════════════════════════

fn new_graph() -> usize {
    let event = unsafe {
        api!(
            "kernel32",
            "CreateEventW",
            fn(usize, i32, i32, usize) -> usize,
            0,
            1,
            0,
            0
        )
        .unwrap_or(0)
    };
    let sh = Arc::new(Shared {
        st: Mutex::new(State {
            file: None,
            duration_ms: 0,
            video: None,
            audio: false,
            run: Run::Stopped,
            session: 0,
            base_ms: 0,
            pos_ms: 0,
            stop_ms: None,
            rate: 1.0,
            ended: false,
            events: VecDeque::new(),
            notify: None,
            notify_flags: 0,
            owner: 0,
            visible: true,
            fullscreen: false,
            rect: None,
            dest: None,
            style: 0x06CF_0000,
            style_ex: 0,
            drain: 0,
            volume: 0,
            balance: 0,
        }),
        event,
        stop_render: AtomicBool::new(false),
        renderer: Mutex::new(None),
    });
    let tables = [
        control_vtable(),
        event_vtable(),
        video_window_vtable(),
        audio_vtable(),
        basic_video_vtable(),
        seeking_vtable(),
        position_vtable(),
        filter_vtable(),
    ];
    let g = Box::into_raw(Box::new(Graph {
        vtbl: graph_vtable(),
        refs: AtomicU32::new(1),
        sh,
        faces: tables.map(|vtbl| Face {
            vtbl,
            graph: std::ptr::null(),
        }),
    }));
    unsafe {
        for face in (*g).faces.iter_mut() {
            face.graph = g;
        }
    }
    GRAPHS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(g as usize);
    g as usize
}

/// Classes DirectShow intégrées (appelé par CoCreateInstance).
///
/// # Safety
/// `iid` et `out` sont valides.
pub unsafe fn create_builtin(
    clsid: &super::ole32::Guid,
    iid: &super::ole32::Guid,
    out: *mut *mut std::ffi::c_void,
) -> Option<i32> {
    let clsid: &Guid = &*(clsid as *const super::ole32::Guid as *const Guid);
    let iid: &Guid = &*(iid as *const super::ole32::Guid as *const Guid);
    if *clsid != CLSID_FILTER_GRAPH && *clsid != CLSID_FILTER_GRAPH_NO_THREAD {
        return None;
    }
    let g = new_graph();
    // `out` est une case de 8 octets côté runtime (passerelle wow32 comprise).
    let mut p: usize = 0;
    let hr = qi(g, iid, &mut p as *mut usize as usize);
    release(g);
    *out = p as *mut std::ffi::c_void;
    Some(hr)
}

// ---------- AMGetErrorTextA/W ----------
pub unsafe extern "win64" fn am_get_error_text_a(hr: i32, buf: *mut u8, max_len: u32) -> u32 {
    let msg = format!("DirectShow error 0x{:08X}", hr as u32);
    crate::dlls::put_a(&msg, buf, max_len)
}

pub unsafe extern "win64" fn am_get_error_text_w(hr: i32, buf: *mut u16, max_len: u32) -> u32 {
    let msg = format!("DirectShow error 0x{:08X}", hr as u32);
    crate::dlls::put_w(&msg, buf, max_len)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "AMGetErrorTextA" => fn am_get_error_text_a,
        "AMGetErrorTextW" => fn am_get_error_text_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_en_decibels() {
        assert_eq!(volume_pct(0), 100);
        assert_eq!(volume_pct(-10000), 0);
        assert_eq!(volume_pct(-2000), 10);
    }

    #[test]
    fn graphe_interfaces_et_evenements() {
        unsafe {
            let g = new_graph();
            let mut ctl: usize = 0;
            assert_eq!(
                qi(g, &IID_IMEDIACONTROL, &mut ctl as *mut usize as usize),
                S_OK
            );
            let mut ev: usize = 0;
            assert_eq!(
                qi(ctl, &IID_IMEDIAEVENTEX, &mut ev as *mut usize as usize),
                S_OK
            );
            let mut back: usize = 0;
            assert_eq!(
                qi(ev, &IID_IGRAPHBUILDER, &mut back as *mut usize as usize),
                S_OK
            );
            assert_eq!(back, g);
            // Sans fichier : Run refuse, GetEvent n'a rien.
            assert_eq!(mc_run(ctl), VFW_E_NOT_CONNECTED);
            let (mut c, mut a, mut b) = (0u32, 0isize, 0isize);
            assert_eq!(me_get_event(ev, &mut c, &mut a, &mut b, 0), E_ABORT);
            sh(g).push_event(EC_COMPLETE, 0, 0);
            assert_eq!(me_get_event(ev, &mut c, &mut a, &mut b, 0), S_OK);
            assert_eq!(c, EC_COMPLETE);
            let mut vw: usize = 0;
            assert_eq!(
                qi(g, &IID_IVIDEOWINDOW, &mut vw as *mut usize as usize),
                S_OK
            );
            vw_set_pos(vw, 10, 20, 300, 200);
            let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
            vw_get_pos(vw, &mut x, &mut y, &mut w, &mut h);
            assert_eq!((x, y, w, h), (10, 20, 300, 200));
            for p in [vw, back, ev, ctl, g] {
                release(p);
            }
            assert!(GRAPHS.lock().unwrap().is_empty() || !GRAPHS.lock().unwrap().contains(&g));
        }
    }
}
