//! Presse-papiers Win32 réel, partagé par tous les programmes Winlux.
//!
//! Le contenu validé (`CloseClipboard`) est écrit dans un fichier unique de
//! la session (`$XDG_RUNTIME_DIR/winlux/clipboard.bin`), relu par les autres
//! processus quand son numéro de séquence change. Ce numéro alimente
//! `GetClipboardSequenceNumber`, ce qui permet à l'historique du bureau
//! (Win+V) de voir chaque nouvelle copie, quel que soit le programme.
//!
//! Formats : tous les formats sont conservés tels quels (CF_DIB, CF_HDROP,
//! formats enregistrés…). CF_TEXT, CF_OEMTEXT et CF_UNICODETEXT sont
//! synthétisés l'un depuis l'autre comme sous Windows.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::lock;

pub const CF_TEXT: u32 = 1;
pub const CF_OEMTEXT: u32 = 7;
pub const CF_DIB: u32 = 8;
pub const CF_UNICODETEXT: u32 = 13;
pub const CF_HDROP: u32 = 15;
const FIRST_REGISTERED: u32 = 0xC000;
const ERROR_CLIPBOARD_NOT_OPEN: u32 = 1418;
const MAGIC: &[u8; 8] = b"WLXCLIP1";
/// Garde-fou : une copie de plus de 256 Mio n'est pas conservée.
const MAX_BYTES: usize = 256 * 1024 * 1024;

/// Contenu validé du presse-papiers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents {
    pub seq: u32,
    pub formats: Vec<(u32, Vec<u8>)>,
}

impl Contents {
    pub fn get(&self, format: u32) -> Option<&[u8]> {
        self.formats
            .iter()
            .find(|(f, _)| *f == format)
            .map(|(_, d)| d.as_slice())
    }

    fn set(&mut self, format: u32, data: Vec<u8>) {
        if let Some(slot) = self.formats.iter_mut().find(|(f, _)| *f == format) {
            slot.1 = data;
        } else {
            self.formats.push((format, data));
        }
    }

    /// Formats disponibles, synthétisés compris, dans l'ordre d'énumération.
    pub fn available(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self.formats.iter().map(|(f, _)| *f).collect();
        let text = out
            .iter()
            .any(|f| matches!(*f, CF_TEXT | CF_OEMTEXT | CF_UNICODETEXT));
        if text {
            for f in [CF_UNICODETEXT, CF_TEXT, CF_OEMTEXT] {
                if !out.contains(&f) {
                    out.push(f);
                }
            }
        }
        out
    }

    /// Données d'un format, en synthétisant les variantes de texte.
    pub fn render(&self, format: u32) -> Option<Vec<u8>> {
        if let Some(d) = self.get(format) {
            return Some(d.to_vec());
        }
        match format {
            CF_TEXT | CF_OEMTEXT => {
                let text = self.text()?;
                let mut bytes = super::encode_ansi(&text);
                bytes.push(0);
                Some(bytes)
            }
            CF_UNICODETEXT => {
                let text = self.text()?;
                let mut out = Vec::with_capacity(text.len() * 2 + 2);
                for u in text.encode_utf16().chain(std::iter::once(0)) {
                    out.extend_from_slice(&u.to_le_bytes());
                }
                Some(out)
            }
            _ => None,
        }
    }

    /// Texte du presse-papiers, depuis le meilleur format disponible.
    pub fn text(&self) -> Option<String> {
        if let Some(w) = self.get(CF_UNICODETEXT) {
            let units: Vec<u16> = w
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&u| u != 0)
                .collect();
            return Some(String::from_utf16_lossy(&units));
        }
        let a = self.get(CF_TEXT).or_else(|| self.get(CF_OEMTEXT))?;
        let end = a.iter().position(|&b| b == 0).unwrap_or(a.len());
        Some(super::decode_ansi(&a[..end]))
    }

    pub fn encode(&self, names: &HashMap<u32, String>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.seq.to_le_bytes());
        out.extend_from_slice(&(self.formats.len() as u32).to_le_bytes());
        for (format, data) in &self.formats {
            let name = names.get(format).map(String::as_bytes).unwrap_or(&[]);
            out.extend_from_slice(&format.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(name);
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(data);
        }
        out
    }

    /// Décode le fichier partagé; renvoie aussi les noms des formats enregistrés.
    pub fn decode(bytes: &[u8]) -> Option<(Self, Vec<(u32, String)>)> {
        let mut r = Reader { bytes, pos: 0 };
        if r.take(8)? != MAGIC {
            return None;
        }
        let seq = r.u32()?;
        let count = r.u32()? as usize;
        let mut formats = Vec::with_capacity(count.min(64));
        let mut names = Vec::new();
        for _ in 0..count {
            let format = r.u32()?;
            let name_len = r.u16()? as usize;
            let name = r.take(name_len)?;
            if !name.is_empty() {
                names.push((format, String::from_utf8_lossy(name).into_owned()));
            }
            let len = r.u32()? as usize;
            formats.push((format, r.take(len)?.to_vec()));
        }
        Some((Self { seq, formats }, names))
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let out = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(out)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
}

/// Fichier partagé de la session.
pub fn shared_path() -> PathBuf {
    if let Some(p) = std::env::var_os("WINLUX_CLIPBOARD_FILE") {
        return PathBuf::from(p);
    }
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(d).join("winlux").join("clipboard.bin");
    }
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir()
        .join(format!("winlux-{uid}"))
        .join("clipboard.bin")
}

/// Numéro de séquence enregistré dans le fichier (lecture de l'en-tête seul).
pub fn read_seq(path: &Path) -> u32 {
    use std::io::Read;
    let mut head = [0u8; 12];
    match std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)) {
        Ok(()) if &head[..8] == MAGIC => u32::from_le_bytes(head[8..12].try_into().unwrap()),
        _ => 0,
    }
}

pub fn load(path: &Path) -> Option<(Contents, Vec<(u32, String)>)> {
    let bytes = std::fs::read(path).ok()?;
    Contents::decode(&bytes)
}

/// Écrit atomiquement (fichier temporaire puis renommage) et en mode 0600.
pub fn store(path: &Path, contents: &Contents, names: &HashMap<u32, String>) -> bool {
    let Some(dir) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let tmp = dir.join(format!(".clipboard-{}.tmp", std::process::id()));
    let ok = (|| {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(&contents.encode(names))?;
        f.sync_data()?;
        std::fs::rename(&tmp, path)
    })();
    if ok.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    ok.is_ok()
}

/// Identifiant stable d'un format enregistré : identique dans tous les
/// processus, sans serveur central (FNV-1a du nom, sans casse).
pub fn registered_id(name: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for b in name.to_lowercase().bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    FIRST_REGISTERED + h % 0x3FFF
}

fn standard_name(format: u32) -> Option<&'static str> {
    Some(match format {
        CF_TEXT => "CF_TEXT",
        CF_OEMTEXT => "CF_OEMTEXT",
        CF_DIB => "CF_DIB",
        CF_UNICODETEXT => "CF_UNICODETEXT",
        CF_HDROP => "CF_HDROP",
        _ => return None,
    })
}

// ---------- État du processus ----------

#[derive(Default)]
struct State {
    open: bool,
    /// Contenu en cours de construction entre EmptyClipboard et CloseClipboard.
    pending: Option<Contents>,
    /// Dernier contenu lu sur disque.
    cache: Contents,
    names: HashMap<u32, String>,
    /// HGLOBAL rendus par GetClipboardData pour la séquence `handles_seq`.
    handles: HashMap<u32, usize>,
    handles_seq: u32,
    /// HGLOBAL confiés par SetClipboardData (libérés au contenu suivant).
    owned: Vec<usize>,
    enum_cache: Vec<u32>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

fn refresh(s: &mut State) {
    let path = shared_path();
    let seq = read_seq(&path);
    if seq == s.cache.seq && (seq != 0 || s.cache.formats.is_empty()) {
        return;
    }
    match load(&path) {
        Some((contents, names)) => {
            for (id, name) in names {
                s.names.insert(id, name);
            }
            s.cache = contents;
        }
        None => s.cache = Contents::default(),
    }
}

/// Contenu visible : celui en construction si ce processus l'a ouvert.
fn current(s: &mut State) -> Contents {
    if let Some(p) = s.pending.as_ref() {
        return p.clone();
    }
    refresh(s);
    s.cache.clone()
}

fn global_bytes(handle: usize) -> Option<Vec<u8>> {
    let memory = super::kernel32::lookup_global_memory(handle)?;
    let bytes = lock(&memory).bytes.clone();
    Some(bytes)
}

fn free_handles(s: &mut State) {
    for (_, h) in s.handles.drain() {
        super::kernel32::free_global_memory(h);
    }
}

pub extern "win64" fn open_clipboard(_hwnd: usize) -> i32 {
    lock(state()).open = true;
    1
}

pub extern "win64" fn close_clipboard() -> i32 {
    let mut s = lock(state());
    if !s.open {
        crate::process::set_last_error(ERROR_CLIPBOARD_NOT_OPEN);
        return 0;
    }
    s.open = false;
    if let Some(mut contents) = s.pending.take() {
        let path = shared_path();
        contents.seq = read_seq(&path).max(s.cache.seq).wrapping_add(1).max(1);
        if store(&path, &contents, &s.names) {
            s.cache = contents;
        } else {
            // Disque indisponible : le contenu reste au moins dans ce processus.
            s.cache = contents;
        }
        let owned = std::mem::take(&mut s.owned);
        free_handles(&mut s);
        drop(s);
        for h in owned {
            super::kernel32::free_global_memory(h);
        }
    }
    1
}

pub extern "win64" fn empty_clipboard() -> i32 {
    let mut s = lock(state());
    if !s.open {
        crate::process::set_last_error(ERROR_CLIPBOARD_NOT_OPEN);
        return 0;
    }
    s.pending = Some(Contents::default());
    1
}

pub extern "win64" fn set_clipboard_data(format: u32, data: usize) -> usize {
    let mut s = lock(state());
    if !s.open {
        crate::process::set_last_error(ERROR_CLIPBOARD_NOT_OPEN);
        return 0;
    }
    if format == 0 || data == 0 {
        // Le rendu différé (données NULL) n'est pas pris en charge.
        crate::process::set_last_error(87);
        return 0;
    }
    let Some(bytes) = global_bytes(data) else {
        crate::process::set_last_error(6);
        return 0;
    };
    if bytes.len() > MAX_BYTES {
        crate::process::set_last_error(8);
        return 0;
    }
    if s.pending.is_none() {
        let base = current(&mut s);
        s.pending = Some(base);
    }
    if let Some(p) = s.pending.as_mut() {
        if matches!(format, CF_TEXT | CF_OEMTEXT | CF_UNICODETEXT) {
            // Une nouvelle variante de texte remplace les variantes synthétisables.
            p.formats
                .retain(|(f, _)| !matches!(*f, CF_TEXT | CF_OEMTEXT | CF_UNICODETEXT));
        }
        p.set(format, bytes);
    }
    // Le système possède désormais la mémoire.
    s.owned.push(data);
    data
}

pub extern "win64" fn get_clipboard_data(format: u32) -> usize {
    let mut s = lock(state());
    let contents = current(&mut s);
    if s.handles_seq != contents.seq || s.pending.is_some() {
        free_handles(&mut s);
        s.handles_seq = contents.seq;
    }
    if let Some(&h) = s.handles.get(&format) {
        return h;
    }
    let Some(bytes) = contents.render(format) else {
        crate::process::set_last_error(0);
        return 0;
    };
    let Some((handle, memory)) = super::kernel32::create_global_memory(bytes.len(), 0x0002) else {
        crate::process::set_last_error(8);
        return 0;
    };
    lock(&memory).bytes.copy_from_slice(&bytes);
    if s.pending.is_none() {
        s.handles.insert(format, handle);
    }
    handle
}

pub extern "win64" fn is_clipboard_format_available(format: u32) -> i32 {
    let mut s = lock(state());
    i32::from(current(&mut s).available().contains(&format))
}

pub extern "win64" fn count_clipboard_formats() -> i32 {
    let mut s = lock(state());
    current(&mut s).available().len() as i32
}

pub extern "win64" fn enum_clipboard_formats(format: u32) -> u32 {
    let mut s = lock(state());
    if format == 0 {
        s.enum_cache = current(&mut s).available();
    }
    let list = &s.enum_cache;
    let next = if format == 0 {
        list.first()
    } else {
        list.iter()
            .position(|&f| f == format)
            .and_then(|i| list.get(i + 1))
    };
    crate::process::set_last_error(0);
    next.copied().unwrap_or(0)
}

pub extern "win64" fn get_clipboard_sequence_number() -> u32 {
    let mut s = lock(state());
    refresh(&mut s);
    s.cache.seq
}

pub extern "win64" fn get_priority_clipboard_format(list: *const u32, count: i32) -> i32 {
    if list.is_null() || count <= 0 {
        return 0;
    }
    let mut s = lock(state());
    let available = current(&mut s).available();
    if available.is_empty() {
        return 0;
    }
    let wanted = unsafe { std::slice::from_raw_parts(list, count as usize) };
    wanted
        .iter()
        .find(|f| available.contains(f))
        .map(|&f| f as i32)
        .unwrap_or(-1)
}

fn register(name: String) -> u32 {
    if name.is_empty() {
        crate::process::set_last_error(87);
        return 0;
    }
    let id = registered_id(&name);
    lock(state()).names.entry(id).or_insert(name);
    id
}

pub unsafe extern "win64" fn register_clipboard_format_a(name: *const u8) -> u32 {
    register(super::cstr(name))
}

pub unsafe extern "win64" fn register_clipboard_format_w(name: *const u16) -> u32 {
    register(super::wstr(name))
}

fn format_name(format: u32) -> Option<String> {
    if let Some(n) = standard_name(format) {
        return Some(n.to_string());
    }
    let mut s = lock(state());
    refresh(&mut s);
    s.names.get(&format).cloned()
}

pub unsafe extern "win64" fn get_clipboard_format_name_a(
    format: u32,
    buf: *mut u8,
    max: i32,
) -> i32 {
    // Les formats prédéfinis n'ont pas de nom sous Windows.
    if format < FIRST_REGISTERED || buf.is_null() || max <= 0 {
        return 0;
    }
    let Some(name) = format_name(format) else {
        return 0;
    };
    let bytes = super::encode_ansi(&name);
    let n = bytes.len().min(max as usize - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as i32
}

pub unsafe extern "win64" fn get_clipboard_format_name_w(
    format: u32,
    buf: *mut u16,
    max: i32,
) -> i32 {
    if format < FIRST_REGISTERED || buf.is_null() || max <= 0 {
        return 0;
    }
    let Some(name) = format_name(format) else {
        return 0;
    };
    let units: Vec<u16> = name.encode_utf16().collect();
    let n = units.len().min(max as usize - 1);
    std::ptr::copy_nonoverlapping(units.as_ptr(), buf, n);
    *buf.add(n) = 0;
    n as i32
}

pub extern "win64" fn get_clipboard_owner() -> usize {
    0
}

pub extern "win64" fn get_open_clipboard_window() -> usize {
    0
}

pub extern "win64" fn clipboard_listener(_hwnd: usize) -> i32 {
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "OpenClipboard" => fn open_clipboard,
        "CloseClipboard" => fn close_clipboard,
        "EmptyClipboard" => fn empty_clipboard,
        "SetClipboardData" => fn set_clipboard_data,
        "GetClipboardData" => fn get_clipboard_data,
        "IsClipboardFormatAvailable" => fn is_clipboard_format_available,
        "CountClipboardFormats" => fn count_clipboard_formats,
        "EnumClipboardFormats" => fn enum_clipboard_formats,
        "GetClipboardSequenceNumber" => fn get_clipboard_sequence_number,
        "GetPriorityClipboardFormat" => fn get_priority_clipboard_format,
        "RegisterClipboardFormatA" => fn register_clipboard_format_a,
        "RegisterClipboardFormatW" => fn register_clipboard_format_w,
        "GetClipboardFormatNameA" => fn get_clipboard_format_name_a,
        "GetClipboardFormatNameW" => fn get_clipboard_format_name_w,
        "GetClipboardOwner" => fn get_clipboard_owner,
        "GetOpenClipboardWindow" => fn get_open_clipboard_window,
        "AddClipboardFormatListener" => fn clipboard_listener,
        "RemoveClipboardFormatListener" => fn clipboard_listener,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "winlux-clip-test-{}-{tag}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        dir.join("clipboard.bin")
    }

    #[test]
    fn presse_papiers_aller_retour_fichier_partage() {
        let path = temp_file("rt");
        let mut c = Contents {
            seq: 7,
            ..Contents::default()
        };
        c.set(CF_UNICODETEXT, vec![b'h', 0, b'i', 0, 0, 0]);
        c.set(CF_DIB, vec![1, 2, 3, 4]);
        let id = registered_id("Winlux Test Format");
        c.set(id, vec![9]);
        let mut names = HashMap::new();
        names.insert(id, "Winlux Test Format".to_string());
        assert!(store(&path, &c, &names));
        assert_eq!(read_seq(&path), 7);
        let (back, back_names) = load(&path).unwrap();
        assert_eq!(back, c);
        assert_eq!(back_names, vec![(id, "Winlux Test Format".to_string())]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn texte_synthetise_entre_ansi_et_unicode() {
        let mut c = Contents::default();
        c.set(CF_TEXT, b"caf\xE9\0".to_vec());
        let wide = c.render(CF_UNICODETEXT).unwrap();
        let units: Vec<u16> = wide
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), "café\0");
        assert!(c.available().contains(&CF_UNICODETEXT));
        assert!(c.available().contains(&CF_OEMTEXT));
        let mut u = Contents::default();
        let wide: Vec<u8> = "€uro"
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        u.set(CF_UNICODETEXT, wide);
        assert_eq!(u.render(CF_TEXT).unwrap(), b"\x80uro\0");
        assert_eq!(u.text().unwrap(), "€uro");
    }

    #[test]
    fn formats_enregistres_stables_et_hors_plage_standard() {
        let a = registered_id("PNG");
        assert_eq!(a, registered_id("png"));
        assert!((FIRST_REGISTERED..=0xFFFF).contains(&a));
        assert_ne!(a, registered_id("HTML Format"));
    }

    #[test]
    fn fichier_tronque_ou_etranger_refuse() {
        assert!(Contents::decode(b"WLXCLIP1\x01\0\0\0\x05\0\0\0").is_none());
        assert!(Contents::decode(b"pas un presse-papiers").is_none());
        let path = temp_file("missing");
        assert_eq!(read_seq(&path), 0);
    }
}
