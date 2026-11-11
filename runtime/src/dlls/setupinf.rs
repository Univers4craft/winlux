//! setupapi.dll — lecture des .inf, files de copie, listes de pilotes et
//! paramètres d'installation (#1625), pour les outils d'installation de
//! pilotes des constructeurs (driverinstalltool_64.exe d'iCUE, devcon…).
//!
//! Rien n'est jamais chargé dans le noyau : une liste de pilotes décrit les
//! modèles du .inf, et l'installation passe par `drivers.rs` (#1532), qui
//! remplace le service du pilote par un appareil virtuel ou le bloque.
//!
//! Comportements et structures d'après la documentation publique de
//! Microsoft (setupapi.h) et les en-têtes mingw-w64 ; les tailles des
//! structures ont été vérifiées avec clang en mode x86_64-pc-windows-msvc.
#![allow(non_snake_case)]

use std::sync::Mutex;

use crate::dlls::devnodes;
use crate::dlls::drivers::{decode, store_dir, store_path, Inf};

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_INVALID_USER_BUFFER: u32 = 1784;
const ERROR_LINE_NOT_FOUND: u32 = 0xE000_0102;
const ERROR_CLASS_MISMATCH: u32 = 0xE000_0201;
const ERROR_INVALID_CLASS: u32 = 0xE000_0209;
const ERROR_NO_SUCH_DEVINST: u32 = 0xE000_020B;
const ERROR_INVALID_MACHINENAME: u32 = 0xE000_0220;
const INVALID: usize = usize::MAX;

const SPDIT_CLASSDRIVER: u32 = 1;
const SPDIT_COMPATDRIVER: u32 = 2;
const DI_ENUMSINGLEINF: u32 = 0x0001_0000;
const DIOCR_INTERFACE: u32 = 2;

/// sizeof(SP_DRVINFO_DATA_V2_W) et sizeof(SP_DRVINFO_DATA_V1_W).
const DRVINFO_V2: u32 = 1568;
const DRVINFO_V1: u32 = 1552;
/// Décalage de HardwareID dans SP_DRVINFO_DETAIL_DATA_W, et sa taille.
const DETAIL_HWID: usize = 1576;
const DETAIL_SIZE: u32 = 1584;
/// sizeof(SP_DEVINSTALL_PARAMS_W).
const INSTALL_PARAMS: usize = 584;
/// sizeof(SP_DRVINSTALL_PARAMS).
const DRV_INSTALL_PARAMS: u32 = 32;
/// sizeof(SP_DEVINFO_LIST_DETAIL_DATA_W).
const LIST_DETAIL: u32 = 560;

fn ok() -> i32 {
    crate::process::set_last_error(0);
    1
}
fn fail(code: u32) -> i32 {
    crate::process::set_last_error(code);
    0
}
unsafe fn opt_w(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| crate::dlls::wstr(p))
}
unsafe fn guid_arg(g: *const u8) -> Option<[u8; 16]> {
    (!g.is_null()).then(|| std::ptr::read_unaligned(g as *const [u8; 16]))
}
fn remote(machine: Option<String>) -> bool {
    machine.is_some_and(|m| !m.trim_start_matches('\\').is_empty())
}
/// Copie `s` (UTF-16, zéro final) ; `required` en caractères.
unsafe fn put(s: &str, buf: *mut u16, chars: u32, required: *mut u32) -> i32 {
    if devnodes::put_wide(s, buf, chars, required) {
        ok()
    } else {
        0
    }
}
unsafe fn write_wide_fixed(dst: *mut u8, chars: usize, s: &str) {
    let w: Vec<u16> = s
        .encode_utf16()
        .take(chars - 1)
        .chain(std::iter::once(0))
        .collect();
    std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, dst, w.len() * 2);
}

/// Chemin Windows d'un .inf : un nom seul se cherche dans C:\Windows\INF.
fn inf_path(name: &str) -> String {
    if name.contains('\\') || name.contains('/') || name.contains(':') {
        return name.to_string();
    }
    let in_store = store_dir().join(name);
    if in_store.exists() {
        return store_path(name);
    }
    name.to_string()
}

fn read_inf(name: &str) -> Option<Inf> {
    let bytes = std::fs::read(crate::paths::to_linux(&inf_path(name))).ok()?;
    Some(Inf::parse(&decode(&bytes)))
}

// ---- Fichiers .inf (SetupOpenInfFile, INFCONTEXT) -------------------------

static INFS: Mutex<Vec<(usize, std::sync::Arc<Inf>)>> = Mutex::new(Vec::new());
static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0x1AF0_0001);

fn next_handle() -> usize {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
fn inf_of(h: usize) -> Option<std::sync::Arc<Inf>> {
    crate::dlls::lock(&INFS)
        .iter()
        .find(|(k, _)| *k == h)
        .map(|(_, i)| i.clone())
}

fn open_inf(name: Option<String>, class: Option<String>, error_line: *mut u32) -> usize {
    if !error_line.is_null() {
        unsafe { *error_line = 0 };
    }
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return INVALID;
    };
    let Some(inf) = read_inf(&name) else {
        crate::process::set_last_error(ERROR_FILE_NOT_FOUND);
        return INVALID;
    };
    if let Some(c) = class.filter(|c| !c.is_empty()) {
        if !inf
            .version("Class")
            .is_some_and(|v| v.eq_ignore_ascii_case(&c))
        {
            crate::process::set_last_error(ERROR_CLASS_MISMATCH);
            return INVALID;
        }
    }
    let h = next_handle();
    crate::dlls::lock(&INFS).push((h, std::sync::Arc::new(inf)));
    crate::process::set_last_error(0);
    h
}

/// `SetupOpenInfFileW` : .inf lu et gardé en mémoire.
pub unsafe extern "win64" fn SetupOpenInfFileW(
    name: *const u16,
    class: *const u16,
    _style: u32,
    error_line: *mut u32,
) -> usize {
    open_inf(opt_w(name), opt_w(class), error_line)
}
pub unsafe extern "win64" fn SetupOpenInfFileA(
    name: *const u8,
    class: *const u8,
    _style: u32,
    error_line: *mut u32,
) -> usize {
    let a = |p: *const u8| (!p.is_null()).then(|| crate::dlls::cstr(p));
    open_inf(a(name), a(class), error_line)
}
pub extern "win64" fn SetupCloseInfFile(h: usize) {
    crate::dlls::lock(&INFS).retain(|(k, _)| *k != h);
}

/// INFCONTEXT : Inf, CurrentInf, Section, Line (24 octets).
unsafe fn write_ctx(ctx: *mut u8, h: usize, section: usize, line: usize) {
    (ctx as *mut usize).write_unaligned(h);
    (ctx.add(8) as *mut usize).write_unaligned(h);
    (ctx.add(16) as *mut u32).write_unaligned(section as u32);
    (ctx.add(20) as *mut u32).write_unaligned(line as u32);
}
unsafe fn read_ctx(ctx: *const u8) -> (usize, usize, usize) {
    (
        (ctx as *const usize).read_unaligned(),
        (ctx.add(16) as *const u32).read_unaligned() as usize,
        (ctx.add(20) as *const u32).read_unaligned() as usize,
    )
}

/// Première ligne de `section` (dont la clé vaut `key` si elle est donnée).
pub fn find_line(
    inf: &Inf,
    section: &str,
    key: Option<&str>,
    from: usize,
) -> Option<(usize, usize)> {
    let s = inf.section_index(section)?;
    let line = inf
        .lines(s)
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, l)| match key {
            None => true,
            Some(k) => inf
                .split_line(l)
                .0
                .is_some_and(|lk| lk.eq_ignore_ascii_case(k)),
        })?;
    Some((s, line.0))
}

pub unsafe extern "win64" fn SetupFindFirstLineW(
    h: usize,
    section: *const u16,
    key: *const u16,
    ctx: *mut u8,
) -> i32 {
    let (Some(inf), Some(section)) = (inf_of(h), opt_w(section)) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if ctx.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    match find_line(&inf, &section, opt_w(key).as_deref(), 0) {
        Some((s, l)) => {
            write_ctx(ctx, h, s, l);
            ok()
        }
        None => fail(ERROR_LINE_NOT_FOUND),
    }
}

/// `SetupFindNextLine` : ligne suivante de la même section.
pub unsafe extern "win64" fn SetupFindNextLine(ctx_in: *const u8, ctx_out: *mut u8) -> i32 {
    if ctx_in.is_null() || ctx_out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let (h, s, l) = read_ctx(ctx_in);
    let Some(inf) = inf_of(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    if l + 1 < inf.lines(s).len() {
        write_ctx(ctx_out, h, s, l + 1);
        ok()
    } else {
        fail(ERROR_LINE_NOT_FOUND)
    }
}

/// `SetupGetFieldCount` : nombre de champs après la clé.
pub unsafe extern "win64" fn SetupGetFieldCount(ctx: *const u8) -> u32 {
    if ctx.is_null() {
        return 0;
    }
    let (h, s, l) = read_ctx(ctx);
    let Some(inf) = inf_of(h) else { return 0 };
    inf.lines(s)
        .get(l)
        .map(|line| inf.split_line(line).1.len() as u32)
        .unwrap_or(0)
}

/// Champ `index` d'une ligne : 0 est la clé, 1… les valeurs.
pub fn field(inf: &Inf, line: &str, index: u32) -> Option<String> {
    let (key, fields) = inf.split_line(line);
    if index == 0 {
        return Some(key.unwrap_or_else(|| fields.first().cloned().unwrap_or_default()));
    }
    fields.get(index as usize - 1).cloned()
}

/// `SetupGetStringFieldW` : champ développé (`%chaîne%` remplacée).
pub unsafe extern "win64" fn SetupGetStringFieldW(
    ctx: *const u8,
    index: u32,
    buf: *mut u16,
    chars: u32,
    required: *mut u32,
) -> i32 {
    if ctx.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let (h, s, l) = read_ctx(ctx);
    let Some(inf) = inf_of(h) else {
        return fail(ERROR_INVALID_HANDLE);
    };
    let Some(value) = inf
        .lines(s)
        .get(l)
        .and_then(|line| field(&inf, line, index))
    else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    if buf.is_null() {
        if !required.is_null() {
            *required = value.encode_utf16().count() as u32 + 1;
        }
        return ok();
    }
    put(&value, buf, chars, required)
}

// ---- Files de copie ------------------------------------------------------

static QUEUES: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// `SetupOpenFileQueue` : file vide (aucun fichier de pilote n'est copié
/// dans System32\drivers, rien ne serait chargé).
pub extern "win64" fn SetupOpenFileQueue() -> usize {
    let h = next_handle();
    crate::dlls::lock(&QUEUES).push(h);
    crate::process::set_last_error(0);
    h
}
pub extern "win64" fn SetupCloseFileQueue(q: usize) -> i32 {
    let mut queues = crate::dlls::lock(&QUEUES);
    let before = queues.len();
    queues.retain(|h| *h != q);
    if queues.len() == before {
        return fail(ERROR_INVALID_HANDLE);
    }
    ok()
}
/// `SetupScanFileQueueW` : la file est vide, aucun rappel, résultat 0.
pub unsafe extern "win64" fn SetupScanFileQueueW(
    q: usize,
    _flags: u32,
    _hwnd: usize,
    _callback: usize,
    _context: usize,
    result: *mut u32,
) -> i32 {
    if !crate::dlls::lock(&QUEUES).contains(&q) {
        return fail(ERROR_INVALID_HANDLE);
    }
    if !result.is_null() {
        *result = 0;
    }
    ok()
}

// ---- Paramètres d'installation -------------------------------------------

/// Clé d'un appareil dans son ensemble (0 : l'ensemble lui-même).
unsafe fn device_key(set: usize, data: *const u8) -> (usize, usize) {
    let i = if data.is_null() {
        0
    } else {
        devnodes::reserved_index(data).wrapping_add(1)
    };
    (set, i)
}

static INSTALL: Mutex<Vec<((usize, usize), Vec<u8>)>> = Mutex::new(Vec::new());

fn install_params(key: (usize, usize)) -> Vec<u8> {
    crate::dlls::lock(&INSTALL)
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| {
            let mut v = vec![0u8; INSTALL_PARAMS];
            v[..4].copy_from_slice(&(INSTALL_PARAMS as u32).to_le_bytes());
            v
        })
}

pub unsafe extern "win64" fn SetupDiGetDeviceInstallParamsW(
    set: usize,
    data: *const u8,
    out: *mut u8,
) -> i32 {
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if (out as *const u32).read_unaligned() as usize != INSTALL_PARAMS {
        return fail(ERROR_INVALID_USER_BUFFER);
    }
    let v = install_params(device_key(set, data));
    std::ptr::copy_nonoverlapping(v.as_ptr(), out, INSTALL_PARAMS);
    ok()
}
pub unsafe extern "win64" fn SetupDiSetDeviceInstallParamsW(
    set: usize,
    data: *const u8,
    params: *const u8,
) -> i32 {
    if params.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if (params as *const u32).read_unaligned() as usize != INSTALL_PARAMS {
        return fail(ERROR_INVALID_USER_BUFFER);
    }
    let v = std::slice::from_raw_parts(params, INSTALL_PARAMS).to_vec();
    let key = device_key(set, data);
    let mut all = crate::dlls::lock(&INSTALL);
    all.retain(|(k, _)| *k != key);
    all.push((key, v));
    ok()
}
/// Drapeaux et dossier du .inf unique choisis par l'appelant.
fn single_inf(key: (usize, usize)) -> Option<String> {
    let v = install_params(key);
    let flags = u32::from_le_bytes([v[4], v[5], v[6], v[7]]);
    if flags & DI_ENUMSINGLEINF == 0 {
        return None;
    }
    let w: Vec<u16> = v[60..60 + 520]
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    let p = String::from_utf16_lossy(&w);
    (!p.is_empty()).then_some(p)
}
/// `SetupDiSetClassInstallParamsW` : acceptés (DIF_REMOVE, DIF_PROPERTYCHANGE…).
pub extern "win64" fn SetupDiSetClassInstallParamsW(
    _set: usize,
    _data: *const u8,
    _p: *const u8,
    _size: u32,
) -> i32 {
    ok()
}

// ---- Listes de pilotes ---------------------------------------------------

/// Modèle d'un .inf : une entrée de la section des modèles d'un fabricant.
#[derive(Clone, Debug, PartialEq)]
pub struct DriverEntry {
    pub description: String,
    pub manufacturer: String,
    pub provider: String,
    pub section: String,
    pub inf: String,
    pub hardware_id: String,
    pub compatible_ids: Vec<String>,
    /// Date (FILETIME) et version (a.b.c.d sur 64 bits) de DriverVer.
    pub date: u64,
    pub version: u64,
}

/// `mm/jj/aaaa` → FILETIME (centaines de ns depuis 1601).
fn filetime(date: &str) -> u64 {
    let p: Vec<i64> = date
        .split('/')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    let [m, d, y] = p[..] else { return 0 };
    // Jours depuis 1970 (algorithme de Howard Hinnant, domaine public).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    ((days + 134_774) * 86_400 * 10_000_000).max(0) as u64
}
fn version_number(v: &str) -> u64 {
    v.split('.')
        .take(4)
        .enumerate()
        .map(|(i, p)| (p.trim().parse::<u64>().unwrap_or(0) & 0xFFFF) << (48 - 16 * i))
        .sum()
}

/// Modèles d'un .inf pour x64 : [Manufacturer] puis sections de modèles,
/// avec la décoration NTamd64 quand elle existe.
pub fn models(inf: &Inf, inf_name: &str) -> Vec<DriverEntry> {
    let provider = inf.version("Provider").unwrap_or_default();
    let driver_ver = inf.version("DriverVer").unwrap_or_default();
    let (date, version) = match driver_ver.split_once(',') {
        Some((d, v)) => (filetime(d), version_number(v)),
        None => (filetime(&driver_ver), 0),
    };
    let mut out = Vec::new();
    let Some(mfg) = inf.section_index("Manufacturer") else {
        return out;
    };
    for line in inf.lines(mfg) {
        let (key, fields) = inf.split_line(line);
        let Some(base) = fields.first().filter(|b| !b.is_empty()) else {
            continue;
        };
        let manufacturer = key.unwrap_or_else(|| base.clone());
        let decorated = fields[1..]
            .iter()
            .filter(|d| d.to_ascii_lowercase().starts_with("ntamd64"))
            .map(|d| format!("{base}.{d}"))
            .find(|s| inf.section_index(s).is_some());
        let section = decorated.unwrap_or_else(|| base.clone());
        let Some(si) = inf.section_index(&section) else {
            continue;
        };
        for model in inf.lines(si) {
            let (desc, f) = inf.split_line(model);
            let (Some(desc), Some(install)) = (desc, f.first()) else {
                continue;
            };
            let Some(hwid) = f.get(1).filter(|h| !h.is_empty()) else {
                continue;
            };
            out.push(DriverEntry {
                description: desc,
                manufacturer: manufacturer.clone(),
                provider: provider.clone(),
                section: install.clone(),
                inf: inf_name.to_string(),
                hardware_id: hwid.clone(),
                compatible_ids: f[2..].iter().filter(|c| !c.is_empty()).cloned().collect(),
                date,
                version,
            });
        }
    }
    out
}

static DRIVERS: Mutex<Vec<((usize, usize, u32), Vec<DriverEntry>)>> = Mutex::new(Vec::new());

fn ids_of(node: &devnodes::Node) -> Vec<String> {
    [1u32, 2]
        .iter()
        .filter_map(|p| node.property(*p))
        .flat_map(|(_, w)| {
            String::from_utf16_lossy(&w)
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .chain(std::iter::once(node.id.clone()))
        .collect()
}

/// `SetupDiBuildDriverInfoList` : modèles du .inf choisi (DI_ENUMSINGLEINF)
/// ou de tout le magasin ; SPDIT_COMPATDRIVER ne garde que ceux qui
/// correspondent aux identifiants de l'appareil.
pub unsafe extern "win64" fn SetupDiBuildDriverInfoList(
    set: usize,
    data: *const u8,
    kind: u32,
) -> i32 {
    if kind != SPDIT_CLASSDRIVER && kind != SPDIT_COMPATDRIVER {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let node = devnodes::node_in_set(set, data);
    if kind == SPDIT_COMPATDRIVER && node.is_none() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let key = device_key(set, data);
    let mut infs: Vec<String> = Vec::new();
    match single_inf(key).or_else(|| single_inf((set, 0))) {
        Some(p) => infs.push(p),
        None => {
            if let Ok(dir) = std::fs::read_dir(store_dir()) {
                let mut names: Vec<String> = dir
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.to_ascii_lowercase().ends_with(".inf"))
                    .collect();
                names.sort();
                infs.extend(names.iter().map(|n| store_path(n)));
            }
        }
    }
    let wanted: Vec<String> = node.as_ref().map(ids_of).unwrap_or_default();
    let mut list = Vec::new();
    for path in infs {
        let Some(inf) = read_inf(&path) else { continue };
        for m in models(&inf, &path) {
            let matches = std::iter::once(&m.hardware_id)
                .chain(&m.compatible_ids)
                .any(|id| wanted.iter().any(|w| w.eq_ignore_ascii_case(id)));
            if kind == SPDIT_CLASSDRIVER || matches {
                list.push(m);
            }
        }
    }
    let mut all = crate::dlls::lock(&DRIVERS);
    all.retain(|(k, _)| *k != (key.0, key.1, kind));
    all.push(((key.0, key.1, kind), list));
    ok()
}

fn driver_list(key: (usize, usize), kind: u32) -> Option<Vec<DriverEntry>> {
    crate::dlls::lock(&DRIVERS)
        .iter()
        .find(|(k, _)| *k == (key.0, key.1, kind))
        .map(|(_, v)| v.clone())
}

/// SP_DRVINFO_DATA_V2_W (ou V1, sans date ni version).
unsafe fn write_drvinfo(out: *mut u8, kind: u32, index: usize, d: &DriverEntry) {
    let size = (out as *const u32).read_unaligned();
    (out.add(4) as *mut u32).write_unaligned(kind);
    (out.add(8) as *mut usize).write_unaligned(index + 1);
    write_wide_fixed(out.add(16), 256, &d.description);
    write_wide_fixed(out.add(16 + 512), 256, &d.manufacturer);
    write_wide_fixed(out.add(16 + 1024), 256, &d.provider);
    if size == DRVINFO_V2 {
        (out.add(1552) as *mut u64).write_unaligned(d.date);
        (out.add(1560) as *mut u64).write_unaligned(d.version);
    }
}

pub unsafe extern "win64" fn SetupDiEnumDriverInfoW(
    set: usize,
    data: *const u8,
    kind: u32,
    index: u32,
    out: *mut u8,
) -> i32 {
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    let size = (out as *const u32).read_unaligned();
    if size != DRVINFO_V2 && size != DRVINFO_V1 {
        return fail(ERROR_INVALID_USER_BUFFER);
    }
    let list = driver_list(device_key(set, data), kind).unwrap_or_default();
    match list.get(index as usize) {
        Some(d) => {
            write_drvinfo(out, kind, index as usize, d);
            ok()
        }
        None => fail(ERROR_NO_MORE_ITEMS),
    }
}

unsafe fn driver_of(set: usize, data: *const u8, drv: *const u8) -> Option<DriverEntry> {
    if drv.is_null() {
        return None;
    }
    let kind = (drv.add(4) as *const u32).read_unaligned();
    let index = (drv.add(8) as *const usize)
        .read_unaligned()
        .checked_sub(1)?;
    driver_list(device_key(set, data), kind)?
        .get(index)
        .cloned()
}

/// `SetupDiGetDriverInfoDetailW` : section, .inf, description et
/// identifiants (matériel puis compatibles, en multi-chaîne).
pub unsafe extern "win64" fn SetupDiGetDriverInfoDetailW(
    set: usize,
    data: *const u8,
    drv: *const u8,
    out: *mut u8,
    size: u32,
    required: *mut u32,
) -> i32 {
    let Some(d) = driver_of(set, data, drv) else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    let mut ids: Vec<u16> = d
        .hardware_id
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let compat_at = ids.len();
    for c in &d.compatible_ids {
        ids.extend(c.encode_utf16().chain(std::iter::once(0)));
    }
    ids.push(0);
    let need = (DETAIL_HWID + ids.len() * 2) as u32;
    if !required.is_null() {
        *required = need;
    }
    if out.is_null() {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    if (out as *const u32).read_unaligned() != DETAIL_SIZE {
        return fail(ERROR_INVALID_USER_BUFFER);
    }
    if size < need {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    (out.add(4) as *mut u64).write_unaligned(d.date);
    let compat_len = if d.compatible_ids.is_empty() {
        0
    } else {
        ids.len() - compat_at
    };
    (out.add(12) as *mut u32).write_unaligned(if compat_len == 0 { 0 } else { compat_at as u32 });
    (out.add(16) as *mut u32).write_unaligned(compat_len as u32);
    (out.add(24) as *mut usize).write_unaligned(0);
    write_wide_fixed(out.add(32), 256, &d.section);
    write_wide_fixed(out.add(544), 260, &d.inf);
    write_wide_fixed(out.add(1064), 256, &d.description);
    std::ptr::copy_nonoverlapping(
        ids.as_ptr() as *const u8,
        out.add(DETAIL_HWID),
        ids.len() * 2,
    );
    ok()
}

/// SP_DRVINSTALL_PARAMS : rang 0 (meilleur), aucun drapeau.
pub unsafe extern "win64" fn SetupDiGetDriverInstallParamsW(
    set: usize,
    data: *const u8,
    drv: *const u8,
    out: *mut u8,
) -> i32 {
    if out.is_null() || driver_of(set, data, drv).is_none() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if (out as *const u32).read_unaligned() != DRV_INSTALL_PARAMS {
        return fail(ERROR_INVALID_USER_BUFFER);
    }
    std::ptr::write_bytes(out.add(4), 0, DRV_INSTALL_PARAMS as usize - 4);
    ok()
}

static SELECTED: Mutex<Vec<((usize, usize), DriverEntry)>> = Mutex::new(Vec::new());

/// `SetupDiSetSelectedDriverW` : pilote retenu pour DIF_INSTALLDEVICE.
pub unsafe extern "win64" fn SetupDiSetSelectedDriverW(
    set: usize,
    data: *const u8,
    drv: *const u8,
) -> i32 {
    let key = device_key(set, data);
    let mut sel = crate::dlls::lock(&SELECTED);
    sel.retain(|(k, _)| *k != key);
    if drv.is_null() {
        return ok();
    }
    match driver_of(set, data, drv) {
        Some(d) => {
            sel.push((key, d));
            ok()
        }
        None => fail(ERROR_INVALID_PARAMETER),
    }
}

pub unsafe extern "win64" fn SetupDiDestroyDriverInfoList(
    set: usize,
    data: *const u8,
    kind: u32,
) -> i32 {
    let key = device_key(set, data);
    crate::dlls::lock(&DRIVERS).retain(|(k, _)| *k != (key.0, key.1, kind));
    ok()
}

// ---- Ensembles et classes ------------------------------------------------

/// `SetupDiGetClassDevsExW` : machine locale seulement ; un ensemble
/// existant reçoit les appareils trouvés.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn SetupDiGetClassDevsExW(
    guid: *const u8,
    enumerator: *const u16,
    _hwnd: usize,
    flags: u32,
    set: usize,
    machine: *const u16,
    _reserved: usize,
) -> usize {
    if remote(opt_w(machine)) {
        crate::process::set_last_error(ERROR_INVALID_MACHINENAME);
        return INVALID;
    }
    let e = opt_w(enumerator);
    let Some(new) = devnodes::get_class_devs(guid_arg(guid), e.as_deref(), flags) else {
        return INVALID;
    };
    if set == 0 || set == INVALID {
        return new;
    }
    if devnodes::merge_set(set, new) {
        set
    } else {
        devnodes::destroy(new);
        crate::process::set_last_error(ERROR_INVALID_HANDLE);
        INVALID
    }
}

/// `SetupDiOpenDeviceInfoW` : appareil présent ajouté à l'ensemble.
pub unsafe extern "win64" fn SetupDiOpenDeviceInfoW(
    set: usize,
    id: *const u16,
    _hwnd: usize,
    _flags: u32,
    data: *mut u8,
) -> i32 {
    let Some(id) = opt_w(id) else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    let Some(node) = devnodes::node_by_id(&id) else {
        return fail(ERROR_NO_SUCH_DEVINST);
    };
    if devnodes::add_to_set(set, node, data) {
        ok()
    } else {
        fail(ERROR_INVALID_HANDLE)
    }
}

/// SP_DEVINFO_LIST_DETAIL_DATA_W : machine locale, sans classe.
pub unsafe extern "win64" fn SetupDiGetDeviceInfoListDetailW(_set: usize, out: *mut u8) -> i32 {
    if out.is_null() {
        return fail(ERROR_INVALID_PARAMETER);
    }
    if (out as *const u32).read_unaligned() != LIST_DETAIL {
        return fail(ERROR_INVALID_USER_BUFFER);
    }
    std::ptr::write_bytes(out.add(4), 0, LIST_DETAIL as usize - 4);
    ok()
}

/// `SetupDiBuildClassInfoListExW` : classes d'installation connues.
pub unsafe extern "win64" fn SetupDiBuildClassInfoListExW(
    _flags: u32,
    list: *mut u8,
    size: u32,
    required: *mut u32,
    machine: *const u16,
    _reserved: usize,
) -> i32 {
    if remote(opt_w(machine)) {
        return fail(ERROR_INVALID_MACHINENAME);
    }
    let n = devnodes::CLASSES.len() as u32;
    if !required.is_null() {
        *required = n;
    }
    if list.is_null() || size < n {
        return fail(ERROR_INSUFFICIENT_BUFFER);
    }
    for (i, (_, g, _)) in devnodes::CLASSES.iter().enumerate() {
        std::ptr::copy_nonoverlapping(g.as_ptr(), list.add(16 * i), 16);
    }
    ok()
}

pub unsafe extern "win64" fn SetupDiClassGuidsFromNameExW(
    name: *const u16,
    list: *mut u8,
    size: u32,
    required: *mut u32,
    machine: *const u16,
    _reserved: usize,
) -> i32 {
    if remote(opt_w(machine)) {
        return fail(ERROR_INVALID_MACHINENAME);
    }
    match opt_w(name) {
        Some(n) => devnodes::class_guids(&n, list, size, required),
        None => fail(ERROR_INVALID_PARAMETER),
    }
}

pub unsafe extern "win64" fn SetupDiClassNameFromGuidExW(
    guid: *const u8,
    name: *mut u16,
    size: u32,
    required: *mut u32,
    machine: *const u16,
    _reserved: usize,
) -> i32 {
    if remote(opt_w(machine)) {
        return fail(ERROR_INVALID_MACHINENAME);
    }
    match guid_arg(guid).as_ref().and_then(devnodes::class_by_guid) {
        Some((n, _)) => put(n, name, size, required),
        None => fail(ERROR_INVALID_CLASS),
    }
}
pub unsafe extern "win64" fn SetupDiClassNameFromGuidW(
    guid: *const u8,
    name: *mut u16,
    size: u32,
    required: *mut u32,
) -> i32 {
    SetupDiClassNameFromGuidExW(guid, name, size, required, std::ptr::null(), 0)
}

pub unsafe extern "win64" fn SetupDiGetClassDescriptionExW(
    guid: *const u8,
    desc: *mut u16,
    size: u32,
    required: *mut u32,
    machine: *const u16,
    _reserved: usize,
) -> i32 {
    if remote(opt_w(machine)) {
        return fail(ERROR_INVALID_MACHINENAME);
    }
    match guid_arg(guid).as_ref().and_then(devnodes::class_by_guid) {
        Some((_, d)) => put(d, desc, size, required),
        None => fail(ERROR_INVALID_CLASS),
    }
}
pub unsafe extern "win64" fn SetupDiGetClassDescriptionW(
    guid: *const u8,
    desc: *mut u16,
    size: u32,
    required: *mut u32,
) -> i32 {
    SetupDiGetClassDescriptionExW(guid, desc, size, required, std::ptr::null(), 0)
}

/// Classe d'un .inf : ClassGUID de [Version], sinon d'après son nom.
pub fn inf_class(inf: &Inf) -> Option<(String, [u8; 16])> {
    let name = inf.version("Class").unwrap_or_default();
    let g = inf
        .version("ClassGUID")
        .and_then(|g| devnodes::parse_guid(&g))
        .or_else(|| devnodes::class_by_name(&name))?;
    Some((name, g))
}

pub unsafe extern "win64" fn SetupDiGetINFClassW(
    inf: *const u16,
    guid: *mut u8,
    name: *mut u16,
    size: u32,
    required: *mut u32,
) -> i32 {
    let Some(path) = opt_w(inf) else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    let Some(parsed) = read_inf(&path) else {
        return fail(ERROR_FILE_NOT_FOUND);
    };
    let Some((n, g)) = inf_class(&parsed) else {
        return fail(ERROR_INVALID_CLASS);
    };
    if !guid.is_null() {
        std::ptr::copy_nonoverlapping(g.as_ptr(), guid, 16);
    }
    put(&n, name, size, required)
}

/// `SetupDiOpenClassRegKeyExW` : clé de la classe (Control\Class) ou de
/// l'interface (Control\DeviceClasses), créée au besoin.
pub unsafe extern "win64" fn SetupDiOpenClassRegKeyExW(
    guid: *const u8,
    _access: u32,
    flags: u32,
    machine: *const u16,
    _reserved: usize,
) -> usize {
    if remote(opt_w(machine)) {
        crate::process::set_last_error(ERROR_INVALID_MACHINENAME);
        return INVALID;
    }
    let base = if flags == DIOCR_INTERFACE {
        "hklm\\system\\currentcontrolset\\control\\deviceclasses"
    } else {
        "hklm\\system\\currentcontrolset\\control\\class"
    };
    let key = match guid_arg(guid) {
        Some(g) => format!("{base}\\{}", devnodes::guid_text(&g)),
        None => base.to_string(),
    };
    let class = guid_arg(guid).as_ref().and_then(devnodes::class_by_guid);
    let made = crate::registry::write(|d| {
        d.ensure(&key);
        if let (Some((n, desc)), false) = (class, flags == DIOCR_INTERFACE) {
            d.set(&key, "Class", 1, crate::registry::utf16z(n));
            d.set(&key, "", 1, crate::registry::utf16z(desc));
        }
        Ok(())
    });
    match made.and_then(|_| crate::registry::open(&key)) {
        Ok(h) => {
            crate::process::set_last_error(0);
            h
        }
        Err(e) => {
            crate::process::set_last_error(e as u32);
            INVALID
        }
    }
}
pub unsafe extern "win64" fn SetupDiOpenClassRegKey(guid: *const u8, access: u32) -> usize {
    SetupDiOpenClassRegKeyExW(guid, access, 1, std::ptr::null(), 0)
}

/// `DiUninstallDevice` (newdev.dll) : un appareil créé par un installeur
/// est retiré ; un vrai appareil n'a aucun pilote Windows à retirer.
pub unsafe extern "win64" fn DiUninstallDevice(
    _hwnd: usize,
    set: usize,
    data: *const u8,
    _flags: u32,
    reboot: *mut i32,
) -> i32 {
    if !reboot.is_null() {
        *reboot = 0;
    }
    let Some(node) = devnodes::node_in_set(set, data) else {
        return fail(ERROR_INVALID_PARAMETER);
    };
    if matches!(node.kind, devnodes::Kind::Virtual(_)) {
        devnodes::remove_virtual(&node.id);
    }
    ok()
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetupOpenInfFileW" => fn SetupOpenInfFileW,
        "SetupOpenInfFileA" => fn SetupOpenInfFileA,
        "SetupCloseInfFile" => fn SetupCloseInfFile,
        "SetupFindFirstLineW" => fn SetupFindFirstLineW,
        "SetupFindNextLine" => fn SetupFindNextLine,
        "SetupGetFieldCount" => fn SetupGetFieldCount,
        "SetupGetStringFieldW" => fn SetupGetStringFieldW,
        "SetupOpenFileQueue" => fn SetupOpenFileQueue,
        "SetupCloseFileQueue" => fn SetupCloseFileQueue,
        "SetupScanFileQueueW" => fn SetupScanFileQueueW,
        "SetupDiGetDeviceInstallParamsW" => fn SetupDiGetDeviceInstallParamsW,
        "SetupDiSetDeviceInstallParamsW" => fn SetupDiSetDeviceInstallParamsW,
        "SetupDiSetClassInstallParamsW" => fn SetupDiSetClassInstallParamsW,
        "SetupDiBuildDriverInfoList" => fn SetupDiBuildDriverInfoList,
        "SetupDiEnumDriverInfoW" => fn SetupDiEnumDriverInfoW,
        "SetupDiGetDriverInfoDetailW" => fn SetupDiGetDriverInfoDetailW,
        "SetupDiGetDriverInstallParamsW" => fn SetupDiGetDriverInstallParamsW,
        "SetupDiSetSelectedDriverW" => fn SetupDiSetSelectedDriverW,
        "SetupDiDestroyDriverInfoList" => fn SetupDiDestroyDriverInfoList,
        "SetupDiGetClassDevsExW" => fn SetupDiGetClassDevsExW,
        "SetupDiOpenDeviceInfoW" => fn SetupDiOpenDeviceInfoW,
        "SetupDiGetDeviceInfoListDetailW" => fn SetupDiGetDeviceInfoListDetailW,
        "SetupDiBuildClassInfoListExW" => fn SetupDiBuildClassInfoListExW,
        "SetupDiClassGuidsFromNameExW" => fn SetupDiClassGuidsFromNameExW,
        "SetupDiClassNameFromGuidExW" => fn SetupDiClassNameFromGuidExW,
        "SetupDiClassNameFromGuidW" => fn SetupDiClassNameFromGuidW,
        "SetupDiGetClassDescriptionExW" => fn SetupDiGetClassDescriptionExW,
        "SetupDiGetClassDescriptionW" => fn SetupDiGetClassDescriptionW,
        "SetupDiGetINFClassW" => fn SetupDiGetINFClassW,
        "SetupDiOpenClassRegKeyExW" => fn SetupDiOpenClassRegKeyExW,
        "SetupDiOpenClassRegKey" => fn SetupDiOpenClassRegKey,
        "DiUninstallDevice" => fn DiUninstallDevice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INF: &str = "[Version]\r\n\
Signature=\"$WINDOWS NT$\"\r\n\
Class=HIDClass\r\n\
ClassGUID={745a17a0-74d3-11d0-b6fe-00a0c90f57da}\r\n\
Provider=%Mfg%\r\n\
DriverVer=07/14/2017,3.3.0.1\r\n\
\r\n\
[Manufacturer]\r\n\
%Mfg% = Essai, NTamd64, NTx86\r\n\
\r\n\
[Essai.NTamd64]\r\n\
%Clavier% = Clavier_Install, USB\\VID_1209&PID_0001, USB\\Class_03\r\n\
%Souris% = Souris_Install, USB\\VID_1209&PID_0002\r\n\
\r\n\
[Essai.NTx86]\r\n\
%Clavier% = Mauvaise_Section, USB\\VID_1209&PID_0001\r\n\
\r\n\
[Strings]\r\n\
Mfg = \"Winlux Essais\"\r\n\
Clavier = \"Clavier d'essai\"\r\n\
Souris = \"Souris d'essai\"\r\n";

    #[test]
    fn modeles_ntamd64_du_inf() {
        let inf = Inf::parse(INF);
        let m = models(&inf, "C:\\essai.inf");
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].description, "Clavier d'essai");
        assert_eq!(m[0].manufacturer, "Winlux Essais");
        assert_eq!(m[0].provider, "Winlux Essais");
        assert_eq!(m[0].section, "Clavier_Install");
        assert_eq!(m[0].hardware_id, "USB\\VID_1209&PID_0001");
        assert_eq!(m[0].compatible_ids, vec!["USB\\Class_03".to_string()]);
        assert_eq!(m[0].version, (3u64 << 48) | (3 << 32) | 1);
        assert_eq!(m[1].compatible_ids, Vec::<String>::new());
    }

    #[test]
    fn date_de_driverver_en_filetime() {
        // 01/01/1970 = 116444736000000000 (écart public 1601-1970).
        assert_eq!(filetime("01/01/1970"), 116_444_736_000_000_000);
        assert_eq!(
            filetime("07/14/2017") - filetime("07/13/2017"),
            864_000_000_000
        );
        assert_eq!(filetime("n'importe quoi"), 0);
    }

    #[test]
    fn lignes_et_champs() {
        let inf = Inf::parse(INF);
        let (s, l) = find_line(&inf, "version", Some("class"), 0).unwrap();
        let line = &inf.lines(s)[l];
        assert_eq!(field(&inf, line, 0).as_deref(), Some("Class"));
        assert_eq!(field(&inf, line, 1).as_deref(), Some("HIDClass"));
        assert_eq!(field(&inf, line, 2), None);
        let (s, l) = find_line(&inf, "Manufacturer", None, 0).unwrap();
        let line = &inf.lines(s)[l];
        assert_eq!(field(&inf, line, 0).as_deref(), Some("Winlux Essais"));
        assert_eq!(field(&inf, line, 2).as_deref(), Some("NTamd64"));
        assert!(find_line(&inf, "Absente", None, 0).is_none());
        assert_eq!(
            inf_class(&inf),
            Some(("HIDClass".to_string(), devnodes::HID_CLASS))
        );
    }
}
