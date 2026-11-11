//! Base ouverte hors session et résumé d'un paquet (#1657) :
//! `MsiOpenDatabase`, `MsiDatabaseCommit`, `MsiGetSummaryInformation`,
//! `MsiSummaryInfo*` et `MsiGetFileHash`, d'après la documentation publique
//! de Windows Installer et la spécification ouverte [MS-OLEPS] (jeu de
//! propriétés du flux « \u{5}SummaryInformation »).
//!
//! Le module msi lit les paquets mais n'en écrit pas : une base ou un
//! résumé modifié ne peut pas être enregistré. `MsiDatabaseCommit` et
//! `MsiSummaryInfoPersist` réussissent quand il n'y a rien à écrire et
//! échouent (ERROR_FUNCTION_FAILED) sinon, en le disant sur stderr.

use super::db::Database;
use super::session;
use super::session_db::{self, with_db};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_OPEN_FAILED: u32 = 110;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_FUNCTION_FAILED: u32 = 1627;
const ERROR_UNKNOWN_PROPERTY: u32 = 1608;
const ERROR_DATATYPE_MISMATCH: u32 = 1629;

const VT_EMPTY: u32 = 0;
const VT_I2: u32 = 2;
const VT_I4: u32 = 3;
const VT_LPSTR: u32 = 30;
const VT_FILETIME: u32 = 64;

const SUMMARY_STREAM: &str = "\u{5}SummaryInformation";

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Prop {
    I2(i16),
    I4(i32),
    Str(String),
    Time(u64),
}

impl Prop {
    fn vt(&self) -> u32 {
        match self {
            Prop::I2(_) => VT_I2,
            Prop::I4(_) => VT_I4,
            Prop::Str(_) => VT_LPSTR,
            Prop::Time(_) => VT_FILETIME,
        }
    }
}

/// Type attendu de chaque propriété du résumé d'un paquet.
fn expected_vt(pid: u32) -> Option<u32> {
    Some(match pid {
        1 => VT_I2,
        2..=9 | 18 => VT_LPSTR,
        11..=13 => VT_FILETIME,
        14..=16 | 19 => VT_I4,
        _ => return None,
    })
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

/// Lit le premier jeu de propriétés du flux ([MS-OLEPS] 2.20-2.21).
pub(super) fn parse(stream: &[u8]) -> BTreeMap<u32, Prop> {
    let mut out = BTreeMap::new();
    let Some(section) = u32_at(stream, 44).map(|v| v as usize) else {
        return out;
    };
    let count = u32_at(stream, section + 4).unwrap_or(0) as usize;
    let mut codepage = 1252u32;
    let mut raw: Vec<(u32, Vec<u8>)> = Vec::new();
    for i in 0..count.min(256) {
        let (Some(pid), Some(off)) = (
            u32_at(stream, section + 8 + i * 8),
            u32_at(stream, section + 12 + i * 8),
        ) else {
            break;
        };
        let off = section + off as usize;
        let Some(vt) = u32_at(stream, off) else {
            continue;
        };
        let value = match vt & 0xFFFF {
            VT_I2 => u16_at(stream, off + 4).map(|v| Prop::I2(v as i16)),
            VT_I4 => u32_at(stream, off + 4).map(|v| Prop::I4(v as i32)),
            VT_FILETIME => stream
                .get(off + 4..off + 12)
                .and_then(|b| b.try_into().ok())
                .map(|b| Prop::Time(u64::from_le_bytes(b))),
            VT_LPSTR => {
                let len = u32_at(stream, off + 4).unwrap_or(0) as usize;
                if let Some(b) = stream.get(off + 8..off + 8 + len) {
                    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
                    raw.push((pid, b[..end].to_vec()));
                }
                None
            }
            _ => None,
        };
        if let Some(v) = value {
            if let (1, Prop::I2(cp)) = (pid, &v) {
                codepage = *cp as u16 as u32;
            }
            out.insert(pid, v);
        }
    }
    // Chaînes lues dans la page de codes du résumé (propriété 1).
    for (pid, bytes) in raw {
        out.insert(pid, Prop::Str(super::db::decode(&bytes, codepage)));
    }
    out
}

struct Summary {
    props: BTreeMap<u32, Prop>,
    changed: bool,
    max_updates: u32,
    updates: u32,
}

static SUMMARIES: Mutex<Option<HashMap<u32, Summary>>> = Mutex::new(None);

fn summaries<T>(f: impl FnOnce(&mut HashMap<u32, Summary>) -> T) -> T {
    let mut g = SUMMARIES.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

/// MsiCloseHandle d'un résumé.
pub fn close(h: u32) -> bool {
    summaries(|s| s.remove(&h)).is_some()
}

unsafe fn text(p: usize, wide: bool) -> String {
    if wide {
        crate::dlls::wstr(p as *const u16)
    } else {
        crate::dlls::cstr(p as *const u8)
    }
}

// ---------- MsiOpenDatabase / MsiDatabaseCommit ----------

fn open_database(path: usize, persist: usize, out: usize, wide: bool) -> u32 {
    if path == 0 || out == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    unsafe { *(out as *mut u32) = 0 };
    let name = unsafe { text(path, wide) };
    // MSIDBOPEN_* : 0 lecture seule, 1 transaction, 2 direct, 3/4 création ;
    // une valeur au-delà est le chemin du fichier de sortie (transaction).
    // Le bit MSIDBOPEN_PATCHFILE est ignoré.
    let mode = if persist > 0xFFFF {
        1
    } else {
        persist & !(0x20 | 0x10)
    };
    if matches!(mode, 3 | 4) {
        eprintln!("msi.dll : création d'une base MSI non prise en charge : {name}");
        return ERROR_OPEN_FAILED;
    }
    if mode > 4 {
        return ERROR_INVALID_PARAMETER;
    }
    let file = crate::paths::to_linux(&name);
    let Ok(db) = Database::open_tables(&file) else {
        return ERROR_OPEN_FAILED;
    };
    let h = session::new_handle();
    session_db::insert_opened(h, db, file, mode == 0);
    unsafe { *(out as *mut u32) = h };
    ERROR_SUCCESS
}

pub extern "win64" fn msi_open_database_w(path: usize, persist: usize, out: usize) -> u32 {
    open_database(path, persist, out, true)
}
pub extern "win64" fn msi_open_database_a(path: usize, persist: usize, out: usize) -> u32 {
    open_database(path, persist, out, false)
}

pub extern "win64" fn msi_database_commit(db: usize) -> u32 {
    match session_db::commit_state(db as u32) {
        None => ERROR_INVALID_HANDLE,
        Some(false) => ERROR_SUCCESS,
        Some(true) => {
            eprintln!("msi.dll : écriture d'une base MSI modifiée non prise en charge");
            ERROR_FUNCTION_FAILED
        }
    }
}

// ---------- MsiGetSummaryInformation ----------

fn get_summary(db: usize, path: usize, updates: u32, out: usize, wide: bool) -> u32 {
    if out == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    unsafe { *(out as *mut u32) = 0 };
    let stream = if db != 0 {
        if !session_db::is_db(db as u32) {
            return ERROR_INVALID_HANDLE;
        }
        match with_db(db as u32, |d| d.streams.get(SUMMARY_STREAM).cloned()) {
            Some(s) => s.unwrap_or_default(),
            None => return ERROR_INVALID_HANDLE,
        }
    } else {
        if path == 0 {
            return ERROR_INVALID_PARAMETER;
        }
        let file = crate::paths::to_linux(&unsafe { text(path, wide) });
        match super::cfb::Compound::open(&file, &|n, _| n == SUMMARY_STREAM) {
            Ok(mut c) => c.streams.remove(SUMMARY_STREAM).unwrap_or_default(),
            Err(_) => return ERROR_OPEN_FAILED,
        }
    };
    let h = session::new_handle();
    summaries(|s| {
        s.insert(
            h,
            Summary {
                props: parse(&stream),
                changed: false,
                max_updates: updates,
                updates: 0,
            },
        )
    });
    unsafe { *(out as *mut u32) = h };
    ERROR_SUCCESS
}

pub extern "win64" fn msi_get_summary_information_w(
    db: usize,
    path: usize,
    updates: u32,
    out: usize,
) -> u32 {
    get_summary(db, path, updates, out, true)
}
pub extern "win64" fn msi_get_summary_information_a(
    db: usize,
    path: usize,
    updates: u32,
    out: usize,
) -> u32 {
    get_summary(db, path, updates, out, false)
}

pub extern "win64" fn msi_summary_info_get_property_count(h: usize, out: usize) -> u32 {
    let Some(n) = summaries(|s| s.get(&(h as u32)).map(|x| x.props.len() as u32)) else {
        return ERROR_INVALID_HANDLE;
    };
    if out != 0 {
        unsafe { *(out as *mut u32) = n };
    }
    ERROR_SUCCESS
}

#[allow(clippy::too_many_arguments)]
fn get_prop(
    h: usize,
    pid: u32,
    vt_out: usize,
    int_out: usize,
    time_out: usize,
    buf: usize,
    pcch: usize,
    wide: bool,
) -> u32 {
    let Some(value) = summaries(|s| s.get(&(h as u32)).map(|x| x.props.get(&pid).cloned())) else {
        return ERROR_INVALID_HANDLE;
    };
    let vt = value.as_ref().map_or(VT_EMPTY, Prop::vt);
    unsafe {
        if vt_out != 0 {
            *(vt_out as *mut u32) = vt;
        }
        match value {
            Some(Prop::I2(v)) if int_out != 0 => *(int_out as *mut i32) = v as i32,
            Some(Prop::I4(v)) if int_out != 0 => *(int_out as *mut i32) = v,
            Some(Prop::Time(t)) if time_out != 0 => (time_out as *mut u64).write_unaligned(t),
            Some(Prop::Str(s)) => return session::put(&s, buf, pcch, wide),
            _ => {}
        }
    }
    ERROR_SUCCESS
}

pub extern "win64" fn msi_summary_info_get_property_w(
    h: usize,
    pid: u32,
    vt: usize,
    int: usize,
    time: usize,
    buf: usize,
    pcch: usize,
) -> u32 {
    get_prop(h, pid, vt, int, time, buf, pcch, true)
}
pub extern "win64" fn msi_summary_info_get_property_a(
    h: usize,
    pid: u32,
    vt: usize,
    int: usize,
    time: usize,
    buf: usize,
    pcch: usize,
) -> u32 {
    get_prop(h, pid, vt, int, time, buf, pcch, false)
}

#[allow(clippy::too_many_arguments)]
fn set_prop(h: usize, pid: u32, vt: u32, int: i32, time: usize, value: usize, wide: bool) -> u32 {
    let Some(expected) = expected_vt(pid) else {
        return ERROR_UNKNOWN_PROPERTY;
    };
    if vt != expected {
        return ERROR_DATATYPE_MISMATCH;
    }
    let prop = match vt {
        VT_I2 => Prop::I2(int as i16),
        VT_I4 => Prop::I4(int),
        VT_FILETIME if time != 0 => Prop::Time(unsafe { (time as *const u64).read_unaligned() }),
        VT_LPSTR if value != 0 => Prop::Str(unsafe { text(value, wide) }),
        _ => return ERROR_INVALID_PARAMETER,
    };
    summaries(|s| {
        let Some(x) = s.get_mut(&(h as u32)) else {
            return ERROR_INVALID_HANDLE;
        };
        if !x.props.contains_key(&pid) {
            // Nouvelle propriété : comptée dans uiUpdateCount.
            if x.updates >= x.max_updates {
                return ERROR_FUNCTION_FAILED;
            }
            x.updates += 1;
        }
        if x.props.get(&pid) != Some(&prop) {
            x.props.insert(pid, prop);
            x.changed = true;
        }
        ERROR_SUCCESS
    })
}

pub extern "win64" fn msi_summary_info_set_property_w(
    h: usize,
    pid: u32,
    vt: u32,
    int: i32,
    time: usize,
    value: usize,
) -> u32 {
    set_prop(h, pid, vt, int, time, value, true)
}
pub extern "win64" fn msi_summary_info_set_property_a(
    h: usize,
    pid: u32,
    vt: u32,
    int: i32,
    time: usize,
    value: usize,
) -> u32 {
    set_prop(h, pid, vt, int, time, value, false)
}

pub extern "win64" fn msi_summary_info_persist(h: usize) -> u32 {
    match summaries(|s| s.get(&(h as u32)).map(|x| x.changed)) {
        None => ERROR_INVALID_HANDLE,
        Some(false) => ERROR_SUCCESS,
        Some(true) => {
            eprintln!("msi.dll : écriture du résumé d'un paquet non prise en charge");
            ERROR_FUNCTION_FAILED
        }
    }
}

// ---------- MsiGetFileHash ----------

/// Empreinte d'un fichier pour la table MsiFileHash : MD5 du contenu, en
/// quatre mots de 32 bits (MSIFILEHASHINFO, 20 octets).
fn file_hash(path: usize, options: u32, out: usize, wide: bool) -> u32 {
    if path == 0 || out == 0 || options != 0 {
        return ERROR_INVALID_PARAMETER;
    }
    if unsafe { (out as *const u32).read_unaligned() } != 20 {
        return ERROR_INVALID_PARAMETER;
    }
    let file = crate::paths::to_linux(&unsafe { text(path, wide) });
    let Ok(data) = std::fs::read(&file) else {
        return ERROR_OPEN_FAILED;
    };
    let digest = crate::dlls::games_win32::md5(&data);
    for (i, w) in digest.as_chunks::<4>().0.iter().enumerate() {
        unsafe {
            (out as *mut u32)
                .add(1 + i)
                .write_unaligned(u32::from_le_bytes(*w))
        };
    }
    ERROR_SUCCESS
}

pub extern "win64" fn msi_get_file_hash_w(path: usize, options: u32, out: usize) -> u32 {
    file_hash(path, options, out, true)
}
pub extern "win64" fn msi_get_file_hash_a(path: usize, options: u32, out: usize) -> u32 {
    file_hash(path, options, out, false)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "MsiOpenDatabaseW" => fn msi_open_database_w,
        "MsiOpenDatabaseA" => fn msi_open_database_a,
        "MsiDatabaseCommit" => fn msi_database_commit,
        "MsiGetSummaryInformationW" => fn msi_get_summary_information_w,
        "MsiGetSummaryInformationA" => fn msi_get_summary_information_a,
        "MsiSummaryInfoGetPropertyCount" => fn msi_summary_info_get_property_count,
        "MsiSummaryInfoGetPropertyW" => fn msi_summary_info_get_property_w,
        "MsiSummaryInfoGetPropertyA" => fn msi_summary_info_get_property_a,
        "MsiSummaryInfoSetPropertyW" => fn msi_summary_info_set_property_w,
        "MsiSummaryInfoSetPropertyA" => fn msi_summary_info_set_property_a,
        "MsiSummaryInfoPersist" => fn msi_summary_info_persist,
        "MsiGetFileHashW" => fn msi_get_file_hash_w,
        "MsiGetFileHashA" => fn msi_get_file_hash_a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/msi/testdata/sample.msi")
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    #[test]
    fn base_et_resume_hors_session() {
        let path = wide(&sample_path().to_string_lossy());
        let mut db = 0u32;
        assert_eq!(
            msi_open_database_w(path.as_ptr() as usize, 0, &mut db as *mut u32 as usize),
            ERROR_SUCCESS
        );
        assert_ne!(db, 0);
        // Une vue sur la base ouverte.
        let sql = wide("SELECT `File` FROM `File`");
        let mut view = 0u32;
        assert_eq!(
            session_db::msi_database_open_view_w(
                db as usize,
                sql.as_ptr() as usize,
                &mut view as *mut u32 as usize
            ),
            ERROR_SUCCESS
        );
        assert_eq!(
            session_db::msi_view_execute(view as usize, 0),
            ERROR_SUCCESS
        );
        let mut rec = 0u32;
        assert_eq!(
            session_db::msi_view_fetch(view as usize, &mut rec as *mut u32 as usize),
            ERROR_SUCCESS
        );
        // Rien de modifié : l'enregistrement réussit.
        assert_eq!(msi_database_commit(db as usize), ERROR_SUCCESS);

        // Résumé : modèle « Intel;1033 » ou « x64;… », lu par la poignée.
        let mut sum = 0u32;
        assert_eq!(
            msi_get_summary_information_w(db as usize, 0, 0, &mut sum as *mut u32 as usize),
            ERROR_SUCCESS
        );
        let mut count = 0u32;
        msi_summary_info_get_property_count(sum as usize, &mut count as *mut u32 as usize);
        assert!(count > 0);
        let (mut vt, mut int, mut ft) = (0u32, 0i32, 0u64);
        let mut buf = [0u8; 64];
        let mut cch = buf.len() as u32;
        assert_eq!(
            msi_summary_info_get_property_a(
                sum as usize,
                7,
                &mut vt as *mut u32 as usize,
                &mut int as *mut i32 as usize,
                &mut ft as *mut u64 as usize,
                buf.as_mut_ptr() as usize,
                &mut cch as *mut u32 as usize,
            ),
            ERROR_SUCCESS
        );
        assert_eq!(vt, VT_LPSTR);
        let template = String::from_utf8_lossy(&buf[..cch as usize]).into_owned();
        let expected = with_db(db, |d| d.template.clone()).unwrap();
        assert_eq!(template, expected);
        // Mot du résumé (15) : entier.
        msi_summary_info_get_property_a(
            sum as usize,
            15,
            &mut vt as *mut u32 as usize,
            &mut int as *mut i32 as usize,
            0,
            0,
            0,
        );
        assert_eq!(vt, VT_I4);
        assert_eq!(int, with_db(db, |d| d.word_count).unwrap());

        // Mauvais type refusé ; nouvelle propriété au-delà du compte refusée.
        let author = wide("Winlux");
        assert_eq!(
            msi_summary_info_set_property_w(sum as usize, 4, VT_I4, 0, 0, author.as_ptr() as usize),
            ERROR_DATATYPE_MISMATCH
        );
        assert_eq!(
            msi_summary_info_set_property_w(sum as usize, 99, VT_I4, 0, 0, 0),
            ERROR_UNKNOWN_PROPERTY
        );
        assert_eq!(msi_summary_info_persist(sum as usize), ERROR_SUCCESS);
        assert!(close(sum));

        // Par le chemin, sans base ouverte.
        let mut sum2 = 0u32;
        assert_eq!(
            msi_get_summary_information_w(
                0,
                path.as_ptr() as usize,
                1,
                &mut sum2 as *mut u32 as usize
            ),
            ERROR_SUCCESS
        );
        let mut count2 = 0u32;
        msi_summary_info_get_property_count(sum2 as usize, &mut count2 as *mut u32 as usize);
        assert_eq!(count, count2);
        assert!(close(sum2));
        assert!(session_db::close(db));
    }

    #[test]
    fn empreinte_md5_du_fichier() {
        let dir = std::env::temp_dir().join(format!("winlux-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("abc.txt");
        std::fs::write(&f, b"abc").unwrap();
        let path = wide(&f.to_string_lossy());
        let mut info = [20u32, 0, 0, 0, 0];
        assert_eq!(
            msi_get_file_hash_w(path.as_ptr() as usize, 0, info.as_mut_ptr() as usize),
            ERROR_SUCCESS
        );
        // MD5("abc") = 900150983cd24fb0d6963f7d28e17f72, mots petit-boutistes.
        assert_eq!(info[1..], [0x98500190, 0xb04fd23c, 0x7d3f96d6, 0x727fe128]);
        let mut bad = [16u32, 0, 0, 0, 0];
        assert_eq!(
            msi_get_file_hash_w(path.as_ptr() as usize, 0, bad.as_mut_ptr() as usize),
            ERROR_INVALID_PARAMETER
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
