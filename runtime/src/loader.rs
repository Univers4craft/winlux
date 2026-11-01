//! Chargeur PE : place l'image en mémoire, applique les relocations,
//! relie les imports aux DLL maison (ou à de vraies DLL livrées avec le
//! programme) et prépare le TLS statique.

use crate::dlls;
use crate::pe::*;
use crate::thunks;
use crate::trace;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

thread_local! {
    static DLL_SEARCH: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}

/// Restrict a dynamic load and its dependency loads without changing the
/// process CWD/PATH or leaking search options to unrelated threads.
pub fn with_dll_search<T>(directories: Vec<PathBuf>, operation: impl FnOnce() -> T) -> T {
    struct Restore(Option<Vec<PathBuf>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DLL_SEARCH.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(DLL_SEARCH.with(|slot| slot.replace(Some(directories))));
    operation()
}

const PAGE: usize = 4096;

#[derive(Clone, Debug)]
pub struct TlsInfo {
    pub raw_start: usize,
    pub raw_end: usize,
    pub index_addr: usize,
    pub callbacks_addr: usize,
    pub zero_fill: usize,
}

/// Un module PE (le .exe ou une DLL réelle) chargé en mémoire.
#[derive(Clone, Debug)]
pub struct Module {
    pub name: String,
    pub path: PathBuf,
    pub base: usize,
    pub size: usize,
    pub pe: PeImage,
    pub tls: Option<TlsInfo>,
    pub initialized: bool,
    pub refcount: u32,
}

impl Module {
    /// Noms des DLL importées, lus dans l'image chargée en mémoire.
    pub fn import_names(&self) -> Vec<String> {
        let dir = self.pe.dir(1);
        let mut names = Vec::new();
        if dir.rva == 0 || dir.size < 20 {
            return names;
        }
        let end = (dir.rva as usize)
            .saturating_add(dir.size as usize)
            .min(self.size);
        let mut desc = dir.rva as usize;
        while desc + 20 <= end && names.len() < 512 {
            // SAFETY: le descripteur est dans l'image mappée (borné par `size`).
            let name_rva =
                unsafe { ((self.base + desc + 12) as *const u32).read_unaligned() } as usize;
            if name_rva == 0 || name_rva >= self.size {
                break;
            }
            let mut name = String::new();
            for i in 0..260 {
                if name_rva + i >= self.size {
                    break;
                }
                let b = unsafe { *((self.base + name_rva + i) as *const u8) };
                if b == 0 {
                    break;
                }
                name.push(b as char);
            }
            names.push(name);
            desc += 20;
        }
        names
    }
    pub fn entry(&self) -> Option<usize> {
        (self.pe.entry_rva != 0 && !self.pe.il_only32)
            .then(|| self.base + self.pe.entry_rva as usize)
    }
    pub fn contains(&self, addr: usize) -> bool {
        addr >= self.base && addr < self.base + self.size
    }
    fn bytes(&self) -> &'static [u8] {
        unsafe { std::slice::from_raw_parts(self.base as *const u8, self.size) }
    }
    fn cstr_at(&self, rva: u32) -> Option<String> {
        let b = self.bytes().get(rva as usize..)?;
        let end = b.iter().position(|&c| c == 0)?;
        Some(String::from_utf8_lossy(&b[..end]).into_owned())
    }
}

/// Modules PE réels chargés (l'exécutable est toujours le premier).
pub static MODULES: Mutex<Vec<Module>> = Mutex::new(Vec::new());

pub struct ModuleInfo {
    pub name: String,
    pub base: usize,
    pub size: usize,
}

pub fn list_modules() -> Vec<ModuleInfo> {
    MODULES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|m| ModuleInfo {
            name: m.name.clone(),
            base: m.base,
            size: m.size,
        })
        .collect()
}

pub fn find_module_by_addr(addr: usize) -> Option<Module> {
    MODULES
        .lock()
        .unwrap()
        .iter()
        .find(|m| m.contains(addr))
        .cloned()
}

pub fn find_module_by_name(name: &str) -> Option<Module> {
    let n = dlls::normalize(name);
    MODULES
        .lock()
        .unwrap()
        .iter()
        .find(|m| dlls::normalize(&m.name) == n)
        .cloned()
}

pub fn find_module_by_path(path: &Path) -> Option<Module> {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    MODULES
        .lock()
        .unwrap()
        .iter()
        .find(|m| m.path == canonical || m.path == path)
        .cloned()
}

pub fn main_module() -> Option<Module> {
    MODULES.lock().unwrap().first().cloned()
}

pub fn increment_ref(base: usize) {
    let mut mods = MODULES.lock().unwrap();
    if let Some(m) = mods.iter_mut().find(|m| m.base == base) {
        m.refcount = m.refcount.saturating_add(1);
    }
}

#[allow(dead_code)]
pub fn decrement_ref(base: usize) -> Option<u32> {
    let mut mods = MODULES.lock().unwrap();
    if let Some(m) = mods.iter_mut().find(|m| m.base == base) {
        m.refcount = m.refcount.saturating_sub(1);
        Some(m.refcount)
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn remove_module(base: usize) -> Option<Module> {
    let mut mods = MODULES.lock().unwrap();
    if let Some(pos) = mods.iter().position(|m| m.base == base) {
        if pos == 0 {
            return None;
        }
        Some(mods.remove(pos))
    } else {
        None
    }
}

fn round_up(v: usize, a: usize) -> usize {
    (v + a - 1) & !(a - 1)
}

unsafe fn map_at(hint: usize, size: usize, fixed: bool) -> Option<usize> {
    let flags =
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | if fixed { libc::MAP_FIXED_NOREPLACE } else { 0 };
    let p = libc::mmap(
        hint as *mut _,
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        flags,
        -1,
        0,
    );
    if p == libc::MAP_FAILED {
        return None;
    }
    if fixed && p as usize != hint {
        // Ancien noyau ignorant MAP_FIXED_NOREPLACE : l'adresse n'est pas celle demandée.
        libc::munmap(p, size);
        return None;
    }
    Some(p as usize)
}

/// Granularité d'allocation de Windows : une image relogée y est toujours
/// placée sur 64 Kio, et coreclr refuse un en-tête dont l'ImageBase ne
/// l'est pas (PEDecoder::CheckNTHeaders).
const ALLOCATION_GRANULARITY: usize = 0x10000;

/// Réserve `size` octets à une adresse alignée sur la granularité
/// d'allocation de Windows, en rendant au noyau les marges inutilisées.
unsafe fn map_aligned(size: usize) -> Option<usize> {
    let raw = map_at(0, size + ALLOCATION_GRANULARITY, false)?;
    let base = round_up(raw, ALLOCATION_GRANULARITY);
    if base > raw {
        libc::munmap(raw as *mut _, base - raw);
    }
    let tail = raw + size + ALLOCATION_GRANULARITY - (base + size);
    if tail > 0 {
        libc::munmap((base + size) as *mut _, tail);
    }
    Some(base)
}

/// Verrou de chargement, réentrant pour le même thread : deux threads qui
/// appellent LoadLibrary en même temps sont sérialisés (comme par le verrou
/// du chargeur Windows), mais un module chargé depuis la résolution des
/// imports ou depuis DllMain reprend le verrou sans s'interbloquer.
struct LoadLock {
    state: Mutex<(Option<std::thread::ThreadId>, usize)>,
    released: std::sync::Condvar,
}
static LOAD_LOCK: LoadLock = LoadLock {
    state: Mutex::new((None, 0)),
    released: std::sync::Condvar::new(),
};
struct LoadGuard;
impl Drop for LoadGuard {
    fn drop(&mut self) {
        let mut s = LOAD_LOCK
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        s.1 -= 1;
        if s.1 == 0 {
            s.0 = None;
            LOAD_LOCK.released.notify_all();
        }
    }
}
fn load_lock() -> LoadGuard {
    let me = std::thread::current().id();
    let mut s = LOAD_LOCK
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while s.0.is_some_and(|owner| owner != me) {
        s = LOAD_LOCK
            .released
            .wait(s)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    s.0 = Some(me);
    s.1 += 1;
    LoadGuard
}

/// Charge un module et toutes ses dépendances. Renvoie l'index dans MODULES.
pub fn load(path: &Path, force_reloc: bool) -> Result<Module, String> {
    let _serialized = load_lock();
    if let Some(existing) = find_module_by_path(path) {
        increment_ref(existing.base);
        return Ok(existing);
    }
    let m = load_unlocked(path, force_reloc)?;
    // Une liaison qui échoue (import introuvable, protection impossible) ne
    // laisse pas un module à moitié lié dans la liste : GetModuleHandle ne
    // doit pas le trouver et un nouveau LoadLibrary doit réessayer.
    let linked = if m.pe.il_only32 {
        protect(&m)
    } else {
        resolve_imports(&m).and_then(|()| protect(&m))
    };
    if let Err(e) = linked {
        remove_module(m.base);
        unsafe {
            libc::munmap(m.base as *mut libc::c_void, m.size);
        }
        return Err(e);
    }
    Ok(m)
}

/// Projette l'image et l'ajoute à MODULES ; la liaison (imports, protections)
/// reste à faire par `load`.
fn load_unlocked(path: &Path, force_reloc: bool) -> Result<Module, String> {
    let file = std::fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
    let pe = PeImage::parse_il_only(&file).map_err(|e| format!("{} : {e}", path.display()))?;
    let size = round_up(pe.size_of_image as usize, PAGE);
    let relocs_stripped = pe.characteristics & 0x0001 != 0;
    let has_relocs = pe.dir(DIR_BASERELOC).size != 0;

    let base = unsafe {
        let preferred = if pe.il_only32 {
            // Assembly IL seul : n'importe où, comme le fait Windows 64 bits.
            Some(map_aligned(size).ok_or("mémoire insuffisante pour l'image")?)
        } else if force_reloc && has_relocs {
            None
        } else {
            map_at(pe.image_base as usize, size, true)
        };
        match preferred {
            Some(b) => b,
            None if relocs_stripped || !has_relocs => {
                return Err(format!(
                    "{} : impossible de placer l'image à son adresse fixe 0x{:x} et elle n'a pas de relocations",
                    path.display(),
                    pe.image_base
                ))
            }
            None => map_aligned(size).ok_or("mémoire insuffisante pour l'image")?,
        }
    };
    trace!(
        "chargement de {} à 0x{base:x} (préférée 0x{:x}, {} octets)",
        path.display(),
        pe.image_base,
        size
    );

    // En-têtes puis sections.
    unsafe {
        let dst = std::slice::from_raw_parts_mut(base as *mut u8, size);
        let hs = (pe.size_of_headers as usize).min(file.len());
        dst[..hs].copy_from_slice(&file[..hs]);
        for s in &pe.sections {
            if s.raw_size == 0 || s.characteristics & SCN_CNT_UNINITIALIZED != 0 && s.raw_ptr == 0 {
                continue;
            }
            let start = s.raw_ptr as usize;
            let mut n = (s.raw_size as usize).min(file.len().saturating_sub(start));
            if s.virtual_size != 0 {
                n = n.min(s.virtual_size as usize);
            }
            let d = s.rva as usize;
            if d + n > size {
                return Err(format!(
                    "{} : section {} déborde de l'image",
                    path.display(),
                    s.name
                ));
            }
            dst[d..d + n].copy_from_slice(&file[start..start + n]);
        }
    }

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut m = Module {
        name,
        path: std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        base,
        size,
        pe,
        tls: None,
        initialized: false,
        refcount: 1,
    };

    if m.pe.il_only32 {
        // Ni relocations, ni imports (mscoree), ni point d'entrée : le CLR
        // lit les métadonnées directement dans l'image projetée.
        MODULES.lock().unwrap().push(m.clone());
        return Ok(m);
    }
    let delta = base.wrapping_sub(m.pe.image_base as usize);
    if delta != 0 {
        apply_relocations(&m, delta)?;
        // ImageBase dans l'en-tête en mémoire reflète la vraie adresse, comme sous Windows.
        unsafe {
            let pe_off = *((base + 0x3c) as *const u32) as usize;
            *((base + pe_off + 24 + 24) as *mut u64) = base as u64;
        }
    }
    m.tls = read_tls(&m);

    MODULES.lock().unwrap().push(m.clone());
    Ok(m)
}

fn apply_relocations(m: &Module, delta: usize) -> Result<(), String> {
    let d = m.pe.dir(DIR_BASERELOC);
    let b = m.bytes();
    let mut off = d.rva as usize;
    let end = off + d.size as usize;
    let mut count = 0usize;
    while off + 8 <= end {
        let page = rd32(b, off).ok_or("relocations tronquées")? as usize;
        let bsize = rd32(b, off + 4).ok_or("relocations tronquées")? as usize;
        if bsize < 8 {
            break;
        }
        for i in 0..(bsize - 8) / 2 {
            let e = rd16(b, off + 8 + i * 2).ok_or("relocations tronquées")?;
            let (ty, ofs) = (e >> 12, (e & 0xfff) as usize);
            let addr = m.base + page + ofs;
            if page + ofs + 8 > m.size && ty != 0 {
                return Err("relocation hors de l'image".into());
            }
            unsafe {
                match ty {
                    0 => {}
                    10 => {
                        let p = addr as *mut u64;
                        p.write_unaligned(p.read_unaligned().wrapping_add(delta as u64));
                    }
                    3 => {
                        let p = addr as *mut u32;
                        p.write_unaligned(p.read_unaligned().wrapping_add(delta as u32));
                    }
                    t => return Err(format!("type de relocation {t} non pris en charge")),
                }
            }
            count += 1;
        }
        off += bsize;
    }
    trace!("{} relocations appliquées (décalage 0x{delta:x})", count);
    Ok(())
}

fn read_tls(m: &Module) -> Option<TlsInfo> {
    let d = m.pe.dir(DIR_TLS);
    if d.rva == 0 {
        return None;
    }
    let b = m.bytes();
    let o = d.rva as usize;
    Some(TlsInfo {
        raw_start: rd64(b, o)? as usize,
        raw_end: rd64(b, o + 8)? as usize,
        index_addr: rd64(b, o + 16)? as usize,
        callbacks_addr: rd64(b, o + 24)? as usize,
        zero_fill: rd32(b, o + 32)? as usize,
    })
}

enum Want {
    Name(String),
    Ordinal(u16),
}

/// Résout un import : DLL maison d'abord, puis DLL réelle à côté du .exe.
fn resolve_one(dll: &str, want: &Want, importer: &Module) -> Result<usize, String> {
    let ndll = dlls::normalize(dll);
    if dlls::is_builtin(&ndll) {
        let found = match want {
            Want::Name(n) => dlls::lookup(&ndll, n),
            // Les DLL intégrées exportent leurs ordinaux sous le nom « #n ».
            Want::Ordinal(o) => dlls::lookup(&ndll, &format!("#{o}")),
        };
        return Ok(match found {
            Some(a) => match want {
                Want::Name(n) => thunks::api_trace(dll, n, a),
                Want::Ordinal(o) => thunks::api_trace(dll, &format!("#{o}"), a),
            },
            None => {
                let label = match want {
                    Want::Name(n) => n.clone(),
                    Want::Ordinal(o) => format!("#{o}"),
                };
                thunks::missing_stub(dll, &label)
            }
        });
    }
    let module = match find_module_by_name(dll) {
        Some(m) => m,
        None => {
            let p = find_dll_file(dll, &importer.path).ok_or_else(|| {
                format!("DLL introuvable : {dll} (ni intégrée à Winlux, ni présente à côté du programme)")
            })?;
            load(&p, false)?
        }
    };
    let r = match want {
        Want::Name(n) => export_by_name(&module, n),
        Want::Ordinal(o) => export_by_ordinal(&module, *o as u32),
    };
    Ok(match r {
        Some(a) => a,
        None => thunks::missing_stub(
            dll,
            &match want {
                Want::Name(n) => n.clone(),
                Want::Ordinal(o) => format!("#{o}"),
            },
        ),
    })
}

pub fn find_dll_file(dll: &str, importer: &Path) -> Option<PathBuf> {
    if dll.contains(['/', '\\']) || dll.as_bytes().get(1) == Some(&b':') {
        let path = crate::paths::to_linux(dll);
        if path.is_file() {
            return Some(path);
        }
        if path.extension().is_none() {
            let path = crate::paths::to_linux(&format!("{dll}.dll"));
            if path.is_file() {
                return Some(path);
            }
        }
        return None;
    }
    let mut dirs: Vec<PathBuf> = vec![];
    if let Some(scoped) = DLL_SEARCH.with(|slot| slot.borrow().clone()) {
        dirs = scoped;
    } else {
        if let Some(d) = importer.parent() {
            dirs.push(d.to_path_buf());
        }
        if let Ok(c) = std::env::current_dir() {
            dirs.push(c);
        }
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
    }
    let want = dll.to_ascii_lowercase();
    let want_dll = if want.ends_with(".dll") {
        want.clone()
    } else {
        format!("{want}.dll")
    };
    for d in dirs {
        // Windows ignore la casse : on cherche sans en tenir compte.
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                if n == want || n == want_dll {
                    return Some(e.path());
                }
            }
        }
    }
    None
}

fn resolve_imports(m: &Module) -> Result<(), String> {
    let b = m.bytes();
    let d = m.pe.dir(DIR_IMPORT);
    if d.rva != 0 {
        let mut o = d.rva as usize;
        loop {
            let ilt = rd32(b, o).ok_or("imports tronqués")? as usize;
            let name = rd32(b, o + 12).ok_or("imports tronqués")?;
            let iat = rd32(b, o + 16).ok_or("imports tronqués")? as usize;
            if name == 0 && iat == 0 {
                break;
            }
            let dll = m.cstr_at(name).ok_or("nom de DLL illisible")?;
            fill_thunks(m, &dll, if ilt != 0 { ilt } else { iat }, iat)?;
            o = o.checked_add(20).ok_or("table des imports trop grande")?;
        }
    }
    // A linker-generated delay thunk calls the PE's __delayLoadHelper2 on first
    // use. Do not replace it or eagerly load optional DLLs: the application may
    // never need them, or may implement its own failure/fallback hooks.
    let d = m.pe.dir(DIR_DELAY_IMPORT);
    if d.rva != 0 {
        let mut o = d.rva as usize;
        let end = o
            .checked_add(d.size as usize)
            .filter(|end| *end <= m.size)
            .ok_or("répertoire d'imports différés hors de l'image")?;
        loop {
            if o.checked_add(32).is_none_or(|next| next > end) {
                return Err("descripteur d'import différé tronqué".into());
            }
            let attrs = rd32(b, o).ok_or("imports différés tronqués")?;
            let name = rd32(b, o + 4).ok_or("imports différés tronqués")?;
            let hmod = rd32(b, o + 8).ok_or("imports différés tronqués")?;
            let iat = rd32(b, o + 12).ok_or("imports différés tronqués")?;
            let int = rd32(b, o + 16).ok_or("imports différés tronqués")?;
            if name == 0 && iat == 0 && int == 0 {
                break;
            }
            if attrs & !1 != 0 {
                return Err("attributs d'import différé invalides".into());
            }
            let name = delay_rva(m, attrs, name)?;
            let hmod = delay_rva(m, attrs, hmod)?;
            let iat = delay_rva(m, attrs, iat)?;
            let int = delay_rva(m, attrs, int)?;
            let dll = m.cstr_at(name as u32).ok_or("nom de DLL illisible")?;
            if name == 0 || iat == 0 || int == 0 || dll.is_empty() {
                return Err("table d'import différé invalide".into());
            }
            if hmod != 0 && hmod.checked_add(8).is_none_or(|end| end > m.size) {
                return Err("handle d'import différé hors de l'image".into());
            }
            let bound = delay_rva(
                m,
                attrs,
                rd32(b, o + 20).ok_or("imports différés tronqués")?,
            )?;
            let unload = delay_rva(
                m,
                attrs,
                rd32(b, o + 24).ok_or("imports différés tronqués")?,
            )?;
            let mut index = 0usize;
            loop {
                let offset = index.checked_mul(8).ok_or("table différée trop grande")?;
                let source = int
                    .checked_add(offset)
                    .ok_or("table différée trop grande")?;
                let target = iat
                    .checked_add(offset)
                    .ok_or("table différée trop grande")?;
                let value = rd64(b, source).ok_or("INT différée tronquée")?;
                rd64(b, target).ok_or("IAT différée tronquée")?;
                for table in [bound, unload].into_iter().filter(|table| *table != 0) {
                    rd64(
                        b,
                        table
                            .checked_add(offset)
                            .ok_or("table différée trop grande")?,
                    )
                    .ok_or("table auxiliaire différée tronquée")?;
                }
                if value == 0 {
                    break;
                }
                if value & (1 << 63) == 0 {
                    let rva = u32::try_from(value)
                        .ok()
                        .and_then(|value| value.checked_add(2))
                        .ok_or("nom d'import différé invalide")?;
                    m.cstr_at(rva).ok_or("nom d'import différé illisible")?;
                }
                index += 1;
            }
            trace!("{dll} : {index} import(s) différé(s), thunks du programme conservés");
            o = o
                .checked_add(32)
                .ok_or("table des imports différés trop grande")?;
        }
    }
    Ok(())
}

fn delay_rva(m: &Module, attrs: u32, value: u32) -> Result<usize, String> {
    if value == 0 {
        return Ok(0);
    }
    if attrs & 1 != 0 {
        let rva = value as usize;
        if rva >= m.size {
            return Err("import différé hors de l'image".into());
        }
        Ok(rva)
    } else {
        let va = value as u64;
        let base = m.base as u64;
        if va < base || va - base >= m.size as u64 {
            return Err("adresse d'import différé invalide".into());
        }
        Ok((va - base) as usize)
    }
}

fn fill_thunks(m: &Module, dll: &str, lookup_rva: usize, iat_rva: usize) -> Result<(), String> {
    let b = m.bytes();
    let mut i: usize = 0;
    loop {
        let lo = i
            .checked_mul(8)
            .and_then(|n| lookup_rva.checked_add(n))
            .ok_or("table d'importations trop grande")?;
        let t = rd64(b, lo).ok_or("table d'imports tronquée")?;
        if t == 0 {
            break;
        }
        let want = if t & (1 << 63) != 0 {
            Want::Ordinal((t & 0xffff) as u16)
        } else {
            Want::Name(
                m.cstr_at((t & 0x7fff_ffff) as u32 + 2)
                    .ok_or("nom d'import illisible")?,
            )
        };
        let addr = resolve_one(dll, &want, m)?;
        let dst = i
            .checked_mul(8)
            .and_then(|n| iat_rva.checked_add(n))
            .ok_or("IAT trop grande")?;
        if dst.checked_add(8).is_none_or(|x| x > m.size) {
            return Err("IAT hors de l'image".into());
        }
        unsafe { *((m.base + dst) as *mut usize) = addr };
        i += 1;
    }
    trace!("{dll} : {i} import(s) reliés");
    Ok(())
}

fn exports(m: &Module) -> Option<(usize, usize, usize, usize, usize, usize, usize)> {
    let d = m.pe.dir(DIR_EXPORT);
    if d.rva == 0 {
        return None;
    }
    let b = m.bytes();
    let o = d.rva as usize;
    Some((
        rd32(b, o + 16)? as usize, // base ordinal
        rd32(b, o + 20)? as usize, // nb fonctions
        rd32(b, o + 24)? as usize, // nb noms
        rd32(b, o + 28)? as usize, // table des fonctions
        rd32(b, o + 32)? as usize, // table des noms
        rd32(b, o + 36)? as usize, // table des ordinaux
        d.size as usize,
    ))
}

fn export_addr(m: &Module, idx: usize) -> Option<usize> {
    let (_, nfun, _, funcs, _, _, _) = exports(m)?;
    if idx >= nfun {
        return None;
    }
    let rva = rd32(m.bytes(), funcs + idx * 4)? as usize;
    if rva == 0 {
        return None;
    }
    let d = m.pe.dir(DIR_EXPORT);
    if rva >= d.rva as usize && rva < (d.rva + d.size) as usize {
        // Export « transféré » : « AUTREDLL.Fonction ».
        let fwd = m.cstr_at(rva as u32)?;
        let (dll, func) = fwd.split_once('.')?;
        let want = match func.strip_prefix('#') {
            Some(n) => Want::Ordinal(n.parse().ok()?),
            None => Want::Name(func.to_string()),
        };
        return resolve_one(&format!("{dll}.dll"), &want, m).ok();
    }
    Some(m.base + rva)
}

pub fn export_by_name(m: &Module, name: &str) -> Option<usize> {
    let (_, _, nnames, _, names, ords, _) = exports(m)?;
    let b = m.bytes();
    for i in 0..nnames {
        let nrva = rd32(b, names + i * 4)?;
        if m.cstr_at(nrva).as_deref() == Some(name) {
            let idx = rd16(b, ords + i * 2)? as usize;
            return export_addr(m, idx);
        }
    }
    None
}

pub fn export_by_ordinal(m: &Module, ord: u32) -> Option<usize> {
    let (base, ..) = exports(m)?;
    export_addr(m, (ord as usize).checked_sub(base)?)
}

fn protect(m: &Module) -> Result<(), String> {
    // mmap initially creates RW pages. Record exactly the permissions applied
    // below, including gaps, rather than reporting loaded images as MEM_FREE.
    let mut pages = vec![0x04; round_up(m.size, PAGE) / PAGE];
    if (m.pe.section_alignment as usize) < PAGE {
        // Petites sections non alignées sur la page : tout reste lisible, inscriptible et exécutable.
        let result = unsafe {
            libc::mprotect(
                m.base as *mut _,
                m.size,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            )
        };
        if result != 0 {
            return Err(format!(
                "protection de {} : {}",
                m.name,
                std::io::Error::last_os_error()
            ));
        }
        pages.fill(0x40);
        crate::dlls::kernel32::register_image(m.base, pages);
        return Ok(());
    }
    unsafe {
        if libc::mprotect(
            m.base as *mut _,
            round_up(m.pe.size_of_headers as usize, PAGE).max(PAGE),
            libc::PROT_READ,
        ) != 0
        {
            return Err(format!(
                "protection des en-têtes de {} : {}",
                m.name,
                std::io::Error::last_os_error()
            ));
        }
        let headers =
            (round_up(m.pe.size_of_headers as usize, PAGE).max(PAGE) / PAGE).min(pages.len());
        pages[..headers].fill(0x02);
        for s in &m.pe.sections {
            let len = round_up(s.virtual_size.max(s.raw_size) as usize, PAGE);
            if len == 0 {
                continue;
            }
            let mut prot = 0;
            if s.characteristics & SCN_MEM_READ != 0 {
                prot |= libc::PROT_READ;
            }
            if s.characteristics & SCN_MEM_WRITE != 0 {
                prot |= libc::PROT_READ | libc::PROT_WRITE;
            }
            if s.characteristics & SCN_MEM_EXECUTE != 0 {
                prot |= libc::PROT_READ | libc::PROT_EXEC;
            }
            if libc::mprotect(
                (m.base + s.rva as usize) as *mut _,
                len.min(m.size - s.rva as usize),
                prot,
            ) != 0
            {
                return Err(format!(
                    "protection d'une section de {} : {}",
                    m.name,
                    std::io::Error::last_os_error()
                ));
            }
            let first = s.rva as usize / PAGE;
            let last = (first + len / PAGE).min(pages.len());
            let windows = match prot {
                0 => 0x01,
                libc::PROT_READ => 0x02,
                p if p == libc::PROT_READ | libc::PROT_WRITE => 0x04,
                p if p == libc::PROT_READ | libc::PROT_EXEC => 0x20,
                _ => 0x40,
            };
            pages[first..last].fill(windows);
        }
    }
    crate::dlls::kernel32::register_image(m.base, pages);
    Ok(())
}

/// Marque un module comme initialisé ; renvoie faux s'il l'était déjà.
pub fn mark_initialized(base: usize) -> bool {
    let mut mods = MODULES.lock().unwrap();
    match mods.iter_mut().find(|m| m.base == base) {
        Some(m) if !m.initialized => {
            m.initialized = true;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_dll_search_does_not_fall_back_and_restores_scope() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("winlux-dll-search-{}-{unique}", std::process::id()));
        let allowed = root.join("allowed");
        let outside = root.join("outside");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let filename = format!("winlux-search-{unique}.dll");
        let module = outside.join(&filename);
        std::fs::write(&module, b"lookup-only fixture, never executed").unwrap();
        let importer = outside.join("application.exe");
        with_dll_search(vec![allowed.clone()], || {
            assert!(find_dll_file(&filename, &importer).is_none());
            with_dll_search(vec![outside.clone()], || {
                assert_eq!(
                    find_dll_file(&filename.to_uppercase(), &importer),
                    Some(module.clone())
                );
            });
            assert!(find_dll_file(&filename, &importer).is_none());
            let (name, importer) = (filename.clone(), importer.clone());
            assert_eq!(
                std::thread::spawn(move || find_dll_file(&name, &importer))
                    .join()
                    .unwrap(),
                Some(module.clone())
            );
        });
        assert_eq!(find_dll_file(&filename, &importer), Some(module.clone()));
        let absolute = crate::paths::to_windows(&module);
        assert_eq!(find_dll_file(&absolute, &importer), Some(module));
        assert!(
            find_dll_file(&format!("Z:\\missing-winlux-folder\\{filename}"), &importer).is_none()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    fn put32(b: &mut [u8], o: usize, v: u32) {
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn resout_un_export_par_nom_et_par_ordinal() {
        let mut image = vec![0u8; 0x400];
        // IMAGE_EXPORT_DIRECTORY à 0x100 : base ordinal 7, une fonction,
        // un nom, puis les trois tables aux RVAs 0x180/0x190/0x1a0.
        put32(&mut image, 0x110, 7);
        put32(&mut image, 0x114, 1);
        put32(&mut image, 0x118, 1);
        put32(&mut image, 0x11c, 0x180);
        put32(&mut image, 0x120, 0x190);
        put32(&mut image, 0x124, 0x1a0);
        put32(&mut image, 0x180, 0x250);
        put32(&mut image, 0x190, 0x1b0);
        image[0x1b0..0x1b0 + 5].copy_from_slice(b"demo\0");
        image[0x1a0..0x1a2].copy_from_slice(&0u16.to_le_bytes());
        image[0x250] = 0xc3;
        let mut dirs = vec![DataDir::default(); 16];
        dirs[DIR_EXPORT] = DataDir {
            rva: 0x100,
            size: 0x80,
        };
        let m = Module {
            name: "demo.dll".into(),
            path: "demo.dll".into(),
            base: image.as_ptr() as usize,
            size: image.len(),
            pe: PeImage {
                characteristics: 0,
                entry_rva: 0,
                image_base: 0,
                section_alignment: 0x1000,
                size_of_image: image.len() as u32,
                size_of_headers: 0,
                subsystem: 3,
                dll_characteristics: 0,
                stack_reserve: 0,
                stack_commit: 0,
                dirs,
                sections: vec![],
                il_only32: false,
            },
            tls: None,
            initialized: false,
            refcount: 1,
        };
        assert_eq!(export_by_name(&m, "demo"), Some(m.base + 0x250));
        assert_eq!(export_by_ordinal(&m, 7), Some(m.base + 0x250));
        assert_eq!(export_by_ordinal(&m, 6), None);
    }

    #[test]
    fn valide_les_references_dimports_differees() {
        let m = Module {
            name: "x".into(),
            path: "x".into(),
            base: 0x1000,
            size: 0x1000,
            pe: PeImage {
                characteristics: 0,
                entry_rva: 0,
                image_base: 0,
                section_alignment: 0,
                size_of_image: 0,
                size_of_headers: 0,
                subsystem: 0,
                dll_characteristics: 0,
                stack_reserve: 0,
                stack_commit: 0,
                dirs: vec![DataDir::default(); 16],
                sections: vec![],
                il_only32: false,
            },
            tls: None,
            initialized: false,
            refcount: 1,
        };
        assert_eq!(delay_rva(&m, 1, 0x120).unwrap(), 0x120);
        assert_eq!(delay_rva(&m, 0, 0x1100).unwrap(), 0x100);
        assert!(delay_rva(&m, 1, 0x2000).is_err());
    }

    #[test]
    fn preserve_le_thunk_differe_sans_charger_une_dll_optionnelle_absente() {
        let mut image = vec![0u8; 0x400];
        put32(&mut image, 0x100, 1); // grAttrs : tous les champs sont des RVA
        put32(&mut image, 0x104, 0x180); // nom de DLL
        put32(&mut image, 0x108, 0x1c0); // HMODULE
        put32(&mut image, 0x10c, 0x1a0); // IAT
        put32(&mut image, 0x110, 0x1b0); // INT
        image[0x180..0x193].copy_from_slice(b"winlux_missing.dll\0");
        put32(&mut image, 0x1b0, 0x1d0); // hint/name RVA
        image[0x1d0..0x1d2].copy_from_slice(&0u16.to_le_bytes());
        image[0x1d2..0x1df].copy_from_slice(b"GetTickCount\0");
        let mut dirs = vec![DataDir::default(); 16];
        dirs[DIR_DELAY_IMPORT] = DataDir {
            rva: 0x100,
            size: 0x40,
        };
        let thunk = image.as_ptr() as usize + 0x250;
        image[0x1a0..0x1a8].copy_from_slice(&(thunk as u64).to_le_bytes());
        let m = Module {
            name: "delay.exe".into(),
            path: "delay.exe".into(),
            base: image.as_ptr() as usize,
            size: image.len(),
            pe: PeImage {
                characteristics: 0,
                entry_rva: 0,
                image_base: 0,
                section_alignment: 0x1000,
                size_of_image: image.len() as u32,
                size_of_headers: 0,
                subsystem: 3,
                dll_characteristics: 0,
                stack_reserve: 0,
                stack_commit: 0,
                dirs,
                sections: vec![],
                il_only32: false,
            },
            tls: None,
            initialized: false,
            refcount: 1,
        };
        resolve_imports(&m).unwrap();
        assert_eq!(unsafe { *(m.base as *const usize).add(0x1a0 / 8) }, thunk);
        assert_eq!(unsafe { *(m.base as *const usize).add(0x1c0 / 8) }, 0);
    }
}
