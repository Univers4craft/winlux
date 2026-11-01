//! Client PipeWire natif (libpipewire-0.3 chargée à l'exécution).
//!
//! Un `pw_stream` par sens (lecture ou capture) ; le rappel temps réel de
//! PipeWire échange les échantillons avec le thread du mixeur via un anneau
//! protégé, ce qui donne au mixeur la même interface bloquante que les autres
//! pilotes. Les constantes et la mise en page des structures viennent des
//! en-têtes publics de PipeWire 1.x (`pipewire/stream.h`, `spa/buffer`).

use super::backend::{load, Input, Kind, Lib, Output};
use super::{PERIOD, RATE};
use libc::{c_char, c_int, c_void};
use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

type Init = unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char);
type LoopNew = unsafe extern "C" fn(*const c_char, *const c_void) -> *mut c_void;
type LoopFn = unsafe extern "C" fn(*mut c_void);
type LoopStart = unsafe extern "C" fn(*mut c_void) -> c_int;
type LoopGet = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type PropsNew = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type StreamNew = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *mut c_void,
    *const Events,
    *mut c_void,
) -> *mut c_void;
type StreamConnect =
    unsafe extern "C" fn(*mut c_void, u32, u32, u32, *const *const u8, u32) -> c_int;
type Dequeue = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type Queue = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
type StreamState = unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int;

struct Api {
    _lib: Lib,
    loop_new: LoopNew,
    loop_get: LoopGet,
    loop_start: LoopStart,
    loop_lock: LoopFn,
    loop_unlock: LoopFn,
    loop_stop: LoopFn,
    loop_destroy: LoopFn,
    props_new: PropsNew,
    stream_new: StreamNew,
    connect: StreamConnect,
    dequeue: Dequeue,
    queue: Queue,
    state: StreamState,
    destroy: LoopFn,
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        // La bibliothèque n'est jamais déchargée : PipeWire garde des threads.
        let lib = Lib::open("libpipewire-0.3.so.0")?;
        let init: Init = load!(lib, "pw_init");
        init(std::ptr::null_mut(), std::ptr::null_mut());
        Some(Api {
            loop_new: load!(lib, "pw_thread_loop_new"),
            loop_get: load!(lib, "pw_thread_loop_get_loop"),
            loop_start: load!(lib, "pw_thread_loop_start"),
            loop_lock: load!(lib, "pw_thread_loop_lock"),
            loop_unlock: load!(lib, "pw_thread_loop_unlock"),
            loop_stop: load!(lib, "pw_thread_loop_stop"),
            loop_destroy: load!(lib, "pw_thread_loop_destroy"),
            props_new: load!(lib, "pw_properties_new_string"),
            stream_new: load!(lib, "pw_stream_new_simple"),
            connect: load!(lib, "pw_stream_connect"),
            dequeue: load!(lib, "pw_stream_dequeue_buffer"),
            queue: load!(lib, "pw_stream_queue_buffer"),
            state: load!(lib, "pw_stream_get_state"),
            destroy: load!(lib, "pw_stream_destroy"),
            _lib: lib,
        })
    })
    .as_ref()
}

/// `struct pw_stream_events` (version 2, 96 octets).
#[repr(C)]
struct Events {
    version: u32,
    callbacks: [Option<unsafe extern "C" fn(*mut c_void)>; 11],
}
const EV_PROCESS: usize = 7; // offset 64 : 8 + 7 * 8

/// Format brut F32LE stéréo à `RATE` Hz (SPA_PARAM_EnumFormat), tel que
/// produit par `spa_format_audio_raw_build`.
fn format_pod() -> Vec<u8> {
    let words: [u32; 42] = [
        0xa0, 15, 0x40003, 3, // objet Format, id EnumFormat
        1, 0, 4, 3, 1, 0, // mediaType = audio
        2, 0, 4, 3, 1, 0, // mediaSubtype = raw
        0x10001, 0, 4, 3, 0x11b, 0, // format = F32_LE
        0x10003, 0, 4, 4, RATE, 0, // rate
        0x10004, 0, 4, 4, 2, 0, // channels
        0x10005, 0, 16, 13, 4, 3, 3, 4, // position = [FL, FR]
    ];
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

struct Shared {
    ring: Mutex<VecDeque<f32>>,
    cv: Condvar,
    capture: bool,
    stream: AtomicPtr<c_void>,
    dequeue: Dequeue,
    queue: Queue,
}

/// Capacité de l'anneau : 3 périodes en lecture, 1 s en capture.
fn capacity(capture: bool) -> usize {
    if capture {
        RATE as usize * 2
    } else {
        PERIOD * 2 * 3
    }
}

unsafe extern "C" fn on_process(data: *mut c_void) {
    let sh = &*(data as *const Shared);
    let stream = sh.stream.load(Ordering::Acquire);
    if stream.is_null() {
        return;
    }
    let b = (sh.dequeue)(stream);
    if b.is_null() {
        return;
    }
    // pw_buffer { spa_buffer *buffer; void *user_data; u64 size; u64 requested; }
    let spa = *(b as *const *const u8);
    let n_datas = *(spa.add(4) as *const u32);
    let datas = *(spa.add(16) as *const *mut u8);
    if n_datas == 0 || datas.is_null() {
        (sh.queue)(stream, b);
        return;
    }
    // spa_data { u32 type; u32 flags; i64 fd; u32 mapoffset; u32 maxsize; void *data; spa_chunk *chunk; }
    let maxsize = *(datas.add(20) as *const u32) as usize;
    let ptr = *(datas.add(24) as *const *mut f32);
    let chunk = *(datas.add(32) as *const *mut u32);
    if ptr.is_null() || chunk.is_null() {
        (sh.queue)(stream, b);
        return;
    }
    if sh.capture {
        let off = (*chunk) as usize;
        let size = (*chunk.add(1) as usize).min(maxsize.saturating_sub(off));
        let src = std::slice::from_raw_parts((ptr as *const u8).add(off) as *const f32, size / 4);
        let mut ring = crate::dlls::lock(&sh.ring);
        let cap = capacity(true);
        ring.extend(src.iter().copied());
        while ring.len() > cap {
            ring.pop_front();
        }
    } else {
        let mut frames = maxsize / 8;
        let requested = *(b.add(24) as *const u64) as usize;
        if requested > 0 {
            frames = frames.min(requested);
        }
        let out = std::slice::from_raw_parts_mut(ptr, frames * 2);
        let mut ring = crate::dlls::lock(&sh.ring);
        let n = out.len().min(ring.len());
        for (o, v) in out.iter_mut().zip(ring.drain(..n)) {
            *o = v;
        }
        out[n..].fill(0.0);
        // spa_chunk { u32 offset; u32 size; i32 stride; i32 flags; }
        *chunk = 0;
        *chunk.add(1) = (frames * 8) as u32;
        *chunk.add(2) = 8;
    }
    (sh.queue)(stream, b);
    sh.cv.notify_all();
}

pub struct Stream {
    api: &'static Api,
    thread_loop: *mut c_void,
    stream: *mut c_void,
    shared: *const Shared,
    _events: Box<Events>,
    failed: AtomicBool,
}
unsafe impl Send for Stream {}

fn quote(s: &str) -> String {
    s.chars().filter(|c| *c != '"' && *c != '\\').collect()
}

impl Stream {
    pub fn open(app: &str, capture: bool) -> Option<Stream> {
        let api = api()?;
        unsafe {
            let name = CString::new(if capture {
                "winlux-capture"
            } else {
                "winlux-playback"
            })
            .ok()?;
            let thread_loop = (api.loop_new)(name.as_ptr(), std::ptr::null());
            if thread_loop.is_null() {
                return None;
            }
            let app = quote(app);
            let props = format!(
                "media.type=Audio media.category={} media.role={} \
                 application.name=\"{app}\" application.process.binary=\"{app}\" \
                 node.name=\"{app}\" node.description=\"{app}\" \
                 node.latency={PERIOD}/{RATE}",
                if capture { "Capture" } else { "Playback" },
                if capture { "Communication" } else { "Game" },
            );
            let props = CString::new(props).ok()?;
            let props = (api.props_new)(props.as_ptr());
            let mut cbs: [Option<unsafe extern "C" fn(*mut c_void)>; 11] = [None; 11];
            cbs[EV_PROCESS] = Some(on_process);
            let events = Box::new(Events {
                version: 2,
                callbacks: cbs,
            });
            let shared = Arc::into_raw(Arc::new(Shared {
                ring: Mutex::new(VecDeque::with_capacity(capacity(capture))),
                cv: Condvar::new(),
                capture,
                stream: AtomicPtr::new(std::ptr::null_mut()),
                dequeue: api.dequeue,
                queue: api.queue,
            }));
            (api.loop_lock)(thread_loop);
            let stream = (api.stream_new)(
                (api.loop_get)(thread_loop),
                name.as_ptr(),
                props,
                &*events,
                shared as *mut c_void,
            );
            if stream.is_null() {
                (api.loop_unlock)(thread_loop);
                (api.loop_destroy)(thread_loop);
                drop(Arc::from_raw(shared));
                return None;
            }
            (*shared).stream.store(stream, Ordering::Release);
            let pod = format_pod();
            let params = [pod.as_ptr()];
            // Direction : entrée = 0, sortie = 1 ; PW_ID_ANY ;
            // AUTOCONNECT | MAP_BUFFERS | RT_PROCESS.
            let rc = (api.connect)(
                stream,
                if capture { 0 } else { 1 },
                u32::MAX,
                0x1 | 0x4 | 0x10,
                params.as_ptr(),
                1,
            );
            (api.loop_unlock)(thread_loop);
            let s = Stream {
                api,
                thread_loop,
                stream,
                shared,
                _events: events,
                failed: AtomicBool::new(false),
            };
            if rc < 0 || (api.loop_start)(thread_loop) < 0 {
                return None;
            }
            // Attend que le graphe accepte le flux (ou refuse : état « error »).
            for _ in 0..100 {
                (api.loop_lock)(thread_loop);
                let st = (api.state)(stream, std::ptr::null_mut());
                (api.loop_unlock)(thread_loop);
                match st {
                    -1 => return None,
                    2 | 3 => return Some(s),
                    _ => std::thread::sleep(Duration::from_millis(20)),
                }
            }
            None
        }
    }

    fn shared(&self) -> &Shared {
        unsafe { &*self.shared }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            (self.api.loop_lock)(self.thread_loop);
            (*self.shared)
                .stream
                .store(std::ptr::null_mut(), Ordering::Release);
            (self.api.destroy)(self.stream);
            (self.api.loop_unlock)(self.thread_loop);
            (self.api.loop_stop)(self.thread_loop);
            (self.api.loop_destroy)(self.thread_loop);
            drop(Arc::from_raw(self.shared));
        }
    }
}

impl Output for Stream {
    fn kind(&self) -> Kind {
        Kind::PipeWire
    }
    fn write(&mut self, stereo: &[f32]) -> bool {
        if self.failed.load(Ordering::Relaxed) {
            return false;
        }
        let sh = self.shared();
        let cap = capacity(false);
        let mut ring = crate::dlls::lock(&sh.ring);
        while ring.len() + stereo.len() > cap {
            let (g, t) = sh
                .cv
                .wait_timeout(ring, Duration::from_secs(1))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            ring = g;
            if t.timed_out() {
                // Personne ne consomme (pas de sortie reliée) : on abandonne
                // PipeWire pour la sortie nulle.
                self.failed.store(true, Ordering::Relaxed);
                return false;
            }
        }
        ring.extend(stereo.iter().copied());
        true
    }
}

impl Input for Stream {
    fn read(&mut self, stereo: &mut [f32]) -> bool {
        let sh = self.shared();
        let mut ring = crate::dlls::lock(&sh.ring);
        while ring.len() < stereo.len() {
            let (g, t) = sh
                .cv
                .wait_timeout(ring, Duration::from_secs(1))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            ring = g;
            if t.timed_out() {
                return false;
            }
        }
        let n = stereo.len();
        for (o, v) in stereo.iter_mut().zip(ring.drain(..n)) {
            *o = v;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pod_de_format_identique_a_spa_format_audio_raw_build() {
        let pod = super::format_pod();
        assert_eq!(pod.len(), 168);
        // Taille du corps (0xa0) + 8 octets d'en-tête = 168.
        assert_eq!(u32::from_le_bytes(pod[0..4].try_into().unwrap()) + 8, 168);
    }
}
