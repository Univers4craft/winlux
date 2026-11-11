//! Actions personnalisées DLL de Windows Installer (#833, types 1 et 17).
//!
//! Le msiexec de Winlux lance chaque action DLL dans un processus à part :
//! `winlux --msi-dll-action <dll> <point d'entrée>`. Ce processus charge la
//! DLL comme `LoadLibrary`, appelle `UINT __stdcall Entree(MSIHANDLE)` et
//! répond aux fonctions de session de `msi.dll` décrites dans la
//! documentation publique de Windows Installer (MsiGetProperty,
//! MsiSetProperty, MsiGetMode, MsiProcessMessage, enregistrements…).
//!
//! Les propriétés de la session passent par un fichier
//! (`WINLUX_MSI_SESSION`) : écrites par le msiexec avant l'appel, relues
//! après, si bien qu'une action immédiate qui pose une propriété la rend
//! visible aux actions suivantes. Pas de base de données active
//! (`MsiGetActiveDatabase` rend 0, comme pour une action différée) : une
//! action qui en a besoin échoue proprement, journalisée, sans arrêter
//! l'installation.

use super::cond::Context;
use std::collections::HashMap;
use std::sync::Mutex;

const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_MORE_DATA: u32 = 234;
const MSI_NULL_INTEGER: i32 = i32::MIN;
/// Poignée de la session passée au point d'entrée.
pub const SESSION_HANDLE: u32 = 0x4D53;
const INSTALLSTATE_ABSENT: i32 = 2;
const INSTALLSTATE_LOCAL: i32 = 3;

/// Champ d'un enregistrement MSI.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Field {
    Null,
    Int(i32),
    Str(String),
    /// Flux binaire et position de lecture (MsiRecordReadStream).
    Stream(Vec<u8>, usize),
}

#[derive(Default)]
struct Session {
    props: HashMap<String, String>,
    loaded: bool,
    records: HashMap<u32, Vec<Field>>,
    next: u32,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Session) -> T) -> T {
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let s = guard.get_or_insert_with(Session::default);
    if !s.loaded {
        s.loaded = true;
        if let Some(path) = std::env::var_os("WINLUX_MSI_SESSION") {
            if let Ok(text) = std::fs::read_to_string(path) {
                s.props = decode(&text);
            }
        }
        s.next = 0x100;
    }
    f(s)
}

fn escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn unescape(v: &str) -> String {
    let mut out = String::new();
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            // Fichier écrit à la main : `C:\Program Files` reste tel quel.
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Propriétés au format du fichier de session : `nom\tvaleur` par ligne.
pub fn encode(props: &HashMap<String, String>) -> String {
    let mut names: Vec<&String> = props.keys().collect();
    names.sort();
    names
        .into_iter()
        .map(|n| format!("{}\t{}\n", escape(n), escape(&props[n])))
        .collect()
}

pub fn decode(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(n, v)| (unescape(n), unescape(v)))
        .collect()
}

/// Clé du fichier de session qui porte le code rendu par l'action (Linux
/// tronque les codes de sortie à 8 bits : 1603 deviendrait 67).
pub const CODE_KEY: &str = "\u{1}code";
/// Clé des actions demandées par MsiDoAction, une par ligne.
pub const DO_KEY: &str = "\u{1}do";

pub(super) fn property(name: &str) -> Option<String> {
    with(|s| s.props.get(name).cloned())
}

/// Action différée (`MSIRUNMODE_SCHEDULED`).
pub(super) fn scheduled() -> bool {
    flag("WINLUX_MSI_SCHEDULED")
}

/// Nouvelle poignée (enregistrement ou vue).
pub(super) fn new_handle() -> u32 {
    with(|s| {
        let h = s.next;
        s.next += 1;
        h
    })
}

pub(super) fn new_record(fields: Vec<Field>) -> u32 {
    let h = new_handle();
    with(|s| s.records.insert(h, fields));
    h
}

pub(super) fn record_fields(h: u32) -> Option<Vec<Field>> {
    with(|s| s.records.get(&h).cloned())
}

/// Réécrit le fichier de session (fin de l'action) avec son code.
pub fn save(code: u32) {
    let Some(path) = std::env::var_os("WINLUX_MSI_SESSION") else {
        return;
    };
    let text = with(|s| {
        let mut props = s.props.clone();
        props.insert(CODE_KEY.into(), code.to_string());
        encode(&props)
    });
    let _ = std::fs::write(path, text);
}

fn flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| v == "1")
}

unsafe fn text(p: usize, wide: bool) -> String {
    if wide {
        crate::dlls::wstr(p as *const u16)
    } else {
        crate::dlls::cstr(p as *const u8)
    }
}

/// Copie façon Windows Installer : `*pcch` = taille du tampon en entrée,
/// longueur (sans le zéro) en sortie ; ERROR_MORE_DATA si trop petit.
pub(super) unsafe fn put(value: &str, buf: usize, pcch: usize, wide: bool) -> u32 {
    if pcch == 0 {
        return if buf == 0 {
            ERROR_SUCCESS
        } else {
            ERROR_INVALID_PARAMETER
        };
    }
    let cap = *(pcch as *const u32) as usize;
    let units: Vec<u16>;
    let bytes: Vec<u8>;
    let len = if wide {
        units = value.encode_utf16().collect();
        units.len()
    } else {
        bytes = crate::dlls::encode_ansi(value);
        bytes.len()
    };
    *(pcch as *mut u32) = len as u32;
    if buf == 0 {
        return ERROR_SUCCESS;
    }
    if cap <= len {
        // Ce qui tient est copié, terminé par un zéro.
        if cap > 0 {
            if wide {
                crate::dlls::put_text_w(value, buf as *mut u16, cap as u32);
            } else {
                crate::dlls::put_text_a(value, buf as *mut u8, cap as u32);
            }
        }
        return ERROR_MORE_DATA;
    }
    if wide {
        crate::dlls::put_w(value, buf as *mut u16, cap as u32);
    } else {
        crate::dlls::put_a(value, buf as *mut u8, cap as u32);
    }
    ERROR_SUCCESS
}

fn is_session(h: usize) -> bool {
    h as u32 == SESSION_HANDLE
}

fn get_property(h: usize, name: usize, buf: usize, pcch: usize, wide: bool) -> u32 {
    if !is_session(h) {
        return ERROR_INVALID_HANDLE;
    }
    let name = unsafe { text(name, wide) };
    let value = with(|s| s.props.get(&name).cloned().unwrap_or_default());
    unsafe { put(&value, buf, pcch, wide) }
}

pub extern "win64" fn msi_get_property_w(h: usize, name: usize, buf: usize, pcch: usize) -> u32 {
    get_property(h, name, buf, pcch, true)
}
pub extern "win64" fn msi_get_property_a(h: usize, name: usize, buf: usize, pcch: usize) -> u32 {
    get_property(h, name, buf, pcch, false)
}

fn set_property(h: usize, name: usize, value: usize, wide: bool) -> u32 {
    if !is_session(h) {
        return ERROR_INVALID_HANDLE;
    }
    if name == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let (name, value) = unsafe { (text(name, wide), text(value, wide)) };
    with(|s| {
        if value.is_empty() {
            s.props.remove(&name);
        } else {
            s.props.insert(name, value);
        }
    });
    ERROR_SUCCESS
}

pub extern "win64" fn msi_set_property_w(h: usize, name: usize, value: usize) -> u32 {
    set_property(h, name, value, true)
}
pub extern "win64" fn msi_set_property_a(h: usize, name: usize, value: usize) -> u32 {
    set_property(h, name, value, false)
}

/// MsiGetMode : seules les questions « différée », « retour arrière »,
/// « validation » et « redémarrage » ont une réponse sûre ici.
pub extern "win64" fn msi_get_mode(h: usize, mode: u32) -> i32 {
    if !is_session(h) {
        return 0;
    }
    match mode {
        16 => flag("WINLUX_MSI_SCHEDULED") as i32, // MSIRUNMODE_SCHEDULED
        17 | 18 => 0,                              // ROLLBACK, COMMIT
        _ => 0,
    }
}

pub extern "win64" fn msi_set_mode(_h: usize, _mode: u32, _state: i32) -> u32 {
    ERROR_SUCCESS
}

pub extern "win64" fn msi_get_language(h: usize) -> u32 {
    if !is_session(h) {
        return 0;
    }
    with(|s| {
        s.props
            .get("ProductLanguage")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1033)
    })
}

/// MsiGetComponentState / MsiGetFeatureState : présent et laissé présent
/// pendant l'installation, présent puis retiré pendant la désinstallation.
pub extern "win64" fn msi_get_state(
    h: usize,
    _name: usize,
    installed: usize,
    action: usize,
) -> u32 {
    if !is_session(h) {
        return ERROR_INVALID_HANDLE;
    }
    let removing = flag("WINLUX_MSI_REMOVING");
    unsafe {
        if installed != 0 {
            *(installed as *mut i32) = if removing {
                INSTALLSTATE_LOCAL
            } else {
                INSTALLSTATE_ABSENT
            };
        }
        if action != 0 {
            *(action as *mut i32) = if removing {
                INSTALLSTATE_ABSENT
            } else {
                INSTALLSTATE_LOCAL
            };
        }
    }
    ERROR_SUCCESS
}

pub extern "win64" fn msi_set_state(h: usize, _name: usize, _state: i32) -> u32 {
    if is_session(h) {
        ERROR_SUCCESS
    } else {
        ERROR_INVALID_HANDLE
    }
}

/// MsiDoAction : l'action demandée est notée et lancée par le msiexec
/// au retour de l'action DLL (une action personnalisée, souvent différée,
/// préparée par l'appelant) ; les actions standard sont déjà faites par
/// le msiexec lui-même.
fn do_action(h: usize, action: usize, wide: bool) -> u32 {
    if !is_session(h) {
        return ERROR_INVALID_HANDLE;
    }
    let name = unsafe { text(action, wide) };
    if name.is_empty() {
        return ERROR_INVALID_PARAMETER;
    }
    with(|s| {
        let list = s.props.entry(DO_KEY.into()).or_default();
        list.push_str(&name);
        list.push('\n');
    });
    ERROR_SUCCESS
}

pub extern "win64" fn msi_do_action_w(h: usize, action: usize) -> u32 {
    do_action(h, action, true)
}
pub extern "win64" fn msi_do_action_a(h: usize, action: usize) -> u32 {
    do_action(h, action, false)
}

pub extern "win64" fn msi_sequence(h: usize, _table: usize, _mode: i32) -> u32 {
    if is_session(h) {
        ERROR_SUCCESS
    } else {
        ERROR_INVALID_HANDLE
    }
}

fn path_property(h: usize, folder: usize, buf: usize, pcch: usize, wide: bool) -> u32 {
    if !is_session(h) {
        return ERROR_INVALID_HANDLE;
    }
    let name = unsafe { text(folder, wide) };
    let Some(value) = with(|s| s.props.get(&name).cloned()) else {
        return 1314; // ERROR_DIRECTORY
    };
    unsafe { put(&value, buf, pcch, wide) }
}

pub extern "win64" fn msi_get_target_path_w(h: usize, f: usize, buf: usize, pcch: usize) -> u32 {
    path_property(h, f, buf, pcch, true)
}
pub extern "win64" fn msi_get_target_path_a(h: usize, f: usize, buf: usize, pcch: usize) -> u32 {
    path_property(h, f, buf, pcch, false)
}

pub extern "win64" fn msi_set_target_path_w(h: usize, folder: usize, path: usize) -> u32 {
    set_property(h, folder, path, true)
}
pub extern "win64" fn msi_set_target_path_a(h: usize, folder: usize, path: usize) -> u32 {
    set_property(h, folder, path, false)
}

fn evaluate(h: usize, cond: usize, wide: bool) -> i32 {
    if !is_session(h) {
        return 3; // MSICONDITION_ERROR
    }
    let cond = unsafe { text(cond, wide) };
    if cond.trim().is_empty() {
        return 2; // MSICONDITION_NONE
    }
    let ctx = Context {
        props: with(|s| s.props.clone()),
        ..Default::default()
    };
    ctx.eval(&cond) as i32
}

pub extern "win64" fn msi_evaluate_condition_w(h: usize, cond: usize) -> i32 {
    evaluate(h, cond, true)
}
pub extern "win64" fn msi_evaluate_condition_a(h: usize, cond: usize) -> i32 {
    evaluate(h, cond, false)
}

// ---------- enregistrements ----------

pub extern "win64" fn msi_create_record(count: u32) -> u32 {
    if count > 65535 {
        return 0;
    }
    with(|s| {
        let h = s.next;
        s.next += 1;
        s.records.insert(h, vec![Field::Null; count as usize + 1]);
        h
    })
}

fn field_mut<T>(rec: usize, field: u32, f: impl FnOnce(&mut Field) -> T) -> Option<T> {
    with(|s| {
        s.records
            .get_mut(&(rec as u32))
            .and_then(|r| r.get_mut(field as usize))
            .map(f)
    })
}

fn record_set_string(rec: usize, field: u32, value: usize, wide: bool) -> u32 {
    let value = unsafe { text(value, wide) };
    let v = if value.is_empty() {
        Field::Null
    } else {
        Field::Str(value)
    };
    match field_mut(rec, field, |f| *f = v) {
        Some(()) => ERROR_SUCCESS,
        None if with(|s| s.records.contains_key(&(rec as u32))) => ERROR_INVALID_PARAMETER,
        None => ERROR_INVALID_HANDLE,
    }
}

pub extern "win64" fn msi_record_set_string_w(rec: usize, field: u32, value: usize) -> u32 {
    record_set_string(rec, field, value, true)
}
pub extern "win64" fn msi_record_set_string_a(rec: usize, field: u32, value: usize) -> u32 {
    record_set_string(rec, field, value, false)
}

pub extern "win64" fn msi_record_set_integer(rec: usize, field: u32, value: i32) -> u32 {
    let v = if value == MSI_NULL_INTEGER {
        Field::Null
    } else {
        Field::Int(value)
    };
    match field_mut(rec, field, |f| *f = v) {
        Some(()) => ERROR_SUCCESS,
        None => ERROR_INVALID_PARAMETER,
    }
}

fn field_text(f: &Field) -> String {
    match f {
        Field::Null => String::new(),
        Field::Int(i) => i.to_string(),
        Field::Str(s) => s.clone(),
        Field::Stream(..) => String::new(),
    }
}

fn record_get_string(rec: usize, field: u32, buf: usize, pcch: usize, wide: bool) -> u32 {
    let Some(value) = field_mut(rec, field, |f| field_text(f)) else {
        return ERROR_INVALID_HANDLE;
    };
    unsafe { put(&value, buf, pcch, wide) }
}

pub extern "win64" fn msi_record_get_string_w(
    rec: usize,
    field: u32,
    buf: usize,
    pcch: usize,
) -> u32 {
    record_get_string(rec, field, buf, pcch, true)
}
pub extern "win64" fn msi_record_get_string_a(
    rec: usize,
    field: u32,
    buf: usize,
    pcch: usize,
) -> u32 {
    record_get_string(rec, field, buf, pcch, false)
}

pub extern "win64" fn msi_record_get_integer(rec: usize, field: u32) -> i32 {
    field_mut(rec, field, |f| match f {
        Field::Int(i) => *i,
        Field::Str(s) => s.trim().parse().unwrap_or(MSI_NULL_INTEGER),
        Field::Null | Field::Stream(..) => MSI_NULL_INTEGER,
    })
    .unwrap_or(MSI_NULL_INTEGER)
}

/// MsiRecordReadStream(enregistrement, champ, tampon, *taille) : lit la
/// suite du flux ; tampon nul = octets restants dans `*taille`.
pub extern "win64" fn msi_record_read_stream(
    rec: usize,
    field: u32,
    buf: usize,
    size: usize,
) -> u32 {
    if size == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let want = unsafe { *(size as *const u32) } as usize;
    let got = field_mut(rec, field, |f| match f {
        Field::Stream(data, pos) => {
            let rest = data.len().saturating_sub(*pos);
            if buf == 0 {
                return Ok(rest);
            }
            let n = rest.min(want);
            unsafe { std::ptr::copy_nonoverlapping(data[*pos..].as_ptr(), buf as *mut u8, n) };
            *pos += n;
            Ok(n)
        }
        // ERROR_DATATYPE_MISMATCH
        _ => Err(1629u32),
    });
    match got {
        Some(Ok(n)) => {
            unsafe { *(size as *mut u32) = n as u32 };
            ERROR_SUCCESS
        }
        Some(Err(e)) => e,
        None => ERROR_INVALID_HANDLE,
    }
}

/// MsiRecordSetStream(enregistrement, champ, fichier) : le flux est lu
/// depuis le fichier ; chemin nul = remise à zéro de la lecture.
fn record_set_stream(rec: usize, field: u32, path: usize, wide: bool) -> u32 {
    if path == 0 {
        return match field_mut(rec, field, |f| {
            if let Field::Stream(_, pos) = f {
                *pos = 0;
            }
        }) {
            Some(()) => ERROR_SUCCESS,
            None => ERROR_INVALID_HANDLE,
        };
    }
    let path = unsafe { text(path, wide) };
    let Ok(data) = std::fs::read(crate::paths::to_linux(&path)) else {
        return 2; // ERROR_FILE_NOT_FOUND
    };
    match field_mut(rec, field, |f| *f = Field::Stream(data, 0)) {
        Some(()) => ERROR_SUCCESS,
        None => ERROR_INVALID_PARAMETER,
    }
}

pub extern "win64" fn msi_record_set_stream_w(rec: usize, field: u32, path: usize) -> u32 {
    record_set_stream(rec, field, path, true)
}
pub extern "win64" fn msi_record_set_stream_a(rec: usize, field: u32, path: usize) -> u32 {
    record_set_stream(rec, field, path, false)
}

/// MsiRecordDataSize : octets d'un flux, caractères d'une chaîne, 4 pour
/// un entier, 0 pour un champ nul.
pub extern "win64" fn msi_record_data_size(rec: usize, field: u32) -> u32 {
    field_mut(rec, field, |f| match f {
        Field::Null => 0,
        Field::Int(_) => 4,
        Field::Str(s) => s.encode_utf16().count() as u32,
        Field::Stream(d, _) => d.len() as u32,
    })
    .unwrap_or(0)
}

pub extern "win64" fn msi_record_get_field_count(rec: usize) -> u32 {
    with(|s| {
        s.records
            .get(&(rec as u32))
            .map(|r| r.len() as u32 - 1)
            .unwrap_or(u32::MAX)
    })
}

pub extern "win64" fn msi_record_is_null(rec: usize, field: u32) -> i32 {
    field_mut(rec, field, |f| *f == Field::Null).unwrap_or(true) as i32
}

pub extern "win64" fn msi_record_clear_data(rec: usize) -> u32 {
    with(|s| match s.records.get_mut(&(rec as u32)) {
        Some(r) => {
            r.iter_mut().for_each(|f| *f = Field::Null);
            ERROR_SUCCESS
        }
        None => ERROR_INVALID_HANDLE,
    })
}

pub extern "win64" fn msi_close_handle(h: usize) -> u32 {
    with(|s| {
        s.records.remove(&(h as u32));
    });
    super::session_db::close(h as u32);
    super::summary::close(h as u32);
    ERROR_SUCCESS
}

pub extern "win64" fn msi_close_all_handles() -> u32 {
    with(|s| std::mem::take(&mut s.records).len() as u32)
}

/// Texte d'un enregistrement : le modèle du champ 0, où `[n]` est le champ
/// n et `[Propriété]` une propriété ; sans modèle, « 1: … 2: … ».
fn format_record(rec: u32) -> Option<String> {
    let (fields, props) = with(|s| (s.records.get(&rec).cloned(), s.props.clone()));
    let fields = fields?;
    let template = field_text(&fields[0]);
    if template.is_empty() {
        return Some(
            fields
                .iter()
                .enumerate()
                .skip(1)
                .filter(|(_, f)| **f != Field::Null)
                .map(|(i, f)| format!("{i}: {}", field_text(f)))
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let mut ctx = Context {
        props,
        ..Default::default()
    };
    // Les champs numérotés passent par des propriétés au nom impossible.
    let mut t = template;
    for (i, f) in fields.iter().enumerate().skip(1).rev() {
        let key = format!("\u{1}{i}");
        t = t.replace(&format!("[{i}]"), &format!("[{key}]"));
        ctx.props.insert(key, field_text(f));
    }
    Some(ctx.format(&t))
}

fn format_record_into(h: usize, rec: usize, buf: usize, pcch: usize, wide: bool) -> u32 {
    let _ = h;
    let Some(value) = format_record(rec as u32) else {
        return ERROR_INVALID_HANDLE;
    };
    unsafe { put(&value, buf, pcch, wide) }
}

pub extern "win64" fn msi_format_record_w(h: usize, rec: usize, buf: usize, pcch: usize) -> u32 {
    format_record_into(h, rec, buf, pcch, true)
}
pub extern "win64" fn msi_format_record_a(h: usize, rec: usize, buf: usize, pcch: usize) -> u32 {
    format_record_into(h, rec, buf, pcch, false)
}

/// MsiProcessMessage : le message est écrit dans le journal du msiexec
/// (sortie d'erreur, relue par le processus parent).
pub extern "win64" fn msi_process_message(h: usize, kind: i32, rec: usize) -> i32 {
    if !is_session(h) {
        return -1;
    }
    let kind = (kind as u32) & 0xFF00_0000;
    // Progression, données d'action, début/fin d'action : rien à écrire.
    if matches!(kind, 0x0A00_0000 | 0x0900_0000 | 0x0800_0000 | 0x0B00_0000) {
        return 0;
    }
    if let Some(text) = format_record(rec as u32) {
        if !text.trim().is_empty() {
            eprintln!("msi.dll : {}", text.trim());
        }
    }
    match kind {
        0x0100_0000 | 0x0200_0000 | 0x0300_0000 => 1, // erreur, avertissement : IDOK
        _ => 0,
    }
}

pub extern "win64" fn msi_get_last_error_record() -> u32 {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "MsiGetPropertyW" => fn msi_get_property_w,
        "MsiGetPropertyA" => fn msi_get_property_a,
        "MsiSetPropertyW" => fn msi_set_property_w,
        "MsiSetPropertyA" => fn msi_set_property_a,
        "MsiGetMode" => fn msi_get_mode,
        "MsiSetMode" => fn msi_set_mode,
        "MsiGetLanguage" => fn msi_get_language,
        "MsiGetComponentStateW" => fn msi_get_state,
        "MsiGetComponentStateA" => fn msi_get_state,
        "MsiGetFeatureStateW" => fn msi_get_state,
        "MsiGetFeatureStateA" => fn msi_get_state,
        "MsiSetComponentStateW" => fn msi_set_state,
        "MsiSetComponentStateA" => fn msi_set_state,
        "MsiSetFeatureStateW" => fn msi_set_state,
        "MsiSetFeatureStateA" => fn msi_set_state,
        "MsiDoActionW" => fn msi_do_action_w,
        "MsiDoActionA" => fn msi_do_action_a,
        "MsiSequenceW" => fn msi_sequence,
        "MsiSequenceA" => fn msi_sequence,
        "MsiGetTargetPathW" => fn msi_get_target_path_w,
        "MsiGetTargetPathA" => fn msi_get_target_path_a,
        "MsiGetSourcePathW" => fn msi_get_target_path_w,
        "MsiGetSourcePathA" => fn msi_get_target_path_a,
        "MsiSetTargetPathW" => fn msi_set_target_path_w,
        "MsiSetTargetPathA" => fn msi_set_target_path_a,
        "MsiEvaluateConditionW" => fn msi_evaluate_condition_w,
        "MsiEvaluateConditionA" => fn msi_evaluate_condition_a,
        "MsiCreateRecord" => fn msi_create_record,
        "MsiRecordSetStringW" => fn msi_record_set_string_w,
        "MsiRecordSetStringA" => fn msi_record_set_string_a,
        "MsiRecordGetStringW" => fn msi_record_get_string_w,
        "MsiRecordGetStringA" => fn msi_record_get_string_a,
        "MsiRecordSetInteger" => fn msi_record_set_integer,
        "MsiRecordGetInteger" => fn msi_record_get_integer,
        "MsiRecordReadStream" => fn msi_record_read_stream,
        "MsiRecordSetStreamW" => fn msi_record_set_stream_w,
        "MsiRecordSetStreamA" => fn msi_record_set_stream_a,
        "MsiRecordDataSize" => fn msi_record_data_size,
        "MsiRecordGetFieldCount" => fn msi_record_get_field_count,
        "MsiRecordIsNull" => fn msi_record_is_null,
        "MsiRecordClearData" => fn msi_record_clear_data,
        "MsiCloseHandle" => fn msi_close_handle,
        "MsiCloseAllHandles" => fn msi_close_all_handles,
        "MsiFormatRecordW" => fn msi_format_record_w,
        "MsiFormatRecordA" => fn msi_format_record_a,
        "MsiProcessMessage" => fn msi_process_message,
        "MsiGetLastErrorRecord" => fn msi_get_last_error_record,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_file_round_trip() {
        let mut p = HashMap::new();
        p.insert("CustomActionData".to_string(), "a\tb\\c\nd".to_string());
        p.insert(
            "INSTALLDIR".to_string(),
            "C:\\Program Files\\X\\".to_string(),
        );
        assert_eq!(decode(&encode(&p)), p);
    }
}
