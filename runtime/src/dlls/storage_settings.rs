//! Réglages d'application Windows Runtime (#2194) :
//! `ApplicationData.LocalSettings` et `RoamingSettings` rendent un
//! `Windows.Storage.ApplicationDataContainer`, dont `Values` est un
//! ensemble de propriétés (`ApplicationDataContainerSettings`, IPropertySet)
//! gardé d'un lancement à l'autre.
//!
//! Ce que dit la documentation publique de Microsoft (ApplicationData,
//! ApplicationDataContainer, ApplicationDataCreateDisposition,
//! ApplicationDataLocality, « Store and retrieve settings and other app
//! data ») :
//! - `Values` porte les réglages du conteneur ; un réglage est une valeur
//!   de type de base Windows Runtime ;
//! - `CreateContainer` avec `Always` crée le conteneur s'il manque ;
//!   avec `Existing`, il ne fait que l'ouvrir ;
//! - un nom de conteneur a au plus 255 caractères, et les conteneurs
//!   s'imbriquent sur 32 niveaux au plus ;
//! - `Locality` vaut Local (0) ou Roaming (1).
//!
//! Choix de Winlux là où la documentation se tait (inférences nommées) :
//! - les réglages d'un paquet sont enregistrés dans
//!   `%LOCALAPPDATA%\Packages\<famille>\Settings`, un fichier texte par
//!   localité (`winlux-local.txt`, `winlux-roaming.txt`), en 0600 dans un
//!   dossier 0700, réécrit entier à chaque changement ;
//! - Winlux ne synchronise rien : les réglages itinérants restent sur la
//!   machine, dans leur propre fichier ;
//! - le conteneur racine a un nom vide ;
//! - `CreateContainer(Existing)` d'un conteneur absent rend
//!   HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND), comme `Values` d'un
//!   conteneur supprimé ; `DeleteContainer` d'un conteneur absent réussit ;
//! - seules les valeurs emballées par Winlux (`PropertyValue`) sont
//!   enregistrées ; une autre valeur (tableau, `ApplicationDataCompositeValue`,
//!   objet de l'application) reste en mémoire pour ce lancement, et le
//!   journal le dit une fois, sans nommer le réglage ;
//! - `Containers` n'est pas encore servi (E_NOTIMPL, journalisé une fois).
//!
//! Sources : la documentation publique citée ; identifiants d'interface et
//! ordre des méthodes tels que les publie windows-rs 0.58 (licence MIT,
//! `src/Windows/Storage/mod.rs`, `src/Windows/Foundation/mod.rs`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
/// HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND).
pub(crate) const E_FILE_NOT_FOUND: i32 = 0x8007_0002u32 as i32;

const fn g(d1: u32, d2: u16, d3: u16, d4: u64) -> Guid {
    guid(d1, d2, d3, d4.to_be_bytes())
}

const IID_IINSPECTABLE: Guid = g(0xAF86_E2E0, 0xB12D, 0x4C6A, 0x9C5A_D7AA_6510_1E90);
const IID_IAGILEOBJECT: Guid = g(0x94EA_2B94, 0xE9CC, 0x49E0, 0xC0FF_EE64_CA8F_5B90);
pub(crate) const IID_CONTAINER: Guid = g(0xc5ae_fd1e, 0xf467, 0x40ba, 0x8566_ab64_0a44_1e1d);
const IID_CLOSABLE: Guid = g(0x30d5_a829, 0x7fa4, 0x4026, 0x83bb_d75b_ae4e_a99e);

const CLASS: &str = "Windows.Storage.ApplicationDataContainer";
const SETTINGS_CLASS: &str = "Windows.Storage.ApplicationDataContainerSettings";

/// ApplicationDataCreateDisposition.
const DISPOSITION_ALWAYS: i32 = 0;
const DISPOSITION_EXISTING: i32 = 1;
const MAX_NAME: usize = 255;
const MAX_DEPTH: usize = 32;

const FILE_HEADER: &str = "winlux-reglages 1";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Une ligne de journal par sujet.
fn warn_once(what: &str) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let mut seen = lock(&SEEN);
    if !seen.iter().any(|m| m == what) {
        seen.push(what.to_string());
        crate::log_warn!("com", "ApplicationDataContainer : {what}");
    }
}

unsafe fn hstr_in(h: usize) -> Vec<u16> {
    super::electron_win32::hstring_slice(h).to_vec()
}

unsafe fn hstr_out(units: &[u16], out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    super::electron_win32::WindowsCreateString(units.as_ptr(), units.len() as u32, out)
}

// ============================================================
//  Fichier des réglages
// ============================================================

/// Chemin d'un conteneur : les noms depuis la racine (vide).
type ContainerPath = Vec<Vec<u16>>;

fn hex(units: &[u16]) -> String {
    if units.is_empty() {
        return "-".into();
    }
    units.iter().map(|u| format!("{u:04x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u16>> {
    if s == "-" {
        return Some(Vec::new());
    }
    if !s.len().is_multiple_of(4) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(4)
        .map(|i| u16::from_str_radix(&s[i..i + 4], 16).ok())
        .collect()
}

fn path_text(path: &ContainerPath) -> String {
    if path.is_empty() {
        return "/".into();
    }
    path.iter().map(|n| hex(n)).collect::<Vec<_>>().join("/")
}

fn path_of_text(s: &str) -> Option<ContainerPath> {
    if s == "/" {
        return Some(Vec::new());
    }
    s.split('/')
        .map(|p| unhex(p).filter(|n| !n.is_empty()))
        .collect()
}

/// Réglages d'une localité d'un paquet : un ensemble de propriétés par
/// conteneur, retenu par le magasin.
pub(crate) struct Store {
    file: PathBuf,
    locality: i32,
    sets: Mutex<BTreeMap<ContainerPath, usize>>,
}

impl Store {
    /// Magasin relu depuis `file` (absent : seulement la racine). Les
    /// lignes illisibles sont ignorées.
    pub(crate) fn load(file: &Path, locality: i32) -> &'static Store {
        let store: &'static Store = Box::leak(Box::new(Store {
            file: file.to_path_buf(),
            locality,
            sets: Mutex::new(BTreeMap::new()),
        }));
        store.ensure(&Vec::new());
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let mut lines = text.lines();
        if lines.next() != Some(FILE_HEADER) {
            return store;
        }
        for line in lines {
            let mut f = line.splitn(4, ' ');
            match (f.next(), f.next(), f.next(), f.next()) {
                (Some("c"), Some(p), None, None) => {
                    if let Some(p) = path_of_text(p).filter(|p| p.len() <= MAX_DEPTH) {
                        store.ensure(&p);
                    }
                }
                (Some("v"), Some(p), Some(k), Some(v)) => {
                    let (Some(p), Some(k)) = (path_of_text(p), unhex(k)) else {
                        continue;
                    };
                    let Some(value) = super::property_value::decode(v) else {
                        continue;
                    };
                    let set = store.ensure(&p);
                    unsafe {
                        super::winrt_coreapp::set_put_quiet(
                            set,
                            String::from_utf16_lossy(&k),
                            value,
                        )
                    };
                }
                _ => {}
            }
        }
        store
    }

    /// Ensemble du conteneur `path`, créé s'il manque (sans enregistrer).
    fn ensure(&'static self, path: &ContainerPath) -> usize {
        let mut sets = lock(&self.sets);
        if let Some(&s) = sets.get(path) {
            return s;
        }
        let s =
            super::winrt_coreapp::new_set_with(SETTINGS_CLASS, Some(Box::new(move || self.save())));
        sets.insert(path.clone(), s);
        s
    }

    fn exists(&self, path: &ContainerPath) -> bool {
        lock(&self.sets).contains_key(path)
    }

    /// Ensemble du conteneur, retenu une fois de plus.
    fn values(&self, path: &ContainerPath) -> Option<usize> {
        let s = *lock(&self.sets).get(path)?;
        unsafe { super::winrt_coreapp::set_add_ref_ptr(s) };
        Some(s)
    }

    /// Supprime le conteneur et ceux qu'il contient.
    fn remove(&self, path: &ContainerPath) {
        let gone: Vec<usize> = {
            let mut sets = lock(&self.sets);
            let keys: Vec<ContainerPath> = sets
                .keys()
                .filter(|k| k.len() >= path.len() && k[..path.len()] == path[..])
                .cloned()
                .collect();
            keys.iter().filter_map(|k| sets.remove(k)).collect()
        };
        for s in gone {
            unsafe { super::winrt_coreapp::set_release_ptr(s) };
        }
    }

    fn text(&self) -> String {
        let sets: Vec<(ContainerPath, usize)> = lock(&self.sets)
            .iter()
            .map(|(p, &s)| (p.clone(), s))
            .collect();
        let mut out = format!("{FILE_HEADER}\n");
        let mut skipped = false;
        for (path, set) in sets {
            let p = path_text(&path);
            if !path.is_empty() {
                out.push_str(&format!("c {p}\n"));
            }
            for (key, value) in unsafe { super::winrt_coreapp::set_entries(set) } {
                let k: Vec<u16> = key.encode_utf16().collect();
                match unsafe { super::property_value::encode(value) } {
                    Some(v) => out.push_str(&format!("v {p} {} {v}\n", hex(&k))),
                    None => skipped = true,
                }
                unsafe { release(value) };
            }
        }
        if skipped {
            warn_once("une valeur qui n'est pas de type de base n'est pas enregistrée");
        }
        out
    }

    /// Réécrit le fichier entier : fichier temporaire 0600, puis rename.
    fn save(&self) {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        // Deux fils qui changent des réglages n'écrivent pas le même
        // fichier temporaire en même temps.
        static SAVING: Mutex<()> = Mutex::new(());
        let _saving = lock(&SAVING);
        let text = self.text();
        let tmp = self.file.with_extension("tmp");
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .and_then(|mut f| f.write_all(text.as_bytes()))
            .and_then(|_| std::fs::rename(&tmp, &self.file));
        if let Err(e) = written {
            warn_once(&format!("réglages non enregistrés : {e}"));
        }
    }
}

unsafe fn release(obj: usize) {
    if obj != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 =
            std::mem::transmute(*(*(obj as *const *const usize)).add(2));
        f(obj);
    }
}

/// Magasin d'un fichier, relu une seule fois par processus.
fn store_for(file: &Path, locality: i32) -> &'static Store {
    static STORES: OnceLock<Mutex<Vec<&'static Store>>> = OnceLock::new();
    let mut stores = lock(STORES.get_or_init(|| Mutex::new(Vec::new())));
    if let Some(s) = stores.iter().find(|s| s.file == file) {
        return s;
    }
    let s = Store::load(file, locality);
    stores.push(s);
    s
}

/// Conteneur racine des réglages enregistrés dans `file` (une référence).
pub(crate) fn root_container(file: &Path, locality: i32) -> usize {
    new_container(store_for(file, locality), Vec::new())
}

// ============================================================
//  ApplicationDataContainer : IApplicationDataContainer et IClosable
// ============================================================

#[repr(C)]
struct Container {
    vt: &'static VTable<12>,
    vt_close: &'static VTable<7>,
    refs: AtomicU32,
    store: &'static Store,
    path: ContainerPath,
    closed: AtomicBool,
}

fn new_container(store: &'static Store, path: ContainerPath) -> usize {
    Box::into_raw(Box::new(Container {
        vt: &CONTAINER_VTBL,
        vt_close: &CLOSABLE_VTBL,
        refs: AtomicU32::new(1),
        store,
        path,
        closed: AtomicBool::new(false),
    })) as usize
}

unsafe fn cont<'a>(base: usize) -> &'a Container {
    &*(base as *const Container)
}

unsafe fn cont_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = if iid == IID_CLOSABLE {
        base + 8
    } else if [
        IID_CONTAINER,
        IID_IUNKNOWN,
        IID_IINSPECTABLE,
        IID_IAGILEOBJECT,
    ]
    .contains(&iid)
    {
        base
    } else {
        super::comobj::log_no_interface("ApplicationDataContainer", &iid as *const Guid);
        0
    };
    if *out == 0 {
        return E_NOINTERFACE;
    }
    cont(base).refs.fetch_add(1, Ordering::SeqCst);
    S_OK
}
unsafe fn cont_release(base: usize) -> u32 {
    let n = cont(base).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(base as *mut Container));
    }
    n
}

unsafe extern "win64" fn c_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    cont_qi(this, iid, out)
}
unsafe extern "win64" fn c_add_ref(this: usize) -> u32 {
    cont(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn c_release(this: usize) -> u32 {
    cont_release(this)
}
unsafe extern "win64" fn x_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    cont_qi(this - 8, iid, out)
}
unsafe extern "win64" fn x_add_ref(this: usize) -> u32 {
    cont(this - 8).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn x_release(this: usize) -> u32 {
    cont_release(this - 8)
}

unsafe extern "win64" fn get_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}
unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}
unsafe extern "win64" fn class_name(_this: usize, out: *mut usize) -> i32 {
    let w: Vec<u16> = CLASS.encode_utf16().collect();
    hstr_out(&w, out)
}

unsafe extern "win64" fn c_name(this: usize, out: *mut usize) -> i32 {
    let empty = Vec::new();
    hstr_out(cont(this).path.last().unwrap_or(&empty), out)
}
unsafe extern "win64" fn c_locality(this: usize, out: *mut i32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = cont(this).store.locality;
    S_OK
}
unsafe extern "win64" fn c_values(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let c = cont(this);
    match c.store.values(&c.path) {
        Some(s) => {
            *out = s;
            S_OK
        }
        None => {
            *out = 0;
            E_FILE_NOT_FOUND
        }
    }
}
unsafe extern "win64" fn c_containers(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    warn_once("Containers pas encore servie (E_NOTIMPL)");
    E_NOTIMPL
}

/// Chemin du sous-conteneur `name`, ou E_INVALIDARG.
unsafe fn child_path(this: usize, name: usize) -> Result<ContainerPath, i32> {
    let name = hstr_in(name);
    let c = cont(this);
    if name.is_empty() || name.len() > MAX_NAME || c.path.len() >= MAX_DEPTH {
        return Err(E_INVALIDARG);
    }
    let mut p = c.path.clone();
    p.push(name);
    Ok(p)
}

unsafe extern "win64" fn c_create_container(
    this: usize,
    name: usize,
    disposition: i32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let path = match child_path(this, name) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    let store = cont(this).store;
    if !store.exists(&path) {
        match disposition {
            DISPOSITION_ALWAYS => {
                store.ensure(&path);
                store.save();
            }
            DISPOSITION_EXISTING => return E_FILE_NOT_FOUND,
            _ => return E_INVALIDARG,
        }
    } else if disposition != DISPOSITION_ALWAYS && disposition != DISPOSITION_EXISTING {
        return E_INVALIDARG;
    }
    *out = new_container(store, path);
    S_OK
}
unsafe extern "win64" fn c_delete_container(this: usize, name: usize) -> i32 {
    let path = match child_path(this, name) {
        Ok(p) => p,
        Err(hr) => return hr,
    };
    let store = cont(this).store;
    if store.exists(&path) {
        store.remove(&path);
        store.save();
    }
    S_OK
}
unsafe extern "win64" fn x_close(this: usize) -> i32 {
    cont(this - 8).closed.store(true, Ordering::SeqCst);
    S_OK
}

static CONTAINER_VTBL: VTable<12> = VTable([
    c_qi as *const (),
    c_add_ref as *const (),
    c_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    c_name as *const (),
    c_locality as *const (),
    c_values as *const (),
    c_containers as *const (),
    c_create_container as *const (),
    c_delete_container as *const (),
]);

static CLOSABLE_VTBL: VTable<7> = VTable([
    x_qi as *const (),
    x_add_ref as *const (),
    x_release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    x_close as *const (),
]);

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn hs(s: &str) -> usize {
        let w: Vec<u16> = s.encode_utf16().collect();
        let mut h = 0usize;
        unsafe {
            super::super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, &mut h)
        };
        h
    }

    unsafe fn call<F: Copy>(obj: usize, slot: usize) -> F {
        std::mem::transmute_copy(&*(*(obj as *const *const usize)).add(slot))
    }

    /// Insert(clé, valeur) par l'interface IMap<String, Object> de l'ensemble.
    unsafe fn insert(set: usize, key: &str, value: usize) {
        let mut map = 0usize;
        let qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 = call(set, 0);
        assert_eq!(
            qi(
                set,
                super::super::winrt_coreapp::map_iid().as_ptr(),
                &mut map
            ),
            S_OK
        );
        let ins: unsafe extern "win64" fn(usize, usize, usize, *mut u8) -> i32 = call(map, 10);
        let mut replaced = 0u8;
        assert_eq!(ins(map, hs(key), value, &mut replaced), S_OK);
        release(value);
        release(map);
    }

    /// Réglages relus du fichier, par conteneur : (chemin, clé, valeur).
    fn reread(file: &Path) -> Vec<(String, String, String)> {
        let store = Store::load(file, 0);
        let mut out = Vec::new();
        for (path, set) in lock(&store.sets).iter() {
            for (k, v) in unsafe { super::super::winrt_coreapp::set_entries(*set) } {
                let name = path
                    .iter()
                    .map(|n| String::from_utf16_lossy(n))
                    .collect::<Vec<_>>()
                    .join("\\");
                out.push((
                    name,
                    k,
                    unsafe { super::super::property_value::encode(v) }.unwrap(),
                ));
                unsafe { release(v) };
            }
        }
        out
    }

    #[test]
    fn reglages_locaux_gardes_d_un_lancement_a_l_autre_2194() {
        let dir = std::env::temp_dir().join(format!("winlux-reglages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("winlux-local.txt");
        unsafe {
            let root = root_container(&file, 0);
            let mut loc = 9i32;
            assert_eq!(c_locality(root, &mut loc), S_OK);
            assert_eq!(loc, 0);
            let mut name = 1usize;
            assert_eq!(c_name(root, &mut name), S_OK);
            assert!(super::super::electron_win32::hstring_slice(name).is_empty());

            // Values : un ensemble de classe ApplicationDataContainerSettings.
            let mut values = 0usize;
            assert_eq!(c_values(root, &mut values), S_OK);
            let cls: unsafe extern "win64" fn(usize, *mut usize) -> i32 = call(values, 4);
            let mut h = 0usize;
            assert_eq!(cls(values, &mut h), S_OK);
            assert_eq!(
                String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h)),
                SETTINGS_CLASS
            );
            insert(
                values,
                "nombre",
                super::super::property_value::decode("4 42").unwrap(),
            );
            let ete: Vec<u16> = "été".encode_utf16().collect();
            insert(
                values,
                "texte",
                super::super::property_value::decode(&format!("12 {}", hex(&ete))).unwrap(),
            );
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                0o600
            );

            // Sous-conteneur : Existing ne crée pas, Always crée.
            let mut sub = 0usize;
            assert_eq!(
                c_create_container(root, hs("Sous"), DISPOSITION_EXISTING, &mut sub),
                E_FILE_NOT_FOUND
            );
            assert_eq!(sub, 0);
            assert_eq!(
                c_create_container(root, hs(""), DISPOSITION_ALWAYS, &mut sub),
                E_INVALIDARG
            );
            assert_eq!(
                c_create_container(root, hs("Sous"), DISPOSITION_ALWAYS, &mut sub),
                S_OK
            );
            let mut sub_name = 0usize;
            assert_eq!(c_name(sub, &mut sub_name), S_OK);
            assert_eq!(
                String::from_utf16_lossy(super::super::electron_win32::hstring_slice(sub_name)),
                "Sous"
            );
            let mut sub_values = 0usize;
            assert_eq!(c_values(sub, &mut sub_values), S_OK);
            insert(
                sub_values,
                "vrai",
                super::super::property_value::decode("11 1").unwrap(),
            );

            // Un nouveau lancement relit les mêmes réglages.
            let mut lu = reread(&file);
            lu.sort();
            assert_eq!(
                lu,
                vec![
                    (String::new(), "nombre".into(), "4 42".into()),
                    (String::new(), "texte".into(), format!("12 {}", hex(&ete))),
                    ("Sous".into(), "vrai".into(), "11 1".into()),
                ]
            );

            // Supprimé, le conteneur disparaît du fichier ; ses Values ne
            // sont plus servies.
            assert_eq!(c_delete_container(root, hs("Sous")), S_OK);
            assert_eq!(c_delete_container(root, hs("Sous")), S_OK);
            let mut gone = 1usize;
            assert_eq!(c_values(sub, &mut gone), E_FILE_NOT_FOUND);
            assert_eq!(reread(&file).len(), 2);

            // IClosable sur le même objet.
            let mut closable = 0usize;
            assert_eq!(c_qi(root, IID_CLOSABLE.as_ptr(), &mut closable), S_OK);
            assert_eq!(closable, root + 8);
            assert_eq!(x_close(closable), S_OK);
            x_release(closable);
            for o in [sub_values, sub, values, root] {
                release(o);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fichier_illisible_ignore_2194() {
        let dir = std::env::temp_dir().join(format!("winlux-reglages-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("winlux-local.txt");
        std::fs::write(
            &file,
            format!("{FILE_HEADER}\nv / zz 4 1\nv / 0061 99 1\nc ../x\nv / 0062 4 7\n"),
        )
        .unwrap();
        assert_eq!(
            reread(&file),
            vec![(String::new(), "b".into(), "4 7".into())]
        );
        std::fs::write(&file, "autre chose\nv / 0062 4 7\n").unwrap();
        assert!(reread(&file).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
