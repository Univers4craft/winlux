//! API privée exposée aux programmes PE du shell (exportée par kernel32
//! comme `WinluxCreatePty`) : ouverture, rendu, métadonnées, vignettes.

use super::{Content, Loaded, View};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

type Shared = Arc<Mutex<Loaded>>;

fn handles() -> &'static Mutex<HashMap<usize, Shared>> {
    static H: OnceLock<Mutex<HashMap<usize, Shared>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get(h: usize) -> Option<Shared> {
    handles().lock().ok()?.get(&h).cloned()
}

static LAST_ERROR: Mutex<(i32, &'static str)> = Mutex::new((0, ""));

fn set_error(e: super::Error) {
    if let Ok(mut l) = LAST_ERROR.lock() {
        *l = (e.code(), e.detail());
    }
}

unsafe fn path_of(p: *const u16) -> Option<PathBuf> {
    if p.is_null() {
        return None;
    }
    let s = crate::dlls::wstr(p);
    if s.is_empty() {
        return None;
    }
    Some(crate::paths::to_linux(&s))
}

/// Copie une chaîne UTF-8 en Latin-1 (police du shell) terminée par 0.
unsafe fn put_latin1(s: &str, buf: *mut u8, cap: u32) -> u32 {
    if buf.is_null() || cap == 0 {
        return 0;
    }
    let mut n = 0usize;
    for c in s.chars() {
        if n + 1 >= cap as usize {
            break;
        }
        let b = match c as u32 {
            0..=0xFF => c as u32 as u8,
            0x2019 | 0x2018 => b'\'',
            0x201C | 0x201D => b'"',
            0x2013 | 0x2014 => b'-',
            0x00D7 => 0xD7,
            _ => b'?',
        };
        *buf.add(n) = b;
        n += 1;
    }
    *buf.add(n) = 0;
    n as u32
}

#[repr(C)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub pages: u32,
    /// 1 animé, 2 calques, 4 RAW, 8 aperçu intégré, 16 HDR, 32 vectoriel, 64 modèle 3D
    pub flags: u32,
    pub delay_ms: u32,
    pub meta_count: u32,
    pub format: [u8; 32],
}

#[repr(C)]
pub struct PageInfo {
    pub width: u32,
    pub height: u32,
    pub delay_ms: u32,
    pub kind: u32,
    pub name: [u8; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ViewC {
    pub zoom: f32,
    pub cx: f32,
    pub cy: f32,
    pub rot: u32,
    pub mirror: u32,
    pub yaw: f32,
    pub pitch: f32,
    pub exposure: f32,
    pub auto_levels: u32,
    pub split: f32,
}

/// Ouvre et décode une image. Renvoie une poignée (0 = échec, voir
/// `WinluxImageLastError`).
pub unsafe extern "win64" fn winlux_image_open(path: *const u16) -> usize {
    let Some(p) = path_of(path) else {
        set_error(super::Error::Io);
        return 0;
    };
    match super::decode_file(&p) {
        Ok(img) => {
            static NEXT: Mutex<usize> = Mutex::new(0xB000_0000);
            let h = {
                let mut n = NEXT.lock().unwrap_or_else(|e| e.into_inner());
                *n += 1;
                *n
            };
            if let Ok(mut m) = handles().lock() {
                m.insert(h, Arc::new(Mutex::new(Loaded::new(img))));
            }
            h
        }
        Err(e) => {
            set_error(e);
            0
        }
    }
}

/// Code de la dernière erreur (1 non pris en charge, 2 variante, 3 corrompu,
/// 4 trop grand, 5 E/S) et détail technique dans `buf`.
pub unsafe extern "win64" fn winlux_image_last_error(buf: *mut u8, cap: u32) -> i32 {
    let (c, d) = LAST_ERROR.lock().map(|l| *l).unwrap_or((5, ""));
    put_latin1(d, buf, cap);
    c
}

pub unsafe extern "win64" fn winlux_image_info(h: usize, out: *mut ImageInfo) -> i32 {
    let Some(l) = get(h) else { return 0 };
    let Ok(l) = l.lock() else { return 0 };
    if out.is_null() {
        return 0;
    }
    let img = &l.img;
    let (w, hh) = img.pages.first().map(|p| p.size()).unwrap_or((0, 0));
    let mut flags = 0;
    if img.animated {
        flags |= 1;
    }
    if img.layered {
        flags |= 2;
    }
    if img.raw {
        flags |= 4;
    }
    if img.embedded_preview {
        flags |= 8;
    }
    if img.hdr {
        flags |= 16;
    }
    match img.pages.first().map(|p| &p.content) {
        Some(Content::Vector(_)) => flags |= 32,
        Some(Content::Model(_)) => flags |= 64,
        _ => {}
    }
    let o = &mut *out;
    o.width = w;
    o.height = hh;
    o.pages = img.pages.len() as u32;
    o.flags = flags;
    o.delay_ms = img.pages.first().map(|p| p.delay_ms).unwrap_or(0);
    o.meta_count = img.meta.len() as u32;
    o.format = [0; 32];
    put_latin1(img.format, o.format.as_mut_ptr(), 32);
    1
}

pub unsafe extern "win64" fn winlux_image_page(h: usize, page: u32, out: *mut PageInfo) -> i32 {
    let Some(l) = get(h) else { return 0 };
    let Ok(l) = l.lock() else { return 0 };
    let Some(p) = l.img.pages.get(page as usize) else { return 0 };
    if out.is_null() {
        return 0;
    }
    let (w, hh) = p.size();
    let o = &mut *out;
    o.width = w;
    o.height = hh;
    o.delay_ms = p.delay_ms;
    o.kind = match p.content {
        Content::Raster(_) => 0,
        Content::Vector(_) => 1,
        Content::Model(_) => 2,
    };
    o.name = [0; 64];
    put_latin1(&p.name, o.name.as_mut_ptr(), 64);
    1
}

/// Rend une page en BGRA 32 bits dans `out` (`stride` octets par ligne).
/// `bg` = 0x00RRGGBB ; `flags` bit 0 : damier sous la transparence.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn winlux_image_render(h: usize, page: u32, view: *const ViewC, out: *mut u8, w: u32, hh: u32, stride: u32, bg: u32, flags: u32) -> i32 {
    if out.is_null() || view.is_null() || w == 0 || hh == 0 || w > 16384 || hh > 16384 || stride < w * 4 {
        return 0;
    }
    let Some(l) = get(h) else { return 0 };
    let Ok(mut l) = l.lock() else { return 0 };
    let v = *view;
    let vw = View {
        zoom: if v.zoom.is_finite() && v.zoom > 0.0 { v.zoom.clamp(1e-4, 256.0) } else { 1.0 },
        cx: v.cx,
        cy: v.cy,
        rot: (v.rot % 4) as u8,
        mirror: v.mirror != 0,
        yaw: v.yaw,
        pitch: v.pitch,
        exposure: v.exposure.clamp(-5.0, 5.0),
        auto_levels: v.auto_levels != 0,
        split: v.split,
    };
    let buf = std::slice::from_raw_parts_mut(out, stride as usize * hh as usize);
    let bgc = [(bg >> 16) as u8, (bg >> 8) as u8, bg as u8];
    l.render(page as usize, &vw, w as usize, hh as usize, stride as usize, buf, bgc, flags & 1 != 0);
    1
}

/// Métadonnée numéro `i` : clé stable (ex. « camera.model ») et valeur.
pub unsafe extern "win64" fn winlux_image_meta(h: usize, i: u32, key: *mut u8, kcap: u32, val: *mut u8, vcap: u32) -> i32 {
    let Some(l) = get(h) else { return 0 };
    let Ok(l) = l.lock() else { return 0 };
    let Some((k, v)) = l.img.meta.get(i as usize) else { return 0 };
    put_latin1(k, key, kcap);
    put_latin1(v, val, vcap);
    1
}

pub unsafe extern "win64" fn winlux_image_close(h: usize) -> i32 {
    handles().lock().map(|mut m| m.remove(&h).is_some() as i32).unwrap_or(0)
}

/// 1 si la visionneuse sait ouvrir ce fichier (d'après son extension).
pub unsafe extern "win64" fn winlux_image_supported(path: *const u16) -> i32 {
    let Some(p) = path_of(path) else { return 0 };
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    super::supported_ext(&ext) as i32
}

/// Liste des formats annoncés, une ligne par famille : « Nom: ext ext ».
pub unsafe extern "win64" fn winlux_image_formats(buf: *mut u8, cap: u32) -> u32 {
    let mut s = String::new();
    for f in super::FORMATS.iter().filter(|f| f.supported) {
        s.push_str(f.name);
        s.push_str(": ");
        s.push_str(&f.exts.join(" "));
        s.push('\n');
    }
    put_latin1(&s, buf, cap)
}

// ------------------------------------------------------------ vignettes

const MEM_MAX: usize = 768;

struct Thumbs {
    ready: HashMap<String, Option<Arc<Vec<u8>>>>,
    order: VecDeque<String>,
    queue: VecDeque<(PathBuf, u32, String)>,
    pending: std::collections::HashSet<String>,
    workers: usize,
}

fn thumbs() -> &'static (Mutex<Thumbs>, Condvar) {
    static T: OnceLock<(Mutex<Thumbs>, Condvar)> = OnceLock::new();
    T.get_or_init(|| {
        (
            Mutex::new(Thumbs {
                ready: HashMap::new(),
                order: VecDeque::new(),
                queue: VecDeque::new(),
                pending: std::collections::HashSet::new(),
                workers: 0,
            }),
            Condvar::new(),
        )
    })
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Clé chemin + taille + date de modification (même règle que les
/// vignettes vidéo, préfixe distinct).
pub fn thumb_key(p: &Path, size: u32) -> Option<String> {
    let m = std::fs::metadata(p).ok()?;
    let mt = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(format!("img-{:016x}-{size}-{mt}-{}", fnv(&p.to_string_lossy()), m.len()))
}

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("winlux").join("thumbnails")
}

/// Fichier de cache : « WLTB » + taille (u32) + BGRA, ou « WLTN » si le
/// fichier n'a pas de vignette possible.
fn read_cache(key: &str, size: u32) -> Option<Option<Vec<u8>>> {
    let d = std::fs::read(cache_dir().join(key)).ok()?;
    if d.starts_with(b"WLTN") {
        return Some(None);
    }
    if d.len() == 8 + (size * size * 4) as usize && d.starts_with(b"WLTB") && super::le32(&d, 4) == size {
        return Some(Some(d[8..].to_vec()));
    }
    None
}

fn write_cache(key: &str, size: u32, px: Option<&[u8]>) {
    let dir = cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut out = Vec::new();
    match px {
        Some(p) => {
            out.extend_from_slice(b"WLTB");
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(p);
        }
        None => out.extend_from_slice(b"WLTN"),
    }
    let tmp = dir.join(format!("{key}.tmp{}", std::process::id()));
    if std::fs::write(&tmp, &out).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(key));
    }
}

/// Génère une vignette (bloquant) : BGRA `size`×`size`, alpha non
/// prémultiplié, ou `None` si le fichier n'est pas lisible.
pub fn make_thumbnail(p: &Path, size: u32) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(p).ok()?;
    if meta.len() > 512 << 20 {
        return None;
    }
    let img = super::decode_file(p).ok()?;
    let mut l = Loaded::new(img);
    Some(super::thumbnail(&mut l, size))
}

fn remember(t: &mut Thumbs, key: String, v: Option<Arc<Vec<u8>>>) {
    if !t.ready.contains_key(&key) {
        t.order.push_back(key.clone());
    }
    t.ready.insert(key, v);
    while t.order.len() > MEM_MAX {
        if let Some(old) = t.order.pop_front() {
            t.ready.remove(&old);
        }
    }
}

fn worker() {
    let (m, cv) = thumbs();
    loop {
        let job = {
            let Ok(mut t) = m.lock() else { return };
            loop {
                if let Some(j) = t.queue.pop_back() {
                    break j;
                }
                t = match cv.wait(t) {
                    Ok(g) => g,
                    Err(_) => return,
                };
            }
        };
        let (path, size, key) = job;
        let px = match read_cache(&key, size) {
            Some(v) => v,
            None => {
                let v = make_thumbnail(&path, size);
                write_cache(&key, size, v.as_deref());
                v
            }
        };
        if let Ok(mut t) = m.lock() {
            t.pending.remove(&key);
            remember(&mut t, key, px.map(Arc::new));
        }
    }
}

/// Vignette asynchrone : 1 = prête (`out` rempli, `size`×`size`×4 BGRA),
/// 0 = en cours (redemander au prochain affichage), -1 = aucune vignette.
pub unsafe extern "win64" fn winlux_image_thumbnail(path: *const u16, size: u32, out: *mut u8) -> i32 {
    if out.is_null() || !(16..=512).contains(&size) {
        return -1;
    }
    let Some(p) = path_of(path) else { return -1 };
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !super::supported_ext(&ext) {
        return -1;
    }
    let Some(key) = thumb_key(&p, size) else { return -1 };
    let (m, cv) = thumbs();
    let Ok(mut t) = m.lock() else { return -1 };
    if let Some(v) = t.ready.get(&key) {
        return match v {
            Some(px) => {
                std::ptr::copy_nonoverlapping(px.as_ptr(), out, px.len());
                1
            }
            None => -1,
        };
    }
    if !t.pending.contains(&key) {
        // le cache disque est lu par le fil de travail : aucun accès disque ici
        t.pending.insert(key.clone());
        t.queue.push_back((p, size, key));
        // Les plus récentes d'abord ; borne la file si l'utilisateur fait défiler vite.
        while t.queue.len() > 256 {
            if let Some((_, _, k)) = t.queue.pop_front() {
                t.pending.remove(&k);
            }
        }
        if t.workers < 2 {
            t.workers += 1;
            let _ = std::thread::Builder::new().name("winlux-thumbs".into()).spawn(worker);
        }
        cv.notify_one();
    }
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "WinluxImageOpen" => fn winlux_image_open,
        "WinluxImageLastError" => fn winlux_image_last_error,
        "WinluxImageInfo" => fn winlux_image_info,
        "WinluxImagePage" => fn winlux_image_page,
        "WinluxImageRender" => fn winlux_image_render,
        "WinluxImageMeta" => fn winlux_image_meta,
        "WinluxImageClose" => fn winlux_image_close,
        "WinluxImageSupported" => fn winlux_image_supported,
        "WinluxImageFormats" => fn winlux_image_formats,
        "WinluxImageThumbnail" => fn winlux_image_thumbnail,
    }
}
