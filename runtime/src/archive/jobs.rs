//! Tâches de fond (extraction, compression, montage…) suivies par l'Explorer.

use super::util::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const RUNNING: u32 = 0;
pub const DONE: u32 = 1;
pub const FAILED: u32 = 2;
pub const NEED_PASSWORD: u32 = 3;
pub const BAD_PASSWORD: u32 = 4;
pub const READ_ONLY: u32 = 5;
pub const UNSUPPORTED: u32 = 6;
pub const CANCELLED: u32 = 7;

pub struct Job {
    pub progress: Progress,
    pub status: AtomicU32,
    pub message: Mutex<String>,
}

fn table() -> &'static Mutex<HashMap<u32, Arc<Job>>> {
    static T: OnceLock<Mutex<HashMap<u32, Arc<Job>>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT: AtomicU32 = AtomicU32::new(1);

/// Lance `f` dans un fil d'exécution ; le texte renvoyé devient le message.
pub fn spawn(f: impl FnOnce(&Progress) -> Res<String> + Send + 'static) -> u32 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let job = Arc::new(Job {
        progress: Progress::default(),
        status: AtomicU32::new(RUNNING),
        message: Mutex::new(String::new()),
    });
    table()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, job.clone());
    std::thread::spawn(move || {
        let r = f(&job.progress);
        let (status, msg) = match r {
            Ok(m) => (DONE, m),
            Err(ArcError::NeedPassword) => (NEED_PASSWORD, String::new()),
            Err(ArcError::BadPassword) => (BAD_PASSWORD, String::new()),
            Err(ArcError::ReadOnly) => (READ_ONLY, String::new()),
            Err(ArcError::Unsupported(s)) => (UNSUPPORTED, s),
            Err(ArcError::Cancelled) => (CANCELLED, String::new()),
            Err(ArcError::Io(e)) => (FAILED, e.to_string()),
        };
        if status != DONE {
            crate::log_warn!("archive", "tâche {id} : statut {status} {msg}");
        }
        *job.message.lock().unwrap_or_else(|e| e.into_inner()) = msg;
        job.status.store(status, Ordering::Release);
    });
    id
}

pub fn get(id: u32) -> Option<Arc<Job>> {
    table()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned()
}

pub fn close(id: u32) {
    table()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
}
