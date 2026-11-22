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
//! - `Containers` rend une vue figée des sous-conteneurs au moment de
//!   l'appel (IMapView et son parcours, #2962) ; un conteneur supprimé
//!   ensuite reste dans la vue, et son `Values` rend alors
//!   HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND).
//!
//! Sources : la documentation publique citée ; identifiants d'interface et
//! ordre des méthodes tels que les publie windows-rs 0.58 (licence MIT,
//! `src/Windows/Storage/mod.rs`, `src/Windows/Foundation/mod.rs`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use super::comobj::{
    guid, pinterface_iid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_BOUNDS: i32 = 0x8000_000Bu32 as i32;
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

    /// Noms des sous-conteneurs directs de `path`, dans l'ordre des noms.
    fn children(&self, path: &ContainerPath) -> Vec<Vec<u16>> {
        lock(&self.sets)
            .keys()
            .filter(|k| k.len() == path.len() + 1 && k[..path.len()] == path[..])
            .map(|k| k[path.len()].clone())
            .collect()
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
/// Containers : vue figée des sous-conteneurs (#2962).
unsafe extern "win64" fn c_containers(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let c = cont(this);
    *out = new_kids(c.store, c.path.clone());
    S_OK
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

// ============================================================
//  Containers : IMapView<String, ApplicationDataContainer>, son parcours
//  (IIterable, IIterator) et ses paires (IKeyValuePair) (#2962)
// ============================================================

/// Interfaces paramétrées de la vue des sous-conteneurs.
struct KidsIids {
    view: Guid,
    iterable: Guid,
    iterator: Guid,
    pair: Guid,
}

fn kids_iids() -> &'static KidsIids {
    static I: OnceLock<KidsIids> = OnceLock::new();
    I.get_or_init(|| {
        let rc =
            "rc(Windows.Storage.ApplicationDataContainer;{c5aefd1e-f467-40ba-8566-ab640a441e1d})";
        let pair = format!("pinterface({{02b51929-c1c4-4a7e-8940-0312b5c18500}};string;{rc})");
        KidsIids {
            view: pinterface_iid(&format!(
                "pinterface({{e480ce40-a338-4ada-adcf-272272e48cb9}};string;{rc})"
            )),
            iterable: pinterface_iid(&format!(
                "pinterface({{faa585ea-6214-4217-afda-7f46de5869b3}};{pair})"
            )),
            iterator: pinterface_iid(&format!(
                "pinterface({{6a79e863-4300-459a-9966-cbb660963ee1}};{pair})"
            )),
            pair: pinterface_iid(&pair),
        }
    })
}

const BASE_IIDS: [Guid; 3] = [IID_IUNKNOWN, IID_IINSPECTABLE, IID_IAGILEOBJECT];

unsafe fn hstr_class(name: &str, out: *mut usize) -> i32 {
    let w: Vec<u16> = name.encode_utf16().collect();
    hstr_out(&w, out)
}

/// Vue : IMapView (entrée 0) et IIterable (entrée 8).
#[repr(C)]
struct Kids {
    vt: &'static VTable<10>,
    vt_iter: &'static VTable<7>,
    refs: AtomicU32,
    store: &'static Store,
    parent: ContainerPath,
    names: Vec<Vec<u16>>,
}

fn new_kids(store: &'static Store, parent: ContainerPath) -> usize {
    let names = store.children(&parent);
    Box::into_raw(Box::new(Kids {
        vt: &KIDS_VTBL,
        vt_iter: &KIDS_ITERABLE_VTBL,
        refs: AtomicU32::new(1),
        store,
        parent,
        names,
    })) as usize
}

unsafe fn kids<'a>(base: usize) -> &'a Kids {
    &*(base as *const Kids)
}

impl Kids {
    fn child(&self, name: &[u16]) -> usize {
        let mut p = self.parent.clone();
        p.push(name.to_vec());
        new_container(self.store, p)
    }
}

unsafe fn kids_qi(base: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = if iid == kids_iids().iterable {
        base + 8
    } else if iid == kids_iids().view || BASE_IIDS.contains(&iid) {
        base
    } else {
        super::comobj::log_no_interface("ApplicationDataContainer.Containers", &iid as *const Guid);
        0
    };
    if *out == 0 {
        return E_NOINTERFACE;
    }
    kids(base).refs.fetch_add(1, Ordering::SeqCst);
    S_OK
}
unsafe fn kids_release(base: usize) -> u32 {
    let n = kids(base).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(base as *mut Kids));
    }
    n
}
unsafe extern "win64" fn k_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    kids_qi(this, iid, out)
}
unsafe extern "win64" fn k_add_ref(this: usize) -> u32 {
    kids(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn k_release(this: usize) -> u32 {
    kids_release(this)
}
unsafe extern "win64" fn ki_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    kids_qi(this - 8, iid, out)
}
unsafe extern "win64" fn ki_add_ref(this: usize) -> u32 {
    kids(this - 8).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn ki_release(this: usize) -> u32 {
    kids_release(this - 8)
}
unsafe extern "win64" fn k_class(_this: usize, out: *mut usize) -> i32 {
    hstr_class(
        "Windows.Foundation.Collections.IMapView`2<String, Windows.Storage.ApplicationDataContainer>",
        out,
    )
}
unsafe extern "win64" fn k_lookup(this: usize, key: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let k = kids(this);
    let name = hstr_in(key);
    if k.names.contains(&name) {
        *out = k.child(&name);
        S_OK
    } else {
        *out = 0;
        E_BOUNDS
    }
}
unsafe extern "win64" fn k_size(this: usize, out: *mut u32) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = kids(this).names.len() as u32;
    S_OK
}
unsafe extern "win64" fn k_has_key(this: usize, key: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = kids(this).names.contains(&hstr_in(key)) as u8;
    S_OK
}
/// Split : une vue qui ne se coupe pas rend deux nuls (Learn, IMapView.Split).
unsafe extern "win64" fn k_split(_this: usize, first: *mut usize, second: *mut usize) -> i32 {
    if first.is_null() || second.is_null() {
        return E_POINTER;
    }
    *first = 0;
    *second = 0;
    S_OK
}
unsafe extern "win64" fn ki_first(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let base = this - 8;
    kids(base).refs.fetch_add(1, Ordering::SeqCst);
    *out = Box::into_raw(Box::new(KidsIter {
        vt: &KIDS_ITERATOR_VTBL,
        refs: AtomicU32::new(1),
        view: base,
        pos: AtomicU32::new(0),
    })) as usize;
    S_OK
}

static KIDS_VTBL: VTable<10> = VTable([
    k_qi as *const (),
    k_add_ref as *const (),
    k_release as *const (),
    get_iids as *const (),
    k_class as *const (),
    trust_level as *const (),
    k_lookup as *const (),
    k_size as *const (),
    k_has_key as *const (),
    k_split as *const (),
]);
static KIDS_ITERABLE_VTBL: VTable<7> = VTable([
    ki_qi as *const (),
    ki_add_ref as *const (),
    ki_release as *const (),
    get_iids as *const (),
    k_class as *const (),
    trust_level as *const (),
    ki_first as *const (),
]);

/// Parcours de la vue : il garde la vue et la position.
#[repr(C)]
struct KidsIter {
    vt: &'static VTable<10>,
    refs: AtomicU32,
    view: usize,
    pos: AtomicU32,
}

unsafe fn kiter<'a>(this: usize) -> &'a KidsIter {
    &*(this as *const KidsIter)
}
unsafe extern "win64" fn it_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if iid != kids_iids().iterator && !BASE_IIDS.contains(&iid) {
        super::comobj::log_no_interface(
            "ApplicationDataContainer.Containers (parcours)",
            &iid as *const Guid,
        );
        *out = 0;
        return E_NOINTERFACE;
    }
    kiter(this).refs.fetch_add(1, Ordering::SeqCst);
    *out = this;
    S_OK
}
unsafe extern "win64" fn it_add_ref(this: usize) -> u32 {
    kiter(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn it_release(this: usize) -> u32 {
    let n = kiter(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        let it = Box::from_raw(this as *mut KidsIter);
        kids_release(it.view);
    }
    n
}
unsafe extern "win64" fn it_class(_this: usize, out: *mut usize) -> i32 {
    hstr_class(
        "Windows.Foundation.Collections.IIterator`1<Windows.Foundation.Collections.IKeyValuePair`2<String, Windows.Storage.ApplicationDataContainer>>",
        out,
    )
}
unsafe fn it_pair(it: &KidsIter, i: usize) -> Option<usize> {
    let k = kids(it.view);
    let name = k.names.get(i)?;
    let mut p = k.parent.clone();
    p.push(name.clone());
    Some(Box::into_raw(Box::new(KidsPair {
        vt: &KIDS_PAIR_VTBL,
        refs: AtomicU32::new(1),
        store: k.store,
        path: p,
    })) as usize)
}
unsafe extern "win64" fn it_current(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let it = kiter(this);
    match it_pair(it, it.pos.load(Ordering::SeqCst) as usize) {
        Some(p) => {
            *out = p;
            S_OK
        }
        None => {
            *out = 0;
            E_BOUNDS
        }
    }
}
unsafe extern "win64" fn it_has_current(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let it = kiter(this);
    *out = ((it.pos.load(Ordering::SeqCst) as usize) < kids(it.view).names.len()) as u8;
    S_OK
}
unsafe extern "win64" fn it_move_next(this: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let it = kiter(this);
    let len = kids(it.view).names.len();
    let pos = it.pos.load(Ordering::SeqCst) as usize;
    if pos >= len {
        *out = 0;
        return E_BOUNDS;
    }
    it.pos.store((pos + 1) as u32, Ordering::SeqCst);
    *out = (pos + 1 < len) as u8;
    S_OK
}
unsafe extern "win64" fn it_get_many(
    this: usize,
    capacity: u32,
    items: *mut usize,
    actual: *mut u32,
) -> i32 {
    if actual.is_null() || (items.is_null() && capacity > 0) {
        return E_POINTER;
    }
    let it = kiter(this);
    let mut n = 0u32;
    while n < capacity {
        let pos = it.pos.load(Ordering::SeqCst) as usize;
        let Some(p) = it_pair(it, pos) else {
            break;
        };
        *items.add(n as usize) = p;
        it.pos.store((pos + 1) as u32, Ordering::SeqCst);
        n += 1;
    }
    *actual = n;
    S_OK
}

static KIDS_ITERATOR_VTBL: VTable<10> = VTable([
    it_qi as *const (),
    it_add_ref as *const (),
    it_release as *const (),
    get_iids as *const (),
    it_class as *const (),
    trust_level as *const (),
    it_current as *const (),
    it_has_current as *const (),
    it_move_next as *const (),
    it_get_many as *const (),
]);

/// Paire (nom, sous-conteneur).
#[repr(C)]
struct KidsPair {
    vt: &'static VTable<8>,
    refs: AtomicU32,
    store: &'static Store,
    path: ContainerPath,
}

unsafe fn kpair<'a>(this: usize) -> &'a KidsPair {
    &*(this as *const KidsPair)
}
unsafe extern "win64" fn pr_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    if iid != kids_iids().pair && !BASE_IIDS.contains(&iid) {
        super::comobj::log_no_interface(
            "ApplicationDataContainer.Containers (paire)",
            &iid as *const Guid,
        );
        *out = 0;
        return E_NOINTERFACE;
    }
    kpair(this).refs.fetch_add(1, Ordering::SeqCst);
    *out = this;
    S_OK
}
unsafe extern "win64" fn pr_add_ref(this: usize) -> u32 {
    kpair(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn pr_release(this: usize) -> u32 {
    let n = kpair(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut KidsPair));
    }
    n
}
unsafe extern "win64" fn pr_class(_this: usize, out: *mut usize) -> i32 {
    hstr_class(
        "Windows.Foundation.Collections.IKeyValuePair`2<String, Windows.Storage.ApplicationDataContainer>",
        out,
    )
}
unsafe extern "win64" fn pr_key(this: usize, out: *mut usize) -> i32 {
    let empty = Vec::new();
    hstr_out(kpair(this).path.last().unwrap_or(&empty), out)
}
unsafe extern "win64" fn pr_value(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let p = kpair(this);
    *out = new_container(p.store, p.path.clone());
    S_OK
}

static KIDS_PAIR_VTBL: VTable<8> = VTable([
    pr_qi as *const (),
    pr_add_ref as *const (),
    pr_release as *const (),
    get_iids as *const (),
    pr_class as *const (),
    trust_level as *const (),
    pr_key as *const (),
    pr_value as *const (),
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

    #[test]
    fn sous_conteneurs_listes_par_containers_2962() {
        let dir = std::env::temp_dir().join(format!("winlux-conteneurs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("winlux-local.txt");
        let text = |h: usize| unsafe {
            String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h))
        };
        unsafe {
            let root = root_container(&file, 0);
            let mut sub = 0usize;
            for n in ["b", "a"] {
                assert_eq!(
                    c_create_container(root, hs(n), DISPOSITION_ALWAYS, &mut sub),
                    S_OK
                );
                c_release(sub);
            }
            // Un petit-enfant n'est pas listé à la racine.
            assert_eq!(
                c_create_container(root, hs("a"), DISPOSITION_EXISTING, &mut sub),
                S_OK
            );
            let mut deep = 0usize;
            assert_eq!(
                c_create_container(sub, hs("c"), DISPOSITION_ALWAYS, &mut deep),
                S_OK
            );
            c_release(deep);

            // Containers (entrée 9) : IMapView des deux enfants.
            let mut view = 0usize;
            assert_eq!(c_containers(root, &mut view), S_OK);
            let qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 = call(view, 0);
            let mut same = 0usize;
            assert_eq!(qi(view, kids_iids().view.as_ptr(), &mut same), S_OK);
            assert_eq!(same, view);
            k_release(same);
            let mut n = 0u32;
            assert_eq!(k_size(view, &mut n), S_OK);
            assert_eq!(n, 2);
            let mut has = 9u8;
            assert_eq!(k_has_key(view, hs("a"), &mut has), S_OK);
            assert_eq!(has, 1);
            assert_eq!(k_has_key(view, hs("c"), &mut has), S_OK);
            assert_eq!(has, 0);
            let mut child = 0usize;
            assert_eq!(k_lookup(view, hs("absent"), &mut child), E_BOUNDS);
            assert_eq!(child, 0);
            assert_eq!(k_lookup(view, hs("b"), &mut child), S_OK);
            let mut name = 0usize;
            assert_eq!(c_name(child, &mut name), S_OK);
            assert_eq!(text(name), "b");
            c_release(child);
            let (mut f, mut g) = (5usize, 5usize);
            assert_eq!(k_split(view, &mut f, &mut g), S_OK);
            assert_eq!((f, g), (0, 0));

            // Le sous-conteneur « a » liste son propre enfant.
            let mut sub_view = 0usize;
            assert_eq!(c_containers(sub, &mut sub_view), S_OK);
            assert_eq!(k_size(sub_view, &mut n), S_OK);
            assert_eq!(n, 1);
            k_release(sub_view);
            c_release(sub);

            // Parcours : IIterable, puis des paires (nom, conteneur) dans
            // l'ordre des noms.
            let mut iterable = 0usize;
            assert_eq!(qi(view, kids_iids().iterable.as_ptr(), &mut iterable), S_OK);
            assert_eq!(iterable, view + 8);
            let mut it = 0usize;
            assert_eq!(ki_first(iterable, &mut it), S_OK);
            ki_release(iterable);
            let mut names = Vec::new();
            loop {
                let mut cur = 9u8;
                assert_eq!(it_has_current(it, &mut cur), S_OK);
                if cur == 0 {
                    break;
                }
                let mut pair = 0usize;
                assert_eq!(it_current(it, &mut pair), S_OK);
                let mut same = 0usize;
                assert_eq!(pr_qi(pair, kids_iids().pair.as_ptr(), &mut same), S_OK);
                pr_release(same);
                let mut key = 0usize;
                assert_eq!(pr_key(pair, &mut key), S_OK);
                let mut value = 0usize;
                assert_eq!(pr_value(pair, &mut value), S_OK);
                let mut vname = 0usize;
                assert_eq!(c_name(value, &mut vname), S_OK);
                assert_eq!(text(vname), text(key));
                names.push(text(key));
                c_release(value);
                assert_eq!(pr_release(pair), 0);
                let mut more = 9u8;
                assert_eq!(it_move_next(it, &mut more), S_OK);
            }
            assert_eq!(names, ["a", "b"]);
            let mut pair = 7usize;
            assert_eq!(it_current(it, &mut pair), E_BOUNDS);
            assert_eq!(pair, 0);
            assert_eq!(it_release(it), 0);

            // GetMany depuis un nouveau parcours.
            let mut it2 = 0usize;
            assert_eq!(ki_first(view + 8, &mut it2), S_OK);
            let mut items = [0usize; 4];
            let mut got = 0u32;
            assert_eq!(it_get_many(it2, 4, items.as_mut_ptr(), &mut got), S_OK);
            assert_eq!(got, 2);
            for p in &items[..2] {
                pr_release(*p);
            }
            assert_eq!(it_release(it2), 0);

            // Le parcours rendu, la vue ne garde plus que sa référence.
            assert_eq!(k_release(view), 0);
            c_release(root);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
