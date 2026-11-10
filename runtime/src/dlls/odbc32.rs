//! odbc32 : gestionnaire de pilotes ODBC (#1392).
//!
//! Le module SQL ODBC de Qt 6 (qsqlodbc) et `odbc.pyd` de pywin32
//! importent odbc32 uniquement par ordinal. Winlux n'a encore aucun pilote
//! ODBC : la DLL répond comme un gestionnaire de pilotes sans pilote ni
//! source de données.
//! - les environnements et les connexions s'allouent, se règlent et se
//!   libèrent normalement ;
//! - toute connexion échoue avec IM002 (source de données introuvable et
//!   aucun pilote par défaut), relue par SQLGetDiagRec, SQLGetDiagField ou
//!   SQLError ;
//! - SQLDrivers et SQLDataSources ne listent rien (SQL_NO_DATA) ;
//! - aucune instruction ni aucun descripteur ne peut exister : leurs
//!   fonctions rendent SQL_INVALID_HANDLE.
//!
//! Provenance : noms et signatures d'après la documentation publique
//! d'ODBC et `sql.h`, `sqlext.h`, `sqlucode.h` ; ordinaux d'après la
//! bibliothèque d'importation `odbc32.lib` du SDK Windows public (paquet
//! NuGet Microsoft.Windows.SDK.CPP.x64 10.0.26100.4654), où chaque
//! fonction est importée par numéro. Plus tard, ces fonctions pourront
//! passer par unixODBC s'il est présent.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

type SqlReturn = i16;
const SQL_SUCCESS: SqlReturn = 0;
const SQL_SUCCESS_WITH_INFO: SqlReturn = 1;
const SQL_ERROR: SqlReturn = -1;
const SQL_INVALID_HANDLE: SqlReturn = -2;
const SQL_NO_DATA: SqlReturn = 100;

const SQL_HANDLE_ENV: i16 = 1;
const SQL_HANDLE_DBC: i16 = 2;
const SQL_HANDLE_STMT: i16 = 3;
const SQL_HANDLE_DESC: i16 = 4;

const SQL_ATTR_ODBC_VERSION: i32 = 200;
const SQL_ATTR_OUTPUT_NTS: i32 = 10001;
const SQL_OV_ODBC2: u32 = 2;
const SQL_OV_ODBC3: u32 = 3;

const SQL_DIAG_NUMBER: i16 = 2;
const SQL_DIAG_SQLSTATE: i16 = 4;
const SQL_DIAG_NATIVE: i16 = 5;
const SQL_DIAG_MESSAGE_TEXT: i16 = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Diag {
    /// IM002 : source de données introuvable, aucun pilote par défaut.
    NoDriver,
    /// 08003 : connexion non ouverte.
    NotOpen,
    /// HY010 : erreur de séquence de fonctions.
    Sequence,
    /// HY009 : pointeur nul invalide.
    NullPointer,
}

impl Diag {
    fn state(self) -> &'static str {
        match self {
            Diag::NoDriver => "IM002",
            Diag::NotOpen => "08003",
            Diag::Sequence => "HY010",
            Diag::NullPointer => "HY009",
        }
    }

    fn message(self) -> &'static str {
        use crate::dlls::comdlg32_ui::tr;
        match self {
            Diag::NoDriver => tr(
                "[Winlux][Gestionnaire ODBC] Source de données introuvable et aucun pilote par défaut spécifié",
                "[Winlux][ODBC Manager] Data source name not found and no default driver specified",
                "[Winlux][ODBC-Manager] Datenquellenname nicht gefunden und kein Standardtreiber angegeben",
                "[Winlux][Administrador ODBC] No se encontró el nombre del origen de datos y no se especificó ningún controlador predeterminado",
            ),
            Diag::NotOpen => tr(
                "[Winlux][Gestionnaire ODBC] Connexion non ouverte",
                "[Winlux][ODBC Manager] Connection not open",
                "[Winlux][ODBC-Manager] Verbindung nicht geöffnet",
                "[Winlux][Administrador ODBC] Conexión no abierta",
            ),
            Diag::Sequence => tr(
                "[Winlux][Gestionnaire ODBC] Erreur de séquence de fonctions",
                "[Winlux][ODBC Manager] Function sequence error",
                "[Winlux][ODBC-Manager] Funktionsfolgefehler",
                "[Winlux][Administrador ODBC] Error de secuencia de funciones",
            ),
            Diag::NullPointer => tr(
                "[Winlux][Gestionnaire ODBC] Pointeur nul invalide",
                "[Winlux][ODBC Manager] Invalid use of null pointer",
                "[Winlux][ODBC-Manager] Ungültige Verwendung eines Nullzeigers",
                "[Winlux][Administrador ODBC] Uso no válido de un puntero nulo",
            ),
        }
    }
}

struct Handle {
    kind: i16,
    parent: usize,
    version: u32,
    diag: Option<Diag>,
}

fn table() -> MutexGuard<'static, HashMap<usize, Handle>> {
    static T: OnceLock<Mutex<HashMap<usize, Handle>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Nouvelle poignée, multiple de 4 et sous 4 Gio (programmes x86, #1360).
fn new_handle(kind: i16, parent: usize, version: u32) -> usize {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let h = 0x5D00_0000 + 4 * (n % 0x0400_0000);
    table().insert(
        h,
        Handle {
            kind,
            parent,
            version,
            diag: None,
        },
    );
    h
}

/// Début d'une fonction sur `h` : vérifie son genre et efface son
/// diagnostic précédent, comme le fait chaque appel ODBC.
fn enter(h: usize, kind: i16) -> bool {
    match table().get_mut(&h) {
        Some(e) if e.kind == kind => {
            e.diag = None;
            true
        }
        _ => false,
    }
}

fn fail(h: usize, diag: Diag) -> SqlReturn {
    if let Some(e) = table().get_mut(&h) {
        e.diag = Some(diag);
    }
    SQL_ERROR
}

unsafe fn put_handle(out: *mut usize, h: usize) {
    if !out.is_null() {
        crate::wow32::write_ptr(out as usize, h);
    }
}

// ── Allocation et libération ───────────────────────────────────────────

pub unsafe extern "win64" fn sql_alloc_handle(
    kind: i16,
    input: usize,
    out: *mut usize,
) -> SqlReturn {
    alloc(kind, input, out, 0)
}

/// SQLAllocHandleStd : comme SQLAllocHandle, l'environnement est déjà en
/// ODBC 3.
pub unsafe extern "win64" fn sql_alloc_handle_std(
    kind: i16,
    input: usize,
    out: *mut usize,
) -> SqlReturn {
    alloc(kind, input, out, SQL_OV_ODBC3)
}

unsafe fn alloc(kind: i16, input: usize, out: *mut usize, env_version: u32) -> SqlReturn {
    match kind {
        SQL_HANDLE_ENV => {
            if out.is_null() {
                return SQL_ERROR;
            }
            put_handle(out, new_handle(SQL_HANDLE_ENV, 0, env_version));
            SQL_SUCCESS
        }
        SQL_HANDLE_DBC => {
            if !enter(input, SQL_HANDLE_ENV) {
                put_handle(out, 0);
                return SQL_INVALID_HANDLE;
            }
            if out.is_null() {
                return fail(input, Diag::NullPointer);
            }
            let version = table().get(&input).map_or(0, |e| e.version);
            if version == 0 {
                // SQL_ATTR_ODBC_VERSION doit être réglé avant.
                put_handle(out, 0);
                return fail(input, Diag::Sequence);
            }
            put_handle(out, new_handle(SQL_HANDLE_DBC, input, version));
            SQL_SUCCESS
        }
        SQL_HANDLE_STMT | SQL_HANDLE_DESC => {
            put_handle(out, 0);
            if !enter(input, SQL_HANDLE_DBC) {
                return SQL_INVALID_HANDLE;
            }
            // Aucune connexion ne peut être ouverte.
            fail(input, Diag::NotOpen)
        }
        _ => {
            put_handle(out, 0);
            SQL_ERROR
        }
    }
}

/// SQLAllocEnv (ODBC 2) : environnement réglé en ODBC 2.
pub unsafe extern "win64" fn sql_alloc_env(out: *mut usize) -> SqlReturn {
    alloc(SQL_HANDLE_ENV, 0, out, SQL_OV_ODBC2)
}

pub unsafe extern "win64" fn sql_alloc_connect(env: usize, out: *mut usize) -> SqlReturn {
    alloc(SQL_HANDLE_DBC, env, out, 0)
}

pub unsafe extern "win64" fn sql_alloc_stmt(dbc: usize, out: *mut usize) -> SqlReturn {
    alloc(SQL_HANDLE_STMT, dbc, out, 0)
}

pub extern "win64" fn sql_free_handle(kind: i16, h: usize) -> SqlReturn {
    let mut t = table();
    match t.get(&h) {
        Some(e) if e.kind == kind => {}
        _ => return SQL_INVALID_HANDLE,
    }
    if kind == SQL_HANDLE_ENV && t.values().any(|e| e.parent == h) {
        // Connexions encore allouées sur cet environnement.
        if let Some(e) = t.get_mut(&h) {
            e.diag = Some(Diag::Sequence);
        }
        return SQL_ERROR;
    }
    t.remove(&h);
    SQL_SUCCESS
}

pub extern "win64" fn sql_free_env(env: usize) -> SqlReturn {
    sql_free_handle(SQL_HANDLE_ENV, env)
}

pub extern "win64" fn sql_free_connect(dbc: usize) -> SqlReturn {
    sql_free_handle(SQL_HANDLE_DBC, dbc)
}

// ── Environnement ──────────────────────────────────────────────────────

pub extern "win64" fn sql_set_env_attr(
    env: usize,
    attr: i32,
    value: usize,
    _len: i32,
) -> SqlReturn {
    if !enter(env, SQL_HANDLE_ENV) {
        return SQL_INVALID_HANDLE;
    }
    if attr == SQL_ATTR_ODBC_VERSION {
        if let Some(e) = table().get_mut(&env) {
            e.version = value as u32;
        }
    }
    SQL_SUCCESS
}

pub unsafe extern "win64" fn sql_get_env_attr(
    env: usize,
    attr: i32,
    value: *mut u32,
    _len: i32,
    out_len: *mut i32,
) -> SqlReturn {
    if !enter(env, SQL_HANDLE_ENV) {
        return SQL_INVALID_HANDLE;
    }
    let v = match attr {
        SQL_ATTR_ODBC_VERSION => table().get(&env).map_or(0, |e| e.version),
        SQL_ATTR_OUTPUT_NTS => 1,
        _ => 0,
    };
    if !value.is_null() {
        std::ptr::write_unaligned(value, v);
    }
    if !out_len.is_null() {
        std::ptr::write_unaligned(out_len, 4);
    }
    SQL_SUCCESS
}

/// SQLDrivers et SQLDataSources (A, W) : aucun pilote, aucune source.
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sql_list_none(
    env: usize,
    _direction: u16,
    _name: usize,
    _name_max: i16,
    _name_len: *mut i16,
    _desc: usize,
    _desc_max: i16,
    _desc_len: *mut i16,
) -> SqlReturn {
    if !enter(env, SQL_HANDLE_ENV) {
        return SQL_INVALID_HANDLE;
    }
    SQL_NO_DATA
}

// ── Connexion ──────────────────────────────────────────────────────────

/// SQLConnect (A, W) : aucune source de données n'existe.
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sql_connect(
    dbc: usize,
    _dsn: usize,
    _dsn_len: i16,
    _user: usize,
    _user_len: i16,
    _auth: usize,
    _auth_len: i16,
) -> SqlReturn {
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    fail(dbc, Diag::NoDriver)
}

/// SQLDriverConnect (A, W) : aucun pilote, même avec une fenêtre parente.
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sql_driver_connect(
    dbc: usize,
    _hwnd: usize,
    _input: usize,
    _input_len: i16,
    _output: usize,
    _output_max: i16,
    output_len: *mut i16,
    _completion: u16,
) -> SqlReturn {
    if !output_len.is_null() {
        // SAFETY : pointeur de sortie fourni par l'appelant.
        unsafe { std::ptr::write_unaligned(output_len, 0) };
    }
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    fail(dbc, Diag::NoDriver)
}

/// SQLBrowseConnect (A, W).
pub extern "win64" fn sql_browse_connect(
    dbc: usize,
    _input: usize,
    _input_len: i16,
    _output: usize,
    _output_max: i16,
    _output_len: *mut i16,
) -> SqlReturn {
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    fail(dbc, Diag::NoDriver)
}

pub extern "win64" fn sql_disconnect(dbc: usize) -> SqlReturn {
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    fail(dbc, Diag::NotOpen)
}

/// Attributs réglés avant la connexion : acceptés (le gestionnaire de
/// pilotes les garde pour le pilote).
pub extern "win64" fn sql_set_connect_attr(
    dbc: usize,
    _attr: i32,
    _value: usize,
    _len: i32,
) -> SqlReturn {
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    SQL_SUCCESS
}

pub extern "win64" fn sql_set_connect_option(dbc: usize, _option: u16, _value: usize) -> SqlReturn {
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    SQL_SUCCESS
}

/// Fonctions qui demandent une connexion ouverte, à trois arguments :
/// SQLGetConnectOption, SQLGetFunctions.
pub extern "win64" fn sql_dbc_not_open_3(dbc: usize, _a: usize, _b: usize) -> SqlReturn {
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    fail(dbc, Diag::NotOpen)
}

/// À cinq arguments : SQLGetConnectAttr, SQLGetInfo (A, W).
pub extern "win64" fn sql_dbc_not_open_5(
    dbc: usize,
    _a: i32,
    _b: usize,
    _c: i32,
    _d: usize,
) -> SqlReturn {
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    fail(dbc, Diag::NotOpen)
}

/// SQLNativeSql (A, W).
pub extern "win64" fn sql_native_sql(
    dbc: usize,
    _input: usize,
    _input_len: i32,
    _output: usize,
    _output_max: i32,
    _output_len: *mut i32,
) -> SqlReturn {
    if !enter(dbc, SQL_HANDLE_DBC) {
        return SQL_INVALID_HANDLE;
    }
    fail(dbc, Diag::NotOpen)
}

/// SQLEndTran : rien à valider sur un environnement sans connexion.
pub extern "win64" fn sql_end_tran(kind: i16, h: usize, _completion: i16) -> SqlReturn {
    match kind {
        SQL_HANDLE_ENV if enter(h, SQL_HANDLE_ENV) => SQL_SUCCESS,
        SQL_HANDLE_DBC if enter(h, SQL_HANDLE_DBC) => fail(h, Diag::NotOpen),
        _ => SQL_INVALID_HANDLE,
    }
}

pub extern "win64" fn sql_transact(env: usize, dbc: usize, completion: u16) -> SqlReturn {
    if dbc != 0 {
        sql_end_tran(SQL_HANDLE_DBC, dbc, completion as i16)
    } else {
        sql_end_tran(SQL_HANDLE_ENV, env, completion as i16)
    }
}

// ── Diagnostics ────────────────────────────────────────────────────────

/// Écrit `text` (terminé par NUL) dans un tampon de `max` unités ; rend la
/// longueur complète en unités et vrai si le texte a été tronqué.
unsafe fn write_text(text: &str, buf: usize, max: usize, wide: bool) -> (usize, bool) {
    if wide {
        let units: Vec<u16> = text.encode_utf16().collect();
        if buf != 0 && max > 0 {
            let n = units.len().min(max - 1);
            let p = buf as *mut u16;
            for (i, u) in units[..n].iter().enumerate() {
                std::ptr::write_unaligned(p.add(i), *u);
            }
            std::ptr::write_unaligned(p.add(n), 0);
        }
        (units.len(), units.len() >= max)
    } else {
        // Page de code du programme : on garde l'ASCII, le reste devient « ? ».
        let bytes: Vec<u8> = text
            .chars()
            .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
            .collect();
        if buf != 0 && max > 0 {
            let n = bytes.len().min(max - 1);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
            *((buf + n) as *mut u8) = 0;
        }
        (bytes.len(), bytes.len() >= max)
    }
}

/// Enregistrement de diagnostic : état sur 5 caractères, code natif 0,
/// message.
#[allow(clippy::too_many_arguments)]
unsafe fn diag_record(
    diag: Diag,
    state: usize,
    native: *mut i32,
    msg: usize,
    max: i16,
    len: *mut i16,
    wide: bool,
) -> SqlReturn {
    if state != 0 {
        write_text(diag.state(), state, 6, wide);
    }
    if !native.is_null() {
        std::ptr::write_unaligned(native, 0);
    }
    let (n, cut) = write_text(diag.message(), msg, max.max(0) as usize, wide);
    if !len.is_null() {
        std::ptr::write_unaligned(len, n as i16);
    }
    if cut && msg != 0 {
        SQL_SUCCESS_WITH_INFO
    } else {
        SQL_SUCCESS
    }
}

fn diag_of(kind: i16, h: usize) -> Result<Option<Diag>, SqlReturn> {
    match table().get(&h) {
        Some(e) if e.kind == kind => Ok(e.diag),
        _ => Err(SQL_INVALID_HANDLE),
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn get_diag_rec(
    kind: i16,
    h: usize,
    rec: i16,
    state: usize,
    native: *mut i32,
    msg: usize,
    max: i16,
    len: *mut i16,
    wide: bool,
) -> SqlReturn {
    let diag = match diag_of(kind, h) {
        Ok(d) => d,
        Err(rc) => return rc,
    };
    if rec < 1 {
        return SQL_ERROR;
    }
    match diag {
        Some(d) if rec == 1 => diag_record(d, state, native, msg, max, len, wide),
        _ => SQL_NO_DATA,
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sql_get_diag_rec_a(
    kind: i16,
    h: usize,
    rec: i16,
    state: usize,
    native: *mut i32,
    msg: usize,
    max: i16,
    len: *mut i16,
) -> SqlReturn {
    get_diag_rec(kind, h, rec, state, native, msg, max, len, false)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sql_get_diag_rec_w(
    kind: i16,
    h: usize,
    rec: i16,
    state: usize,
    native: *mut i32,
    msg: usize,
    max: i16,
    len: *mut i16,
) -> SqlReturn {
    get_diag_rec(kind, h, rec, state, native, msg, max, len, true)
}

#[allow(clippy::too_many_arguments)]
unsafe fn get_diag_field(
    kind: i16,
    h: usize,
    rec: i16,
    id: i16,
    buf: usize,
    max: i16,
    len: *mut i16,
    wide: bool,
) -> SqlReturn {
    let diag = match diag_of(kind, h) {
        Ok(d) => d,
        Err(rc) => return rc,
    };
    if id == SQL_DIAG_NUMBER {
        if buf != 0 {
            std::ptr::write_unaligned(buf as *mut i32, i32::from(diag.is_some()));
        }
        return SQL_SUCCESS;
    }
    let Some(d) = diag.filter(|_| rec == 1) else {
        return SQL_NO_DATA;
    };
    let unit = if wide { 2 } else { 1 };
    let text = match id {
        SQL_DIAG_NATIVE => {
            if buf != 0 {
                std::ptr::write_unaligned(buf as *mut i32, 0);
            }
            return SQL_SUCCESS;
        }
        SQL_DIAG_SQLSTATE => d.state(),
        SQL_DIAG_MESSAGE_TEXT => d.message(),
        _ => return SQL_ERROR,
    };
    // Longueurs en octets pour SQLGetDiagField, même en W.
    let (n, cut) = write_text(text, buf, max.max(0) as usize / unit, wide);
    if !len.is_null() {
        std::ptr::write_unaligned(len, (n * unit) as i16);
    }
    if cut && buf != 0 {
        SQL_SUCCESS_WITH_INFO
    } else {
        SQL_SUCCESS
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sql_get_diag_field_a(
    kind: i16,
    h: usize,
    rec: i16,
    id: i16,
    buf: usize,
    max: i16,
    len: *mut i16,
) -> SqlReturn {
    get_diag_field(kind, h, rec, id, buf, max, len, false)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sql_get_diag_field_w(
    kind: i16,
    h: usize,
    rec: i16,
    id: i16,
    buf: usize,
    max: i16,
    len: *mut i16,
) -> SqlReturn {
    get_diag_field(kind, h, rec, id, buf, max, len, true)
}

/// SQLError (ODBC 2) : rend le diagnostic de la poignée la plus précise
/// fournie, puis l'efface.
#[allow(clippy::too_many_arguments)]
unsafe fn error(
    env: usize,
    dbc: usize,
    stmt: usize,
    state: usize,
    native: *mut i32,
    msg: usize,
    max: i16,
    len: *mut i16,
    wide: bool,
) -> SqlReturn {
    let (kind, h) = if stmt != 0 {
        (SQL_HANDLE_STMT, stmt)
    } else if dbc != 0 {
        (SQL_HANDLE_DBC, dbc)
    } else {
        (SQL_HANDLE_ENV, env)
    };
    let diag = match diag_of(kind, h) {
        Ok(d) => d,
        Err(rc) => return rc,
    };
    let Some(d) = diag else {
        return SQL_NO_DATA;
    };
    if let Some(e) = table().get_mut(&h) {
        e.diag = None;
    }
    diag_record(d, state, native, msg, max, len, wide)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sql_error_a(
    env: usize,
    dbc: usize,
    stmt: usize,
    state: usize,
    native: *mut i32,
    msg: usize,
    max: i16,
    len: *mut i16,
) -> SqlReturn {
    error(env, dbc, stmt, state, native, msg, max, len, false)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn sql_error_w(
    env: usize,
    dbc: usize,
    stmt: usize,
    state: usize,
    native: *mut i32,
    msg: usize,
    max: i16,
    len: *mut i16,
) -> SqlReturn {
    error(env, dbc, stmt, state, native, msg, max, len, true)
}

// ── Instructions et descripteurs ───────────────────────────────────────
// Aucun ne peut exister : chaque fonction rend SQL_INVALID_HANDLE. Une
// fonction par nombre d'arguments, pour que la passerelle x86 (stdcall)
// dépile le bon nombre d'octets.

pub extern "win64" fn sans_instruction_1(_h: usize) -> SqlReturn {
    SQL_INVALID_HANDLE
}
pub extern "win64" fn sans_instruction_2(_h: usize, _a: usize) -> SqlReturn {
    SQL_INVALID_HANDLE
}
pub extern "win64" fn sans_instruction_3(_h: usize, _a: usize, _b: usize) -> SqlReturn {
    SQL_INVALID_HANDLE
}
pub extern "win64" fn sans_instruction_4(_h: usize, _a: usize, _b: usize, _c: usize) -> SqlReturn {
    SQL_INVALID_HANDLE
}
pub extern "win64" fn sans_instruction_5(
    _h: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
) -> SqlReturn {
    SQL_INVALID_HANDLE
}
pub extern "win64" fn sans_instruction_6(
    _h: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
) -> SqlReturn {
    SQL_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sans_instruction_7(
    _h: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
) -> SqlReturn {
    SQL_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sans_instruction_8(
    _h: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
) -> SqlReturn {
    SQL_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sans_instruction_9(
    _h: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
    _i: usize,
) -> SqlReturn {
    SQL_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sans_instruction_10(
    _h: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
    _i: usize,
    _j: usize,
) -> SqlReturn {
    SQL_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sans_instruction_11(
    _h: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
    _i: usize,
    _j: usize,
    _k: usize,
) -> SqlReturn {
    SQL_INVALID_HANDLE
}
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn sans_instruction_13(
    _h: usize,
    _a: usize,
    _b: usize,
    _c: usize,
    _d: usize,
    _e: usize,
    _f: usize,
    _g: usize,
    _i: usize,
    _j: usize,
    _k: usize,
    _l: usize,
    _m: usize,
) -> SqlReturn {
    SQL_INVALID_HANDLE
}

/// Ordinaux publics d'odbc32 (bibliothèque d'importation du SDK Windows) :
/// les variantes W valent 100 de plus que la fonction de base, les
/// variantes A 200 de plus.
const ORDINALS: &[(u16, &str)] = &[
    (1, "SQLAllocConnect"),
    (2, "SQLAllocEnv"),
    (3, "SQLAllocStmt"),
    (4, "SQLBindCol"),
    (5, "SQLCancel"),
    (6, "SQLColAttributes"),
    (7, "SQLConnect"),
    (8, "SQLDescribeCol"),
    (9, "SQLDisconnect"),
    (10, "SQLError"),
    (11, "SQLExecDirect"),
    (12, "SQLExecute"),
    (13, "SQLFetch"),
    (14, "SQLFreeConnect"),
    (15, "SQLFreeEnv"),
    (16, "SQLFreeStmt"),
    (17, "SQLGetCursorName"),
    (18, "SQLNumResultCols"),
    (19, "SQLPrepare"),
    (20, "SQLRowCount"),
    (21, "SQLSetCursorName"),
    (22, "SQLSetParam"),
    (23, "SQLTransact"),
    (24, "SQLAllocHandle"),
    (25, "SQLBindParam"),
    (26, "SQLCloseCursor"),
    (27, "SQLColAttribute"),
    (28, "SQLCopyDesc"),
    (29, "SQLEndTran"),
    (30, "SQLFetchScroll"),
    (31, "SQLFreeHandle"),
    (32, "SQLGetConnectAttr"),
    (33, "SQLGetDescField"),
    (34, "SQLGetDescRec"),
    (35, "SQLGetDiagField"),
    (36, "SQLGetDiagRec"),
    (37, "SQLGetEnvAttr"),
    (38, "SQLGetStmtAttr"),
    (39, "SQLSetConnectAttr"),
    (40, "SQLColumns"),
    (41, "SQLDriverConnect"),
    (42, "SQLGetConnectOption"),
    (43, "SQLGetData"),
    (44, "SQLGetFunctions"),
    (45, "SQLGetInfo"),
    (46, "SQLGetStmtOption"),
    (47, "SQLGetTypeInfo"),
    (48, "SQLParamData"),
    (49, "SQLPutData"),
    (50, "SQLSetConnectOption"),
    (51, "SQLSetStmtOption"),
    (52, "SQLSpecialColumns"),
    (53, "SQLStatistics"),
    (54, "SQLTables"),
    (55, "SQLBrowseConnect"),
    (56, "SQLColumnPrivileges"),
    (57, "SQLDataSources"),
    (58, "SQLDescribeParam"),
    (59, "SQLExtendedFetch"),
    (60, "SQLForeignKeys"),
    (61, "SQLMoreResults"),
    (62, "SQLNativeSql"),
    (63, "SQLNumParams"),
    (64, "SQLParamOptions"),
    (65, "SQLPrimaryKeys"),
    (66, "SQLProcedureColumns"),
    (67, "SQLProcedures"),
    (68, "SQLSetPos"),
    (69, "SQLSetScrollOptions"),
    (70, "SQLTablePrivileges"),
    (71, "SQLDrivers"),
    (72, "SQLBindParameter"),
    (73, "SQLSetDescField"),
    (74, "SQLSetDescRec"),
    (75, "SQLSetEnvAttr"),
    (76, "SQLSetStmtAttr"),
    (77, "SQLAllocHandleStd"),
    (78, "SQLBulkOperations"),
];

/// Fonctions de base qui ont des variantes A et W (ordinal + 100, + 200).
const WIDE: &[u16] = &[
    6, 7, 8, 10, 11, 17, 19, 21, 27, 32, 33, 34, 35, 36, 38, 39, 40, 41, 42, 45, 47, 50, 52, 53,
    54, 55, 56, 57, 60, 62, 65, 66, 67, 70, 71, 73, 76,
];

pub fn exports() -> crate::dlls::Exports {
    let mut e = crate::exports! {
        "SQLAllocConnect" => fn sql_alloc_connect,
        "SQLAllocEnv" => fn sql_alloc_env,
        "SQLAllocStmt" => fn sql_alloc_stmt,
        "SQLAllocHandle" => fn sql_alloc_handle,
        "SQLAllocHandleStd" => fn sql_alloc_handle_std,
        "SQLFreeHandle" => fn sql_free_handle,
        "SQLFreeEnv" => fn sql_free_env,
        "SQLFreeConnect" => fn sql_free_connect,
        "SQLSetEnvAttr" => fn sql_set_env_attr,
        "SQLGetEnvAttr" => fn sql_get_env_attr,
        "SQLDrivers" => fn sql_list_none,
        "SQLDriversA" => fn sql_list_none,
        "SQLDriversW" => fn sql_list_none,
        "SQLDataSources" => fn sql_list_none,
        "SQLDataSourcesA" => fn sql_list_none,
        "SQLDataSourcesW" => fn sql_list_none,
        "SQLConnect" => fn sql_connect,
        "SQLConnectA" => fn sql_connect,
        "SQLConnectW" => fn sql_connect,
        "SQLDriverConnect" => fn sql_driver_connect,
        "SQLDriverConnectA" => fn sql_driver_connect,
        "SQLDriverConnectW" => fn sql_driver_connect,
        "SQLBrowseConnect" => fn sql_browse_connect,
        "SQLBrowseConnectA" => fn sql_browse_connect,
        "SQLBrowseConnectW" => fn sql_browse_connect,
        "SQLDisconnect" => fn sql_disconnect,
        "SQLSetConnectAttr" => fn sql_set_connect_attr,
        "SQLSetConnectAttrA" => fn sql_set_connect_attr,
        "SQLSetConnectAttrW" => fn sql_set_connect_attr,
        "SQLSetConnectOption" => fn sql_set_connect_option,
        "SQLSetConnectOptionA" => fn sql_set_connect_option,
        "SQLSetConnectOptionW" => fn sql_set_connect_option,
        "SQLGetConnectOption" => fn sql_dbc_not_open_3,
        "SQLGetConnectOptionA" => fn sql_dbc_not_open_3,
        "SQLGetConnectOptionW" => fn sql_dbc_not_open_3,
        "SQLGetFunctions" => fn sql_dbc_not_open_3,
        "SQLGetConnectAttr" => fn sql_dbc_not_open_5,
        "SQLGetConnectAttrA" => fn sql_dbc_not_open_5,
        "SQLGetConnectAttrW" => fn sql_dbc_not_open_5,
        "SQLGetInfo" => fn sql_dbc_not_open_5,
        "SQLGetInfoA" => fn sql_dbc_not_open_5,
        "SQLGetInfoW" => fn sql_dbc_not_open_5,
        "SQLNativeSql" => fn sql_native_sql,
        "SQLNativeSqlA" => fn sql_native_sql,
        "SQLNativeSqlW" => fn sql_native_sql,
        "SQLEndTran" => fn sql_end_tran,
        "SQLTransact" => fn sql_transact,
        "SQLGetDiagRec" => fn sql_get_diag_rec_a,
        "SQLGetDiagRecA" => fn sql_get_diag_rec_a,
        "SQLGetDiagRecW" => fn sql_get_diag_rec_w,
        "SQLGetDiagField" => fn sql_get_diag_field_a,
        "SQLGetDiagFieldA" => fn sql_get_diag_field_a,
        "SQLGetDiagFieldW" => fn sql_get_diag_field_w,
        "SQLError" => fn sql_error_a,
        "SQLErrorA" => fn sql_error_a,
        "SQLErrorW" => fn sql_error_w,
        // Instructions et descripteurs, par nombre d'arguments.
        "SQLCancel" => fn sans_instruction_1,
        "SQLExecute" => fn sans_instruction_1,
        "SQLFetch" => fn sans_instruction_1,
        "SQLCloseCursor" => fn sans_instruction_1,
        "SQLMoreResults" => fn sans_instruction_1,
        "SQLFreeStmt" => fn sans_instruction_2,
        "SQLNumResultCols" => fn sans_instruction_2,
        "SQLRowCount" => fn sans_instruction_2,
        "SQLCopyDesc" => fn sans_instruction_2,
        "SQLGetTypeInfo" => fn sans_instruction_2,
        "SQLGetTypeInfoA" => fn sans_instruction_2,
        "SQLGetTypeInfoW" => fn sans_instruction_2,
        "SQLParamData" => fn sans_instruction_2,
        "SQLNumParams" => fn sans_instruction_2,
        "SQLBulkOperations" => fn sans_instruction_2,
        "SQLExecDirect" => fn sans_instruction_3,
        "SQLExecDirectA" => fn sans_instruction_3,
        "SQLExecDirectW" => fn sans_instruction_3,
        "SQLPrepare" => fn sans_instruction_3,
        "SQLPrepareA" => fn sans_instruction_3,
        "SQLPrepareW" => fn sans_instruction_3,
        "SQLSetCursorName" => fn sans_instruction_3,
        "SQLSetCursorNameA" => fn sans_instruction_3,
        "SQLSetCursorNameW" => fn sans_instruction_3,
        "SQLFetchScroll" => fn sans_instruction_3,
        "SQLGetStmtOption" => fn sans_instruction_3,
        "SQLSetStmtOption" => fn sans_instruction_3,
        "SQLPutData" => fn sans_instruction_3,
        "SQLParamOptions" => fn sans_instruction_3,
        "SQLGetCursorName" => fn sans_instruction_4,
        "SQLGetCursorNameA" => fn sans_instruction_4,
        "SQLGetCursorNameW" => fn sans_instruction_4,
        "SQLSetStmtAttr" => fn sans_instruction_4,
        "SQLSetStmtAttrA" => fn sans_instruction_4,
        "SQLSetStmtAttrW" => fn sans_instruction_4,
        "SQLSetPos" => fn sans_instruction_4,
        "SQLSetScrollOptions" => fn sans_instruction_4,
        "SQLGetStmtAttr" => fn sans_instruction_5,
        "SQLGetStmtAttrA" => fn sans_instruction_5,
        "SQLGetStmtAttrW" => fn sans_instruction_5,
        "SQLSetDescField" => fn sans_instruction_5,
        "SQLSetDescFieldA" => fn sans_instruction_5,
        "SQLSetDescFieldW" => fn sans_instruction_5,
        "SQLExtendedFetch" => fn sans_instruction_5,
        "SQLBindCol" => fn sans_instruction_6,
        "SQLGetData" => fn sans_instruction_6,
        "SQLGetDescField" => fn sans_instruction_6,
        "SQLGetDescFieldA" => fn sans_instruction_6,
        "SQLGetDescFieldW" => fn sans_instruction_6,
        "SQLDescribeParam" => fn sans_instruction_6,
        "SQLColAttributes" => fn sans_instruction_7,
        "SQLColAttributesA" => fn sans_instruction_7,
        "SQLColAttributesW" => fn sans_instruction_7,
        "SQLColAttribute" => fn sans_instruction_7,
        "SQLColAttributeA" => fn sans_instruction_7,
        "SQLColAttributeW" => fn sans_instruction_7,
        "SQLPrimaryKeys" => fn sans_instruction_7,
        "SQLPrimaryKeysA" => fn sans_instruction_7,
        "SQLPrimaryKeysW" => fn sans_instruction_7,
        "SQLProcedures" => fn sans_instruction_7,
        "SQLProceduresA" => fn sans_instruction_7,
        "SQLProceduresW" => fn sans_instruction_7,
        "SQLTablePrivileges" => fn sans_instruction_7,
        "SQLTablePrivilegesA" => fn sans_instruction_7,
        "SQLTablePrivilegesW" => fn sans_instruction_7,
        "SQLSetParam" => fn sans_instruction_8,
        "SQLBindParam" => fn sans_instruction_8,
        "SQLDescribeCol" => fn sans_instruction_9,
        "SQLDescribeColA" => fn sans_instruction_9,
        "SQLDescribeColW" => fn sans_instruction_9,
        "SQLColumns" => fn sans_instruction_9,
        "SQLColumnsA" => fn sans_instruction_9,
        "SQLColumnsW" => fn sans_instruction_9,
        "SQLStatistics" => fn sans_instruction_9,
        "SQLStatisticsA" => fn sans_instruction_9,
        "SQLStatisticsW" => fn sans_instruction_9,
        "SQLTables" => fn sans_instruction_9,
        "SQLTablesA" => fn sans_instruction_9,
        "SQLTablesW" => fn sans_instruction_9,
        "SQLColumnPrivileges" => fn sans_instruction_9,
        "SQLColumnPrivilegesA" => fn sans_instruction_9,
        "SQLColumnPrivilegesW" => fn sans_instruction_9,
        "SQLProcedureColumns" => fn sans_instruction_9,
        "SQLProcedureColumnsA" => fn sans_instruction_9,
        "SQLProcedureColumnsW" => fn sans_instruction_9,
        "SQLSpecialColumns" => fn sans_instruction_10,
        "SQLSpecialColumnsA" => fn sans_instruction_10,
        "SQLSpecialColumnsW" => fn sans_instruction_10,
        "SQLBindParameter" => fn sans_instruction_10,
        "SQLSetDescRec" => fn sans_instruction_10,
        "SQLGetDescRec" => fn sans_instruction_11,
        "SQLGetDescRecA" => fn sans_instruction_11,
        "SQLGetDescRecW" => fn sans_instruction_11,
        "SQLForeignKeys" => fn sans_instruction_13,
        "SQLForeignKeysA" => fn sans_instruction_13,
        "SQLForeignKeysW" => fn sans_instruction_13,
    };
    for &(ordinal, name) in ORDINALS {
        let mut add = |n: u16, export: String| {
            if let Some(&addr) = e.get(export.as_str()) {
                e.insert(Box::leak(format!("#{n}").into_boxed_str()), addr);
            }
        };
        add(ordinal, name.to_string());
        if WIDE.contains(&ordinal) {
            add(ordinal + 100, format!("{name}W"));
            add(ordinal + 200, format!("{name}A"));
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odbc_sans_pilote_par_ordinal() {
        let _teb = crate::process::test_teb();
        let e = exports();
        // Ordinaux importés par pywin32 312 (odbc.pyd) et Qt 6 (qsqlodbc).
        for o in [
            1, 2, 3, 4, 9, 12, 13, 14, 16, 18, 20, 23, 24, 26, 29, 30, 31, 43, 44, 48, 49, 61, 72,
            75, 108, 110, 111, 119, 127, 136, 138, 139, 140, 141, 145, 147, 150, 152, 154, 157,
            165, 176,
        ] {
            assert!(e.contains_key(format!("#{o}").as_str()), "#{o}");
        }
        assert_eq!(e.get("#141"), e.get("SQLDriverConnectW"));
        assert_eq!(e.get("#241"), e.get("SQLDriverConnectA"));
        assert_eq!(e.get("#24"), e.get("SQLAllocHandle"));
        assert_eq!(crate::dlls::normalize("ODBC32.dll"), "odbc32");
        unsafe {
            // Chemin de Qt : environnement ODBC 3, connexion, échec IM002.
            let mut env = 0usize;
            assert_eq!(sql_alloc_handle(SQL_HANDLE_ENV, 0, &mut env), SQL_SUCCESS);
            let mut dbc = 0usize;
            assert_eq!(sql_alloc_handle(SQL_HANDLE_DBC, env, &mut dbc), SQL_ERROR);
            assert_eq!(dbc, 0);
            assert_eq!(
                sql_set_env_attr(env, SQL_ATTR_ODBC_VERSION, 3, 0),
                SQL_SUCCESS
            );
            assert_eq!(sql_alloc_handle(SQL_HANDLE_DBC, env, &mut dbc), SQL_SUCCESS);
            assert!(dbc != 0 && dbc % 4 == 0);
            let conn: Vec<u16> = "DSN=x\0".encode_utf16().collect();
            assert_eq!(
                sql_driver_connect(
                    dbc,
                    0,
                    conn.as_ptr() as usize,
                    -3,
                    0,
                    0,
                    std::ptr::null_mut(),
                    0
                ),
                SQL_ERROR
            );
            let mut state = [0u16; 6];
            let mut msg = [0u16; 256];
            let (mut native, mut len) = (7i32, 0i16);
            assert_eq!(
                sql_get_diag_rec_w(
                    SQL_HANDLE_DBC,
                    dbc,
                    1,
                    state.as_mut_ptr() as usize,
                    &mut native,
                    msg.as_mut_ptr() as usize,
                    256,
                    &mut len
                ),
                SQL_SUCCESS
            );
            assert_eq!(String::from_utf16_lossy(&state[..5]), "IM002");
            assert_eq!(native, 0);
            assert!(len > 0 && msg[len as usize] == 0);
            assert_eq!(
                sql_get_diag_rec_w(
                    SQL_HANDLE_DBC,
                    dbc,
                    2,
                    0,
                    std::ptr::null_mut(),
                    0,
                    0,
                    std::ptr::null_mut()
                ),
                SQL_NO_DATA
            );
            let mut stmt = 1usize;
            assert_eq!(sql_alloc_handle(SQL_HANDLE_STMT, dbc, &mut stmt), SQL_ERROR);
            assert_eq!(stmt, 0);
            assert_eq!(sans_instruction_3(0, 0, 0), SQL_INVALID_HANDLE);
            assert_eq!(sql_free_handle(SQL_HANDLE_ENV, env), SQL_ERROR);
            assert_eq!(sql_free_handle(SQL_HANDLE_DBC, dbc), SQL_SUCCESS);
            assert_eq!(sql_free_handle(SQL_HANDLE_ENV, env), SQL_SUCCESS);
            assert_eq!(sql_free_handle(SQL_HANDLE_ENV, env), SQL_INVALID_HANDLE);

            // Chemin de pywin32 : ODBC 2, SQLConnect puis SQLError en ANSI.
            let mut env2 = 0usize;
            assert_eq!(sql_alloc_env(&mut env2), SQL_SUCCESS);
            assert_eq!(
                sql_list_none(
                    env2,
                    2,
                    0,
                    0,
                    std::ptr::null_mut(),
                    0,
                    0,
                    std::ptr::null_mut()
                ),
                SQL_NO_DATA
            );
            let mut dbc2 = 0usize;
            assert_eq!(sql_alloc_connect(env2, &mut dbc2), SQL_SUCCESS);
            assert_eq!(sql_connect(dbc2, 0, 0, 0, 0, 0, 0), SQL_ERROR);
            let mut st = [0u8; 6];
            let mut m = [0u8; 8];
            let mut l = 0i16;
            assert_eq!(
                sql_error_a(
                    env2,
                    dbc2,
                    0,
                    st.as_mut_ptr() as usize,
                    &mut native,
                    m.as_mut_ptr() as usize,
                    8,
                    &mut l
                ),
                SQL_SUCCESS_WITH_INFO
            );
            assert_eq!(&st[..5], b"IM002");
            assert_eq!(m[7], 0);
            assert!(l > 8);
            // SQLError consomme le diagnostic.
            assert_eq!(
                sql_error_a(
                    env2,
                    dbc2,
                    0,
                    0,
                    std::ptr::null_mut(),
                    0,
                    0,
                    std::ptr::null_mut()
                ),
                SQL_NO_DATA
            );
            assert_eq!(sql_disconnect(dbc2), SQL_ERROR);
            assert_eq!(sql_transact(env2, 0, 0), SQL_SUCCESS);
            assert_eq!(sql_free_connect(dbc2), SQL_SUCCESS);
            assert_eq!(sql_free_env(env2), SQL_SUCCESS);
        }
    }
}
