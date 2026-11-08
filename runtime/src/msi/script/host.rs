//! Objets et fonctions intégrées des scripts : `Session` de Windows
//! Installer (et ses vues, enregistrements, base), `WScript.Shell`,
//! `Scripting.FileSystemObject`, `Scripting.Dictionary`, `RegExp`, `Math`,
//! `Err`, fonctions de chaîne et de conversion des deux langages. Tout est
//! écrit d'après leur documentation publique.

use super::regex::Regex;
use super::{make_error, num_str, parse_num, round_even, Ex, Host, Interp, Lang, R, V};
use crate::msi::{session, session_db};
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

fn arg(args: &[V], i: usize) -> V {
    args.get(i).cloned().unwrap_or(V::Empty)
}

fn given(args: &[V], i: usize) -> bool {
    !matches!(args.get(i), None | Some(V::Empty))
}

fn lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

// ---------- appels vers msi.dll (même processus) ----------

const SESSION: usize = session::SESSION_HANDLE as usize;
const ERROR_MORE_DATA: u32 = 234;

fn wz(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Lit une chaîne rendue « façon Windows Installer » (tampon et taille).
fn fetch_text(f: impl Fn(usize, usize) -> u32) -> Result<String, u32> {
    let mut cap: u32 = 256;
    loop {
        let mut buf = vec![0u16; cap as usize + 1];
        let mut n = cap + 1;
        let r = f(buf.as_mut_ptr() as usize, &mut n as *mut u32 as usize);
        if r == ERROR_MORE_DATA {
            cap = n + 1;
            continue;
        }
        if r != 0 {
            return Err(r);
        }
        buf.truncate(n as usize);
        return Ok(String::from_utf16_lossy(&buf));
    }
}

fn msi_err(r: u32, what: &str) -> Ex {
    Ex::new(
        -2147467259,
        format!("{what} : erreur Windows Installer {r}"),
    )
}

pub fn session() -> V {
    V::host(SessionObj)
}

struct SessionObj;

impl Host for SessionObj {
    fn type_name(&self) -> &str {
        "Session"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let s = |i: usize| it.to_str(&arg(&args, i));
        Ok(match lower(name).as_str() {
            "property" => {
                let n = wz(&s(0));
                V::str(
                    fetch_text(|b, p| {
                        session::msi_get_property_w(SESSION, n.as_ptr() as usize, b, p)
                    })
                    .map_err(|r| msi_err(r, "Session.Property"))?,
                )
            }
            "installer" => V::host(InstallerObj),
            "database" => {
                let h = session_db::msi_get_active_database(SESSION);
                if h == 0 {
                    return Err(Ex::new(
                        -2147467259,
                        "Session.Database : pas de base dans une action différée",
                    ));
                }
                V::host(DatabaseObj(h))
            }
            "mode" => {
                let m = it.to_num(&arg(&args, 0))? as u32;
                V::Bool(session::msi_get_mode(SESSION, m) != 0)
            }
            "language" => V::Num(session::msi_get_language(SESSION) as f64),
            "targetpath" | "sourcepath" => {
                let n = wz(&s(0));
                let r = fetch_text(|b, p| {
                    session::msi_get_target_path_w(SESSION, n.as_ptr() as usize, b, p)
                });
                match r {
                    Ok(v) => V::str(v),
                    Err(_) if lower(name) == "sourcepath" => {
                        let d = wz("SourceDir");
                        V::str(
                            fetch_text(|b, p| {
                                session::msi_get_property_w(SESSION, d.as_ptr() as usize, b, p)
                            })
                            .unwrap_or_default(),
                        )
                    }
                    Err(r) => return Err(msi_err(r, &format!("Session.TargetPath(\"{}\")", s(0)))),
                }
            }
            "evaluatecondition" => {
                let c = wz(&s(0));
                V::Num(session::msi_evaluate_condition_w(SESSION, c.as_ptr() as usize) as f64)
            }
            "doaction" => {
                let a = wz(&s(0));
                session::msi_do_action_w(SESSION, a.as_ptr() as usize);
                V::Num(1.0)
            }
            "message" => {
                let kind = it.to_num(&arg(&args, 0))? as i64 as i32;
                let rec = record_handle(&arg(&args, 1))
                    .ok_or_else(|| Ex::new(424, "Session.Message : enregistrement attendu"))?;
                V::Num(session::msi_process_message(SESSION, kind, rec as usize) as f64)
            }
            "formatrecord" => {
                let rec = record_handle(&arg(&args, 0))
                    .ok_or_else(|| Ex::new(424, "Session.FormatRecord : enregistrement attendu"))?;
                V::str(
                    fetch_text(|b, p| session::msi_format_record_w(SESSION, rec as usize, b, p))
                        .map_err(|r| msi_err(r, "Session.FormatRecord"))?,
                )
            }
            "featurecurrentstate"
            | "componentcurrentstate"
            | "featurerequeststate"
            | "componentrequeststate" => {
                let (mut installed, mut action) = (0i32, 0i32);
                let n = wz(&s(0));
                session::msi_get_state(
                    SESSION,
                    n.as_ptr() as usize,
                    &mut installed as *mut i32 as usize,
                    &mut action as *mut i32 as usize,
                );
                V::Num(if lower(name).ends_with("currentstate") {
                    installed
                } else {
                    action
                } as f64)
            }
            "setinstalllevel" | "sequence" => V::Num(0.0),
            "productproperty" => V::str(""),
            _ => return Err(Ex::no_member(name)),
        })
    }
    fn put(&self, it: &mut Interp, name: &str, args: Vec<V>, value: V) -> R<()> {
        match lower(name).as_str() {
            "property" | "targetpath" => {
                let n = wz(&it.to_str(&arg(&args, 0)));
                let v = wz(&it.to_str(&value));
                let r =
                    session::msi_set_property_w(SESSION, n.as_ptr() as usize, v.as_ptr() as usize);
                if r != 0 {
                    return Err(msi_err(r, "Session.Property"));
                }
                Ok(())
            }
            "featurerequeststate" | "componentrequeststate" => Ok(()),
            _ => Err(Ex::no_member(name)),
        }
    }
}

struct InstallerObj;

impl Host for InstallerObj {
    fn type_name(&self) -> &str {
        "Installer"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        Ok(match lower(name).as_str() {
            "createrecord" => {
                let n = it.to_num(&arg(&args, 0))?.max(0.0) as u32;
                let h = session::msi_create_record(n);
                if h == 0 {
                    return Err(Ex::new(
                        5,
                        "Installer.CreateRecord : nombre de champs incorrect",
                    ));
                }
                V::host(RecordObj(h))
            }
            "environment" => {
                let n = it.to_str(&arg(&args, 0));
                V::str(crate::dlls::kernel32::env_value(&n).unwrap_or_default())
            }
            "filesize" => {
                let p = crate::paths::to_linux(&it.to_str(&arg(&args, 0)));
                let m = std::fs::metadata(&p).map_err(|_| Ex::new(53, "fichier introuvable"))?;
                V::Num(m.len() as f64)
            }
            "fileversion" => V::str(""),
            "lasterrorrecord" => V::Nothing,
            "version" => V::str("5.0"),
            _ => return Err(Ex::no_member(name)),
        })
    }
    fn put(&self, it: &mut Interp, name: &str, args: Vec<V>, value: V) -> R<()> {
        if lower(name) == "environment" {
            let n = it.to_str(&arg(&args, 0));
            let v = it.to_str(&value);
            std::env::set_var(&n, &v);
            return Ok(());
        }
        Err(Ex::no_member(name))
    }
}

fn record_handle(v: &V) -> Option<u32> {
    match v {
        V::Host(h) => h.as_any().downcast_ref::<RecordObj>().map(|r| r.0),
        _ => None,
    }
}

struct RecordObj(u32);

impl Drop for RecordObj {
    fn drop(&mut self) {
        session::msi_close_handle(self.0 as usize);
    }
}

impl Host for RecordObj {
    fn type_name(&self) -> &str {
        "Record"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let h = self.0 as usize;
        let field = || -> R<u32> { Ok(it.to_num(&arg(&args, 0))?.max(0.0) as u32) };
        Ok(match lower(name).as_str() {
            "stringdata" => {
                let f = field()?;
                V::str(
                    fetch_text(|b, p| session::msi_record_get_string_w(h, f, b, p))
                        .map_err(|r| msi_err(r, "Record.StringData"))?,
                )
            }
            "integerdata" => V::Num(session::msi_record_get_integer(h, field()?) as f64),
            "fieldcount" => V::Num(session::msi_record_get_field_count(h) as f64),
            "isnull" => V::Bool(session::msi_record_is_null(h, field()?) != 0),
            "datasize" => V::Num(session::msi_record_data_size(h, field()?) as f64),
            "cleardata" => {
                session::msi_record_clear_data(h);
                V::Empty
            }
            "readstream" => {
                let f = field()?;
                let len = it.to_num(&arg(&args, 1))?.max(0.0) as u32;
                let mut buf = vec![0u8; len as usize];
                let mut n = len;
                let r = session::msi_record_read_stream(
                    h,
                    f,
                    buf.as_mut_ptr() as usize,
                    &mut n as *mut u32 as usize,
                );
                if r != 0 {
                    return Err(msi_err(r, "Record.ReadStream"));
                }
                buf.truncate(n as usize);
                V::str(buf.iter().map(|&b| b as char).collect::<String>())
            }
            "formattext" => V::str(
                fetch_text(|b, p| session::msi_format_record_w(SESSION, h, b, p))
                    .map_err(|r| msi_err(r, "Record.FormatText"))?,
            ),
            _ => return Err(Ex::no_member(name)),
        })
    }
    fn put(&self, it: &mut Interp, name: &str, args: Vec<V>, value: V) -> R<()> {
        let h = self.0 as usize;
        let f = it.to_num(&arg(&args, 0))?.max(0.0) as u32;
        let r = match lower(name).as_str() {
            "stringdata" => {
                let v = wz(&it.to_str(&value));
                session::msi_record_set_string_w(h, f, v.as_ptr() as usize)
            }
            "integerdata" => {
                session::msi_record_set_integer(h, f, it.to_num(&value)? as i64 as i32)
            }
            _ => return Err(Ex::no_member(name)),
        };
        if r != 0 {
            return Err(msi_err(r, "Record"));
        }
        Ok(())
    }
}

struct DatabaseObj(u32);

impl Drop for DatabaseObj {
    fn drop(&mut self) {
        session::msi_close_handle(self.0 as usize);
    }
}

impl Host for DatabaseObj {
    fn type_name(&self) -> &str {
        "Database"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        Ok(match lower(name).as_str() {
            "openview" => {
                let q = wz(&it.to_str(&arg(&args, 0)));
                let mut view: u32 = 0;
                let r = session_db::msi_database_open_view_w(
                    self.0 as usize,
                    q.as_ptr() as usize,
                    &mut view as *mut u32 as usize,
                );
                if r != 0 {
                    return Err(msi_err(
                        r,
                        &format!("Database.OpenView(\"{}\")", it.to_str(&arg(&args, 0))),
                    ));
                }
                V::host(ViewObj(view))
            }
            "tablepersistent" | "istablepersistent" => {
                let t = wz(&it.to_str(&arg(&args, 0)));
                V::Num(session_db::msi_database_is_table_persistent_w(
                    self.0 as usize,
                    t.as_ptr() as usize,
                ) as f64)
            }
            "commit" => V::Empty,
            _ => return Err(Ex::no_member(name)),
        })
    }
}

struct ViewObj(u32);

impl Drop for ViewObj {
    fn drop(&mut self) {
        session::msi_close_handle(self.0 as usize);
    }
}

impl Host for ViewObj {
    fn type_name(&self) -> &str {
        "View"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let h = self.0 as usize;
        Ok(match lower(name).as_str() {
            "execute" => {
                let rec = record_handle(&arg(&args, 0)).unwrap_or(0);
                let r = session_db::msi_view_execute(h, rec as usize);
                if r != 0 {
                    return Err(msi_err(r, "View.Execute"));
                }
                V::Empty
            }
            "fetch" => {
                let mut rec: u32 = 0;
                match session_db::msi_view_fetch(h, &mut rec as *mut u32 as usize) {
                    0 => V::host(RecordObj(rec)),
                    259 => V::Nothing,
                    r => return Err(msi_err(r, "View.Fetch")),
                }
            }
            "modify" => {
                let mode = it.to_num(&arg(&args, 0))? as i64 as i32;
                let rec = record_handle(&arg(&args, 1))
                    .ok_or_else(|| Ex::new(424, "View.Modify : enregistrement attendu"))?;
                let r = session_db::msi_view_modify(h, mode, rec as usize);
                if r != 0 {
                    return Err(msi_err(r, "View.Modify"));
                }
                V::Empty
            }
            "close" => {
                session_db::msi_view_close(h);
                V::Empty
            }
            "columninfo" => {
                let kind = it.to_num(&arg(&args, 0))? as i64 as i32;
                let mut rec: u32 = 0;
                let r =
                    session_db::msi_view_get_column_info(h, kind, &mut rec as *mut u32 as usize);
                if r != 0 {
                    return Err(msi_err(r, "View.ColumnInfo"));
                }
                V::host(RecordObj(rec))
            }
            _ => return Err(Ex::no_member(name)),
        })
    }
}

// ---------- WScript.Shell ----------

struct ShellObj;

/// `HKLM\Software\X\Nom` → (clé, nom) ; un `\` final désigne la valeur par
/// défaut de la clé.
fn split_reg(path: &str) -> (String, String) {
    match path.rsplit_once('\\') {
        Some((k, n)) => (k.to_string(), n.to_string()),
        None => (path.to_string(), String::new()),
    }
}

fn reg_missing(path: &str) -> Ex {
    Ex::new(
        -2147024894,
        format!("clé ou valeur de registre introuvable : {path}"),
    )
}

impl Host for ShellObj {
    fn type_name(&self) -> &str {
        "IWshShell3"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let s = |i: usize| it.to_str(&arg(&args, i));
        Ok(match lower(name).as_str() {
            "expandenvironmentstrings" => {
                V::str(crate::dlls::kernel32_fs::expand_environment(&s(0)))
            }
            "regread" => {
                let path = s(0);
                let (key, value) = split_reg(&path);
                let canon = crate::registry::canonical(&key).ok_or_else(|| reg_missing(&path))?;
                let found = crate::registry::read(|d| {
                    Ok(d.get(&canon, &value).map(|v| (v.kind, v.bytes.clone())))
                })
                .ok()
                .flatten();
                let Some((kind, data)) = found else {
                    return Err(reg_missing(&path));
                };
                match kind {
                    4 if data.len() >= 4 => {
                        V::Num(
                            u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as i32 as f64,
                        )
                    }
                    11 if data.len() >= 8 => {
                        V::Num(u64::from_le_bytes(data[..8].try_into().unwrap()) as f64)
                    }
                    3 => V::arr(data.iter().map(|&b| V::Num(b as f64)).collect()),
                    7 => {
                        let text = crate::registry::string_of(&data);
                        V::arr(
                            text.split('\0')
                                .filter(|x| !x.is_empty())
                                .map(V::str)
                                .collect(),
                        )
                    }
                    _ => V::str(crate::registry::string_of(&data)),
                }
            }
            "regwrite" => {
                let path = s(0);
                let (key, value) = split_reg(&path);
                let kind = if given(&args, 2) {
                    s(2).to_ascii_uppercase()
                } else {
                    "REG_SZ".into()
                };
                let v = arg(&args, 1);
                let (code, bytes) = match kind.as_str() {
                    "REG_DWORD" => (4, (it.to_num(&v)? as i64 as u32).to_le_bytes().to_vec()),
                    "REG_BINARY" => (3, (it.to_num(&v)? as i64 as u32).to_le_bytes().to_vec()),
                    "REG_EXPAND_SZ" => (2, utf16z(&it.to_str(&v))),
                    _ => (1, utf16z(&it.to_str(&v))),
                };
                let edits = if value.is_empty() && path.ends_with('\\') && !given(&args, 1) {
                    vec![crate::dlls::advapi32::RegEdit::Key(key)]
                } else {
                    vec![
                        crate::dlls::advapi32::RegEdit::Key(key.clone()),
                        crate::dlls::advapi32::RegEdit::Set {
                            key,
                            name: value,
                            kind: code,
                            bytes,
                        },
                    ]
                };
                if !crate::dlls::advapi32::apply_registry_edits(&edits) {
                    return Err(Ex::new(
                        -2147024891,
                        format!("écriture refusée dans le registre : {path}"),
                    ));
                }
                V::Empty
            }
            "regdelete" => {
                let path = s(0);
                let edit = if path.ends_with('\\') {
                    crate::dlls::advapi32::RegEdit::DeleteTree(
                        path.trim_end_matches('\\').to_string(),
                    )
                } else {
                    let (key, name) = split_reg(&path);
                    crate::dlls::advapi32::RegEdit::DeleteValue { key, name }
                };
                if !crate::dlls::advapi32::apply_registry_edits(&[edit]) {
                    return Err(reg_missing(&path));
                }
                V::Empty
            }
            "run" => {
                let line = s(0);
                let wait = given(&args, 2) && it.truthy(&arg(&args, 2))?;
                let (outcome, _) = crate::setup::run::command_line(&line, "", wait, false);
                use crate::setup::run::Outcome;
                match outcome {
                    Outcome::Exited(c) => V::Num(c as f64),
                    Outcome::Started | Outcome::TimedOut => V::Num(0.0),
                    Outcome::Skipped(why) | Outcome::Failed(why) => {
                        return Err(Ex::new(
                            -2147024894,
                            format!("WScript.Shell.Run « {line} » : {why}"),
                        ))
                    }
                }
            }
            "environment" => V::host(EnvObj),
            "currentdirectory" => V::str(crate::paths::to_windows(
                &std::env::current_dir().unwrap_or_default(),
            )),
            "specialfolders" => {
                let var = match lower(&s(0)).as_str() {
                    "desktop" | "alluserdesktop" => "USERPROFILE",
                    "appdata" => "APPDATA",
                    "programs" | "startmenu" | "allusersprograms" | "allusersstartmenu" => {
                        "APPDATA"
                    }
                    _ => "USERPROFILE",
                };
                V::str(crate::dlls::kernel32::env_value(var).unwrap_or_default())
            }
            "popup" => {
                eprintln!("msi.dll : MsgBox : {}", s(0));
                V::Num(-1.0)
            }
            "logevent" => {
                eprintln!("msi.dll : {}", s(1));
                V::Bool(true)
            }
            _ => return Err(Ex::no_member(name)),
        })
    }
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

struct EnvObj;

impl Host for EnvObj {
    fn type_name(&self) -> &str {
        "IWshEnvironment"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        match lower(name).as_str() {
            "" | "item" => {
                let n = it.to_str(&arg(&args, 0));
                Ok(V::str(
                    crate::dlls::kernel32::env_value(&n).unwrap_or_default(),
                ))
            }
            "remove" => {
                std::env::remove_var(it.to_str(&arg(&args, 0)));
                Ok(V::Empty)
            }
            _ => Err(Ex::no_member(name)),
        }
    }
    fn put(&self, it: &mut Interp, name: &str, args: Vec<V>, value: V) -> R<()> {
        match lower(name).as_str() {
            "" | "item" => {
                std::env::set_var(it.to_str(&arg(&args, 0)), it.to_str(&value));
                Ok(())
            }
            _ => Err(Ex::no_member(name)),
        }
    }
}

// ---------- Scripting.FileSystemObject ----------

fn linux(p: &str) -> std::path::PathBuf {
    crate::paths::to_linux(p)
}

fn win_parts(p: &str) -> (String, String) {
    let t = p.trim_end_matches(['\\', '/']);
    match t.rfind(['\\', '/']) {
        Some(i) => (t[..i].to_string(), t[i + 1..].to_string()),
        None => (String::new(), t.to_string()),
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

fn file_not_found(p: &str) -> Ex {
    Ex::new(53, format!("fichier introuvable : {p}"))
}

fn path_not_found(p: &str) -> Ex {
    Ex::new(76, format!("chemin introuvable : {p}"))
}

struct FsoObj;

impl Host for FsoObj {
    fn type_name(&self) -> &str {
        "FileSystemObject"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let s = |i: usize| it.to_str(&arg(&args, i));
        let p0 = s(0);
        Ok(match lower(name).as_str() {
            "fileexists" => V::Bool(linux(&p0).is_file()),
            "folderexists" => V::Bool(linux(&p0).is_dir()),
            "driveexists" => V::Bool(p0.len() >= 2 && p0.as_bytes()[1] == b':'),
            "createfolder" => {
                let l = linux(&p0);
                if l.exists() {
                    return Err(Ex::new(58, format!("le dossier existe déjà : {p0}")));
                }
                std::fs::create_dir(&l).map_err(|_| path_not_found(&p0))?;
                V::host(FolderObj(p0))
            }
            "deletefile" => {
                std::fs::remove_file(linux(&p0)).map_err(|_| file_not_found(&p0))?;
                V::Empty
            }
            "deletefolder" => {
                std::fs::remove_dir_all(linux(&p0)).map_err(|_| path_not_found(&p0))?;
                V::Empty
            }
            "copyfile" | "movefile" | "copyfolder" | "movefolder" => {
                let dst = s(1);
                let src = linux(&p0);
                let mut to = linux(&dst);
                if dst.ends_with(['\\', '/']) || to.is_dir() && src.is_file() {
                    to = to.join(src.file_name().unwrap_or_default());
                }
                let overwrite = !given(&args, 2) || it.truthy(&arg(&args, 2))?;
                if to.exists() && !overwrite && !lower(name).starts_with("move") {
                    return Err(Ex::new(58, format!("le fichier existe déjà : {dst}")));
                }
                if !src.exists() {
                    return Err(file_not_found(&p0));
                }
                let r = if lower(name).starts_with("move") {
                    std::fs::rename(&src, &to).or_else(|_| {
                        copy_tree(&src, &to).and_then(|_| {
                            if src.is_dir() {
                                std::fs::remove_dir_all(&src)
                            } else {
                                std::fs::remove_file(&src)
                            }
                        })
                    })
                } else {
                    copy_tree(&src, &to)
                };
                r.map_err(|e| Ex::new(70, format!("copie impossible vers {dst} : {e}")))?;
                V::Empty
            }
            "getfile" => {
                if !linux(&p0).is_file() {
                    return Err(file_not_found(&p0));
                }
                V::host(FileObj(p0))
            }
            "getfolder" => {
                if !linux(&p0).is_dir() {
                    return Err(path_not_found(&p0));
                }
                V::host(FolderObj(p0))
            }
            "opentextfile" => {
                let mode = if given(&args, 1) {
                    it.to_num(&arg(&args, 1))? as i32
                } else {
                    1
                };
                let create = given(&args, 2) && it.truthy(&arg(&args, 2))?;
                let unicode = given(&args, 3) && it.to_num(&arg(&args, 3))? as i32 == -1;
                TextStream::open(&p0, mode, create, unicode)?
            }
            "createtextfile" => {
                let overwrite = !given(&args, 1) || it.truthy(&arg(&args, 1))?;
                let unicode = given(&args, 2) && it.truthy(&arg(&args, 2))?;
                if !overwrite && linux(&p0).exists() {
                    return Err(Ex::new(58, format!("le fichier existe déjà : {p0}")));
                }
                TextStream::create(&p0, unicode)?
            }
            "getfilename" => V::str(win_parts(&p0).1),
            "getbasename" => {
                let n = win_parts(&p0).1;
                V::str(match n.rfind('.') {
                    Some(i) => n[..i].to_string(),
                    None => n,
                })
            }
            "getextensionname" => {
                let n = win_parts(&p0).1;
                V::str(
                    n.rfind('.')
                        .map(|i| n[i + 1..].to_string())
                        .unwrap_or_default(),
                )
            }
            "getparentfoldername" => V::str(win_parts(&p0).0),
            "getdrivename" => V::str(if p0.len() >= 2 && p0.as_bytes()[1] == b':' {
                p0[..2].to_string()
            } else {
                String::new()
            }),
            "getabsolutepathname" => V::str(p0),
            "buildpath" => {
                let b = p0;
                let n = s(1);
                V::str(if b.ends_with(['\\', '/']) || b.is_empty() {
                    format!("{b}{n}")
                } else {
                    format!("{b}\\{}", n.trim_start_matches(['\\', '/']))
                })
            }
            "getspecialfolder" => {
                let which = it.to_num(&arg(&args, 0))? as i32;
                let p = match which {
                    0 => crate::dlls::kernel32::env_value("WINDIR")
                        .unwrap_or_else(|| "C:\\Windows".into()),
                    1 => format!(
                        "{}\\System32",
                        crate::dlls::kernel32::env_value("WINDIR")
                            .unwrap_or_else(|| "C:\\Windows".into())
                    ),
                    _ => crate::dlls::kernel32::env_value("TEMP")
                        .unwrap_or_else(|| "C:\\Windows\\Temp".into()),
                };
                V::host(FolderObj(p))
            }
            "gettempname" => V::str(format!("rad{:05X}.tmp", (it.random() * 1_048_575.0) as u32)),
            _ => return Err(Ex::no_member(name)),
        })
    }
}

struct FileObj(String);

impl Host for FileObj {
    fn type_name(&self) -> &str {
        "File"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let l = linux(&self.0);
        Ok(match lower(name).as_str() {
            "" | "path" => V::str(&self.0),
            "name" => V::str(win_parts(&self.0).1),
            "shortpath" => V::str(&self.0),
            "size" => V::Num(std::fs::metadata(&l).map(|m| m.len()).unwrap_or(0) as f64),
            "parentfolder" => V::host(FolderObj(win_parts(&self.0).0)),
            "attributes" => V::Num(32.0),
            "type" => V::str("Fichier"),
            "datelastmodified" | "datecreated" | "datelastaccessed" => V::str(""),
            "delete" => {
                std::fs::remove_file(&l).map_err(|_| file_not_found(&self.0))?;
                V::Empty
            }
            "copy" | "move" => FsoObj.get(
                it,
                &format!("{name}file"),
                vec![V::str(&self.0), arg(&args, 0), arg(&args, 1)],
            )?,
            "openastextstream" => {
                let mode = if given(&args, 0) {
                    it.to_num(&arg(&args, 0))? as i32
                } else {
                    1
                };
                TextStream::open(&self.0, mode, false, false)?
            }
            _ => return Err(Ex::no_member(name)),
        })
    }
}

struct FolderObj(String);

impl Host for FolderObj {
    fn type_name(&self) -> &str {
        "Folder"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let l = linux(&self.0);
        let children = |dirs: bool| -> Vec<V> {
            let mut names: Vec<String> = std::fs::read_dir(&l)
                .map(|r| {
                    r.filter_map(|e| e.ok())
                        .filter(|e| e.path().is_dir() == dirs)
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            let base = self.0.trim_end_matches(['\\', '/']).to_string();
            names
                .into_iter()
                .map(|n| {
                    let p = format!("{base}\\{n}");
                    if dirs {
                        V::host(FolderObj(p))
                    } else {
                        V::host(FileObj(p))
                    }
                })
                .collect()
        };
        Ok(match lower(name).as_str() {
            "" | "path" | "shortpath" => V::str(&self.0),
            "name" => V::str(win_parts(&self.0).1),
            "parentfolder" => V::host(FolderObj(win_parts(&self.0).0)),
            "files" => V::host(Collection(children(false))),
            "subfolders" => V::host(Collection(children(true))),
            "isrootfolder" => V::Bool(self.0.trim_end_matches('\\').len() <= 2),
            "attributes" => V::Num(16.0),
            "size" => V::Num(0.0),
            "delete" => {
                std::fs::remove_dir_all(&l).map_err(|_| path_not_found(&self.0))?;
                V::Empty
            }
            "copy" | "move" => FsoObj.get(
                it,
                &format!("{name}folder"),
                vec![V::str(&self.0), arg(&args, 0), arg(&args, 1)],
            )?,
            "createtextfile" => {
                let p = format!(
                    "{}\\{}",
                    self.0.trim_end_matches('\\'),
                    it.to_str(&arg(&args, 0))
                );
                TextStream::create(&p, false)?
            }
            _ => return Err(Ex::no_member(name)),
        })
    }
}

/// Collection en lecture (`Files`, `SubFolders`, `Matches`…).
struct Collection(Vec<V>);

impl Host for Collection {
    fn type_name(&self) -> &str {
        "Collection"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        match lower(name).as_str() {
            "count" | "length" => Ok(V::Num(self.0.len() as f64)),
            "" | "item" => {
                let k = arg(&args, 0);
                if let V::Str(key) = &k {
                    // Fichier ou dossier par son nom.
                    for v in &self.0 {
                        if let V::Host(h) = v {
                            let n = h.get(it, "name", Vec::new())?;
                            if it.to_str(&n).eq_ignore_ascii_case(key) {
                                return Ok(v.clone());
                            }
                        }
                    }
                }
                let i = it.to_num(&k)? as usize;
                self.0
                    .get(i)
                    .cloned()
                    .ok_or_else(|| Ex::new(5, "indice hors limites"))
            }
            _ => Err(Ex::no_member(name)),
        }
    }
    fn items(&self, _it: &mut Interp) -> Option<Vec<V>> {
        Some(self.0.clone())
    }
}

struct TextStream {
    path: String,
    /// Lecture : texte et position (en caractères).
    read: Option<(Vec<char>, RefCell<usize>)>,
    /// Écriture : fichier et encodage.
    write: RefCell<Option<(std::fs::File, bool)>>,
    line: RefCell<usize>,
}

impl TextStream {
    fn open(p: &str, mode: i32, create: bool, unicode: bool) -> R<V> {
        let l = linux(p);
        if mode == 1 {
            let data = std::fs::read(&l).map_err(|_| file_not_found(p))?;
            let text = super::decode_source(&data).replace("\r\n", "\n");
            return Ok(V::host(TextStream {
                path: p.into(),
                read: Some((text.chars().collect(), RefCell::new(0))),
                write: RefCell::new(None),
                line: RefCell::new(1),
            }));
        }
        if !l.exists() && !create {
            return Err(file_not_found(p));
        }
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(mode == 8)
            .truncate(mode != 8)
            .open(&l)
            .map_err(|_| path_not_found(p))?;
        Ok(V::host(TextStream {
            path: p.into(),
            read: None,
            write: RefCell::new(Some((f, unicode))),
            line: RefCell::new(1),
        }))
    }

    fn create(p: &str, unicode: bool) -> R<V> {
        let l = linux(p);
        let mut f = std::fs::File::create(&l).map_err(|_| path_not_found(p))?;
        if unicode {
            use std::io::Write;
            let _ = f.write_all(&[0xFF, 0xFE]);
        }
        Ok(V::host(TextStream {
            path: p.into(),
            read: None,
            write: RefCell::new(Some((f, unicode))),
            line: RefCell::new(1),
        }))
    }

    fn put_text(&self, text: &str) -> R<()> {
        use std::io::Write;
        let mut w = self.write.borrow_mut();
        let Some((f, unicode)) = w.as_mut() else {
            return Err(Ex::new(
                54,
                format!("fichier ouvert en lecture : {}", self.path),
            ));
        };
        let bytes: Vec<u8> = if *unicode {
            text.encode_utf16().flat_map(u16::to_le_bytes).collect()
        } else {
            crate::dlls::encode_ansi(text)
        };
        f.write_all(&bytes)
            .map_err(|e| Ex::new(57, format!("écriture impossible : {e}")))
    }

    fn reader(&self) -> R<(&Vec<char>, &RefCell<usize>)> {
        self.read
            .as_ref()
            .map(|(t, p)| (t, p))
            .ok_or_else(|| Ex::new(54, format!("fichier ouvert en écriture : {}", self.path)))
    }
}

impl Host for TextStream {
    fn type_name(&self) -> &str {
        "TextStream"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        Ok(match lower(name).as_str() {
            "write" => {
                self.put_text(&it.to_str(&arg(&args, 0)))?;
                V::Empty
            }
            "writeline" => {
                self.put_text(&format!("{}\r\n", it.to_str(&arg(&args, 0))))?;
                *self.line.borrow_mut() += 1;
                V::Empty
            }
            "writeblanklines" => {
                let n = it.to_num(&arg(&args, 0))?.max(0.0) as usize;
                self.put_text(&"\r\n".repeat(n))?;
                V::Empty
            }
            "close" => {
                self.write.borrow_mut().take();
                V::Empty
            }
            "readline" | "skipline" => {
                let (t, pos) = self.reader()?;
                let mut p = pos.borrow_mut();
                if *p >= t.len() {
                    return Err(Ex::new(62, "lecture après la fin du fichier"));
                }
                let start = *p;
                while *p < t.len() && t[*p] != '\n' {
                    *p += 1;
                }
                let line: String = t[start..*p].iter().collect();
                if *p < t.len() {
                    *p += 1;
                }
                *self.line.borrow_mut() += 1;
                V::str(line)
            }
            "readall" => {
                let (t, pos) = self.reader()?;
                let mut p = pos.borrow_mut();
                let rest: String = t[*p..].iter().collect();
                *p = t.len();
                V::str(rest.replace('\n', "\r\n"))
            }
            "read" | "skip" => {
                let n = it.to_num(&arg(&args, 0))?.max(0.0) as usize;
                let (t, pos) = self.reader()?;
                let mut p = pos.borrow_mut();
                let end = (*p + n).min(t.len());
                let s: String = t[*p..end].iter().collect();
                *p = end;
                V::str(s)
            }
            "atendofstream" => {
                let (t, pos) = self.reader()?;
                V::Bool(*pos.borrow() >= t.len())
            }
            "atendofline" => {
                let (t, pos) = self.reader()?;
                let p = *pos.borrow();
                V::Bool(p >= t.len() || t[p] == '\n')
            }
            "line" => V::Num(*self.line.borrow() as f64),
            _ => return Err(Ex::no_member(name)),
        })
    }
}

// ---------- Scripting.Dictionary ----------

struct Dictionary {
    items: RefCell<Vec<(V, V)>>,
    text_compare: RefCell<bool>,
}

impl Dictionary {
    fn find(&self, it: &Interp, key: &V) -> Option<usize> {
        let tc = *self.text_compare.borrow();
        self.items
            .borrow()
            .iter()
            .position(|(k, _)| match (k, key) {
                (V::Str(a), V::Str(b)) => {
                    if tc {
                        a.to_lowercase() == b.to_lowercase()
                    } else {
                        a == b
                    }
                }
                (V::Num(a), V::Num(b)) => a == b,
                (a, b) if a.is_object() || b.is_object() => super::same_object(a, b),
                (a, b) => {
                    it.to_str(a) == it.to_str(b)
                        && std::mem::discriminant(a) == std::mem::discriminant(b)
                }
            })
    }
}

impl Host for Dictionary {
    fn type_name(&self) -> &str {
        "Dictionary"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let key = arg(&args, 0);
        Ok(match lower(name).as_str() {
            "add" => {
                if self.find(it, &key).is_some() {
                    return Err(Ex::new(
                        457,
                        format!("clé déjà présente : {}", it.to_str(&key)),
                    ));
                }
                self.items.borrow_mut().push((key, arg(&args, 1)));
                V::Empty
            }
            "exists" => V::Bool(self.find(it, &key).is_some()),
            "" | "item" => match self.find(it, &key) {
                Some(i) => self.items.borrow()[i].1.clone(),
                None => {
                    // Comme sous Windows : lire une clé absente l'ajoute.
                    self.items.borrow_mut().push((key, V::Empty));
                    V::Empty
                }
            },
            "remove" => {
                let i = self
                    .find(it, &key)
                    .ok_or_else(|| Ex::new(32811, "élément introuvable"))?;
                self.items.borrow_mut().remove(i);
                V::Empty
            }
            "removeall" => {
                self.items.borrow_mut().clear();
                V::Empty
            }
            "keys" => V::arr(self.items.borrow().iter().map(|(k, _)| k.clone()).collect()),
            "items" => V::arr(self.items.borrow().iter().map(|(_, v)| v.clone()).collect()),
            "count" => V::Num(self.items.borrow().len() as f64),
            "comparemode" => V::Num(*self.text_compare.borrow() as i32 as f64),
            _ => return Err(Ex::no_member(name)),
        })
    }
    fn put(&self, it: &mut Interp, name: &str, args: Vec<V>, value: V) -> R<()> {
        let key = arg(&args, 0);
        match lower(name).as_str() {
            "" | "item" => {
                match self.find(it, &key) {
                    Some(i) => self.items.borrow_mut()[i].1 = value,
                    None => self.items.borrow_mut().push((key, value)),
                }
                Ok(())
            }
            "key" => {
                let i = self
                    .find(it, &key)
                    .ok_or_else(|| Ex::new(32811, "élément introuvable"))?;
                self.items.borrow_mut()[i].0 = value;
                Ok(())
            }
            "comparemode" => {
                *self.text_compare.borrow_mut() = it.to_num(&value)? as i32 == 1;
                Ok(())
            }
            _ => Err(Ex::no_member(name)),
        }
    }
    fn items(&self, _it: &mut Interp) -> Option<Vec<V>> {
        Some(self.items.borrow().iter().map(|(k, _)| k.clone()).collect())
    }
}

// ---------- RegExp ----------

pub struct RegExpObj {
    pattern: RefCell<String>,
    ignore_case: RefCell<bool>,
    global: RefCell<bool>,
    multiline: RefCell<bool>,
    last_index: RefCell<usize>,
}

impl RegExpObj {
    fn compiled(&self) -> R<Regex> {
        let mut flags = String::new();
        if *self.ignore_case.borrow() {
            flags.push('i');
        }
        if *self.multiline.borrow() {
            flags.push('m');
        }
        Regex::new(&self.pattern.borrow(), &flags).map_err(|e| Ex::new(5017, e))
    }
}

pub fn regexp(pattern: &str, flags: &str) -> R<V> {
    let obj = RegExpObj {
        pattern: RefCell::new(pattern.to_string()),
        ignore_case: RefCell::new(flags.contains('i')),
        global: RefCell::new(flags.contains('g')),
        multiline: RefCell::new(flags.contains('m')),
        last_index: RefCell::new(0),
    };
    obj.compiled()?;
    Ok(V::host(obj))
}

fn as_regexp(v: &V) -> Option<&RegExpObj> {
    match v {
        V::Host(h) => h.as_any().downcast_ref::<RegExpObj>(),
        _ => None,
    }
}

fn match_value(s: &[char], caps: &[Option<(usize, usize)>]) -> V {
    let (a, b) = caps[0].unwrap_or((0, 0));
    let subs: Vec<V> = caps[1..]
        .iter()
        .map(|c| match c {
            Some((x, y)) => V::str(s[*x..*y].iter().collect::<String>()),
            None => V::Empty,
        })
        .collect();
    V::host(MatchObj {
        value: s[a..b].iter().collect(),
        first: a,
        subs,
    })
}

struct MatchObj {
    value: String,
    first: usize,
    subs: Vec<V>,
}

impl Host for MatchObj {
    fn type_name(&self) -> &str {
        "Match"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, _it: &mut Interp, name: &str, _args: Vec<V>) -> R<V> {
        Ok(match lower(name).as_str() {
            "" | "value" => V::str(&self.value),
            "firstindex" => V::Num(self.first as f64),
            "length" => V::Num(self.value.chars().count() as f64),
            "submatches" => V::host(Collection(self.subs.clone())),
            _ => return Err(Ex::no_member(name)),
        })
    }
}

impl Host for RegExpObj {
    fn type_name(&self) -> &str {
        "IRegExp2"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let text = it.to_str(&arg(&args, 0));
        Ok(match name.to_ascii_lowercase().as_str() {
            "pattern" | "source" => V::str(self.pattern.borrow().as_str()),
            "ignorecase" => V::Bool(*self.ignore_case.borrow()),
            "global" => V::Bool(*self.global.borrow()),
            "multiline" => V::Bool(*self.multiline.borrow()),
            "lastindex" => V::Num(*self.last_index.borrow() as f64),
            "test" => {
                let re = self.compiled()?;
                V::Bool(re.is_match(&text))
            }
            "replace" => {
                let re = self.compiled()?;
                V::str(re.replace(&text, &it.to_str(&arg(&args, 1)), *self.global.borrow()))
            }
            "execute" => {
                let re = self.compiled()?;
                let s: Vec<char> = text.chars().collect();
                let found = re.find_all(&s, *self.global.borrow());
                V::host(Collection(
                    found.iter().map(|c| match_value(&s, c)).collect(),
                ))
            }
            "exec" => {
                let re = self.compiled()?;
                let s: Vec<char> = text.chars().collect();
                let global = *self.global.borrow();
                let from = if global { *self.last_index.borrow() } else { 0 };
                match (from <= s.len()).then(|| re.find_at(&s, from)).flatten() {
                    Some(caps) => {
                        let (a, b) = caps[0].unwrap();
                        if global {
                            *self.last_index.borrow_mut() = if a == b { b + 1 } else { b };
                        }
                        V::arr(
                            caps.iter()
                                .map(|c| match c {
                                    Some((x, y)) => V::str(s[*x..*y].iter().collect::<String>()),
                                    None => V::Empty,
                                })
                                .collect(),
                        )
                    }
                    None => {
                        *self.last_index.borrow_mut() = 0;
                        V::Null
                    }
                }
            }
            "tostring" => V::str(format!("/{}/", self.pattern.borrow())),
            _ => return Err(Ex::no_member(name)),
        })
    }
    fn put(&self, it: &mut Interp, name: &str, _args: Vec<V>, value: V) -> R<()> {
        match name.to_ascii_lowercase().as_str() {
            "pattern" => *self.pattern.borrow_mut() = it.to_str(&value),
            "ignorecase" => *self.ignore_case.borrow_mut() = it.truthy(&value)?,
            "global" => *self.global.borrow_mut() = it.truthy(&value)?,
            "multiline" => *self.multiline.borrow_mut() = it.truthy(&value)?,
            "lastindex" => *self.last_index.borrow_mut() = it.to_num(&value)?.max(0.0) as usize,
            _ => return Err(Ex::no_member(name)),
        }
        Ok(())
    }
}

// ---------- Err (VBScript) et Math (JScript) ----------

struct ErrObj;

impl Host for ErrObj {
    fn type_name(&self) -> &str {
        "ErrObject"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        Ok(match lower(name).as_str() {
            "" | "number" => V::Num(it.err.0 as f64),
            "description" => V::str(&it.err.1),
            "source" => V::str(&it.err.2),
            "helpfile" => V::str(""),
            "helpcontext" => V::Num(0.0),
            "clear" => {
                it.err = (0, String::new(), String::new());
                V::Empty
            }
            "raise" => {
                let n = it.to_num(&arg(&args, 0))? as i64 as i32;
                let desc = if given(&args, 2) {
                    it.to_str(&arg(&args, 2))
                } else {
                    format!("erreur {n}")
                };
                return Err(Ex::Err(n, desc));
            }
            _ => return Err(Ex::no_member(name)),
        })
    }
    fn put(&self, it: &mut Interp, name: &str, _args: Vec<V>, value: V) -> R<()> {
        match lower(name).as_str() {
            "" | "number" => it.err.0 = it.to_num(&value)? as i64 as i32,
            "description" => it.err.1 = it.to_str(&value),
            "source" => it.err.2 = it.to_str(&value),
            _ => return Err(Ex::no_member(name)),
        }
        Ok(())
    }
}

struct MathObj;

impl Host for MathObj {
    fn type_name(&self) -> &str {
        "Math"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn get(&self, it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
        let n = |it: &Interp, i: usize| it.to_num(&arg(&args, i));
        Ok(V::Num(match name {
            "PI" => std::f64::consts::PI,
            "E" => std::f64::consts::E,
            "LN2" => std::f64::consts::LN_2,
            "LN10" => std::f64::consts::LN_10,
            "LOG2E" => std::f64::consts::LOG2_E,
            "LOG10E" => std::f64::consts::LOG10_E,
            "SQRT2" => std::f64::consts::SQRT_2,
            "SQRT1_2" => std::f64::consts::FRAC_1_SQRT_2,
            "abs" => n(it, 0)?.abs(),
            "floor" => n(it, 0)?.floor(),
            "ceil" => n(it, 0)?.ceil(),
            "round" => (n(it, 0)? + 0.5).floor(),
            "sqrt" => n(it, 0)?.sqrt(),
            "pow" => n(it, 0)?.powf(n(it, 1)?),
            "exp" => n(it, 0)?.exp(),
            "log" => n(it, 0)?.ln(),
            "sin" => n(it, 0)?.sin(),
            "cos" => n(it, 0)?.cos(),
            "tan" => n(it, 0)?.tan(),
            "asin" => n(it, 0)?.asin(),
            "acos" => n(it, 0)?.acos(),
            "atan" => n(it, 0)?.atan(),
            "atan2" => n(it, 0)?.atan2(n(it, 1)?),
            "random" => it.random(),
            "max" | "min" => {
                let mut acc = if name == "max" {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
                for a in &args {
                    let x = it.to_num(a)?;
                    if x.is_nan() {
                        return Ok(V::Num(f64::NAN));
                    }
                    acc = if name == "max" {
                        acc.max(x)
                    } else {
                        acc.min(x)
                    };
                }
                acc
            }
            _ => return Err(Ex::no_member(name)),
        }))
    }
}

/// `CreateObject` / `new ActiveXObject` : objets connus par leur ProgID.
pub fn create_object(progid: &str) -> R<V> {
    Ok(match progid.to_ascii_lowercase().as_str() {
        "wscript.shell" => V::host(ShellObj),
        "scripting.filesystemobject" => V::host(FsoObj),
        "scripting.dictionary" => V::host(Dictionary {
            items: RefCell::new(Vec::new()),
            text_compare: RefCell::new(false),
        }),
        "vbscript.regexp" => regexp("", "")?,
        "windowsinstaller.installer" => V::host(InstallerObj),
        _ => {
            return Err(Ex::new(
                429,
                format!("objet « {progid} » non disponible dans Winlux"),
            ))
        }
    })
}

// ---------- fonctions intégrées ----------

const JS_NATIVES: &[&str] = &[
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "String",
    "Number",
    "Boolean",
    "Array",
    "Object",
    "Error",
    "RegExp",
    "ActiveXObject",
    "escape",
    "unescape",
    "encodeURIComponent",
    "decodeURIComponent",
    "encodeURI",
    "decodeURI",
    "Date",
    "GetObject",
];

const VB_NATIVES: &[&str] = &[
    "len",
    "left",
    "right",
    "mid",
    "instr",
    "instrrev",
    "lcase",
    "ucase",
    "trim",
    "ltrim",
    "rtrim",
    "replace",
    "split",
    "join",
    "cstr",
    "cint",
    "clng",
    "cdbl",
    "csng",
    "cbool",
    "cbyte",
    "ccur",
    "cdate",
    "int",
    "fix",
    "abs",
    "sgn",
    "round",
    "sqr",
    "exp",
    "log",
    "isempty",
    "isnull",
    "isobject",
    "isnumeric",
    "isarray",
    "isdate",
    "chr",
    "chrw",
    "asc",
    "ascw",
    "ubound",
    "lbound",
    "array",
    "createobject",
    "getobject",
    "msgbox",
    "inputbox",
    "strcomp",
    "string",
    "space",
    "strreverse",
    "hex",
    "oct",
    "typename",
    "vartype",
    "now",
    "date",
    "time",
    "timer",
    "filter",
    "rnd",
    "scriptengine",
    "scriptenginemajorversion",
    "scriptengineminorversion",
    "getlocale",
    "eval",
    "escape",
    "unescape",
    "formatnumber",
];

pub fn install_globals(it: &mut Interp) {
    match it.lang {
        Lang::JScript => {
            for n in JS_NATIVES {
                it.define_global(n, V::Native(n));
            }
            it.define_global("Math", V::host(MathObj));
            it.define_global("undefined", V::Empty);
            it.define_global("NaN", V::Num(f64::NAN));
            it.define_global("Infinity", V::Num(f64::INFINITY));
        }
        Lang::VBScript => {
            for n in VB_NATIVES {
                it.define_global(n, V::Native(n));
            }
            it.define_global("err", V::host(ErrObj));
            let consts: &[(&str, V)] = &[
                ("vbcrlf", V::str("\r\n")),
                ("vbnewline", V::str("\r\n")),
                ("vbcr", V::str("\r")),
                ("vblf", V::str("\n")),
                ("vbtab", V::str("\t")),
                ("vbnullstring", V::str("")),
                ("vbnullchar", V::str("\0")),
                ("vbbinarycompare", V::Num(0.0)),
                ("vbtextcompare", V::Num(1.0)),
                ("vbokonly", V::Num(0.0)),
                ("vbokcancel", V::Num(1.0)),
                ("vbabortretryignore", V::Num(2.0)),
                ("vbyesnocancel", V::Num(3.0)),
                ("vbyesno", V::Num(4.0)),
                ("vbretrycancel", V::Num(5.0)),
                ("vbcritical", V::Num(16.0)),
                ("vbquestion", V::Num(32.0)),
                ("vbexclamation", V::Num(48.0)),
                ("vbinformation", V::Num(64.0)),
                ("vbok", V::Num(1.0)),
                ("vbcancel", V::Num(2.0)),
                ("vbabort", V::Num(3.0)),
                ("vbretry", V::Num(4.0)),
                ("vbignore", V::Num(5.0)),
                ("vbyes", V::Num(6.0)),
                ("vbno", V::Num(7.0)),
                ("vbtrue", V::Num(-1.0)),
                ("vbfalse", V::Num(0.0)),
                ("vbusedefault", V::Num(-2.0)),
                ("vbempty", V::Num(0.0)),
                ("vbnull", V::Num(1.0)),
                ("vbinteger", V::Num(2.0)),
                ("vblong", V::Num(3.0)),
                ("vbsingle", V::Num(4.0)),
                ("vbdouble", V::Num(5.0)),
                ("vbdate", V::Num(7.0)),
                ("vbstring", V::Num(8.0)),
                ("vbobject", V::Num(9.0)),
                ("vberror", V::Num(10.0)),
                ("vbboolean", V::Num(11.0)),
                ("vbvariant", V::Num(12.0)),
                ("vbarray", V::Num(8192.0)),
                ("vbobjecterror", V::Num(-2147221504.0)),
            ];
            for (n, v) in consts {
                it.define_global(n, v.clone());
            }
        }
    }
}

fn type_name(it: &Interp, v: &V) -> String {
    match v {
        V::Empty => "Empty".into(),
        V::Null => "Null".into(),
        V::Nothing => "Nothing".into(),
        V::Bool(_) => "Boolean".into(),
        V::Num(n) if *n == n.trunc() && n.abs() <= 32767.0 => "Integer".into(),
        V::Num(n) if *n == n.trunc() && n.abs() <= 2147483647.0 => "Long".into(),
        V::Num(_) => "Double".into(),
        V::Str(_) => "String".into(),
        V::Arr(_) => "Variant()".into(),
        V::Host(h) => h.type_name().to_string(),
        _ => {
            let _ = it;
            "Object".into()
        }
    }
}

fn var_type(v: &V) -> f64 {
    match v {
        V::Empty => 0.0,
        V::Null => 1.0,
        V::Bool(_) => 11.0,
        V::Num(n) if *n == n.trunc() && n.abs() <= 32767.0 => 2.0,
        V::Num(n) if *n == n.trunc() && n.abs() <= 2147483647.0 => 3.0,
        V::Num(_) => 5.0,
        V::Str(_) => 8.0,
        V::Arr(_) => 8204.0,
        _ => 9.0,
    }
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// `InStr` à partir de `start` (indice en caractères, 0 = début) ;
/// rend la position (1 = premier caractère) ou 0.
fn find_from(hay: &str, needle: &str, start: usize, text: bool) -> usize {
    let (h, n) = if text {
        (hay.to_lowercase(), needle.to_lowercase())
    } else {
        (hay.to_string(), needle.to_string())
    };
    let hc = chars(&h);
    let nc = chars(&n);
    if start > hc.len() {
        return 0;
    }
    if nc.is_empty() {
        return start + 1;
    }
    (start..=hc.len().saturating_sub(nc.len()))
        .find(|&i| hc[i..i + nc.len()] == nc[..])
        .map(|i| i + 1)
        .unwrap_or(0)
}

fn replace_text(s: &str, find: &str, with: &str, text: bool, count: i64) -> String {
    if find.is_empty() {
        return s.to_string();
    }
    let sc = chars(s);
    let fc = chars(&if text {
        find.to_lowercase()
    } else {
        find.to_string()
    });
    let lc: Vec<char> = if text {
        sc.iter().flat_map(|c| c.to_lowercase().take(1)).collect()
    } else {
        sc.clone()
    };
    let mut out = String::new();
    let mut i = 0;
    let mut done = 0;
    while i < sc.len() {
        if (count < 0 || done < count) && i + fc.len() <= lc.len() && lc[i..i + fc.len()] == fc[..]
        {
            out.push_str(with);
            i += fc.len();
            done += 1;
        } else {
            out.push(sc[i]);
            i += 1;
        }
    }
    out
}

pub fn native(it: &mut Interp, name: &'static str, args: Vec<V>) -> R<V> {
    let a = |i: usize| arg(&args, i);
    let s = |it: &Interp, i: usize| it.to_str(&arg(&args, i));
    let n = |it: &Interp, i: usize| it.to_num(&arg(&args, i));
    let text_cmp = |it: &Interp, i: usize| -> R<bool> {
        Ok(given(&args, i) && it.to_num(&arg(&args, i))? as i32 == 1)
    };
    Ok(match name {
        // ----- JScript -----
        "parseInt" => {
            let t = s(it, 0);
            let t = t.trim();
            let mut radix = if given(&args, 1) {
                n(it, 1)? as u32
            } else {
                10
            };
            let (neg, mut body) = match t.strip_prefix('-') {
                Some(r) => (true, r),
                None => (false, t.strip_prefix('+').unwrap_or(t)),
            };
            if (radix == 16 || !given(&args, 1))
                && (body.starts_with("0x") || body.starts_with("0X"))
            {
                body = &body[2..];
                radix = 16;
            }
            if !(2..=36).contains(&radix) {
                return Ok(V::Num(f64::NAN));
            }
            let digits: String = body.chars().take_while(|c| c.is_digit(radix)).collect();
            match i64::from_str_radix(&digits, radix) {
                Ok(v) => V::Num(if neg { -v } else { v } as f64),
                Err(_) => V::Num(f64::NAN),
            }
        }
        "parseFloat" => {
            let t = s(it, 0);
            let t = t.trim_start();
            let mut end = 0;
            let mut best = f64::NAN;
            for (i, _) in t.char_indices().chain(std::iter::once((t.len(), ' '))) {
                if i == 0 {
                    continue;
                }
                if let Ok(v) = t[..i].parse::<f64>() {
                    best = v;
                    end = i;
                } else if i > end + 2 {
                    break;
                }
            }
            V::Num(best)
        }
        "isNaN" => V::Bool(n(it, 0)?.is_nan()),
        "isFinite" => V::Bool(n(it, 0)?.is_finite()),
        "String" => V::str(if args.is_empty() {
            String::new()
        } else {
            s(it, 0)
        }),
        "Number" => V::Num(if args.is_empty() { 0.0 } else { n(it, 0)? }),
        "Boolean" => V::Bool(it.truthy(&a(0))?),
        "Array" | "Object" | "Error" | "RegExp" | "Date" => construct_native(it, name, args)?,
        "ActiveXObject" | "GetObject" | "getobject" => {
            return Err(Ex::new(
                429,
                "« new ActiveXObject(…) » attendu pour créer un objet",
            ))
        }
        "escape" | "encodeURIComponent" | "encodeURI" => {
            let t = s(it, 0);
            let keep = |c: char| {
                c.is_ascii_alphanumeric()
                    || "-_.!~*'()".contains(c)
                    || name == "encodeURI" && ";/?:@&=+$,#".contains(c)
            };
            let mut out = String::new();
            for c in t.chars() {
                if keep(c) {
                    out.push(c);
                } else {
                    let mut b = [0u8; 4];
                    for byte in c.encode_utf8(&mut b).bytes() {
                        out.push_str(&format!("%{byte:02X}"));
                    }
                }
            }
            V::str(out)
        }
        "unescape" | "decodeURIComponent" | "decodeURI" => {
            let t = s(it, 0);
            let b = t.as_bytes();
            let mut out = Vec::new();
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'%' && i + 2 < b.len() {
                    if let Ok(v) = u8::from_str_radix(&t[i + 1..i + 3], 16) {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                }
                out.push(b[i]);
                i += 1;
            }
            V::str(String::from_utf8_lossy(&out))
        }
        "hasOwnProperty" => V::Bool(false),
        // ----- VBScript -----
        "len" => match a(0) {
            V::Null => V::Null,
            v => V::Num(it.to_str(&v).chars().count() as f64),
        },
        "left" => V::str(
            s(it, 0)
                .chars()
                .take(n(it, 1)?.max(0.0) as usize)
                .collect::<String>(),
        ),
        "right" => {
            let c = chars(&s(it, 0));
            let k = (n(it, 1)?.max(0.0) as usize).min(c.len());
            V::str(c[c.len() - k..].iter().collect::<String>())
        }
        "mid" => {
            let c = chars(&s(it, 0));
            let start = (n(it, 1)?.max(1.0) as usize - 1).min(c.len());
            let len = if given(&args, 2) {
                n(it, 2)?.max(0.0) as usize
            } else {
                c.len()
            };
            V::str(
                c[start..(start + len).min(c.len())]
                    .iter()
                    .collect::<String>(),
            )
        }
        "instr" => {
            let (start, h, nd, cmp) = if args.len() >= 3 && matches!(a(0), V::Num(_)) {
                (
                    n(it, 0)?.max(1.0) as usize - 1,
                    s(it, 1),
                    s(it, 2),
                    text_cmp(it, 3)?,
                )
            } else {
                (0, s(it, 0), s(it, 1), false)
            };
            V::Num(find_from(&h, &nd, start, cmp) as f64)
        }
        "instrrev" => {
            let h = chars(&s(it, 0));
            let nd = chars(&s(it, 1));
            let start = if given(&args, 2) && n(it, 2)? > 0.0 {
                n(it, 2)? as usize
            } else {
                h.len()
            };
            let text = text_cmp(it, 3)?;
            let eq = |x: &[char], y: &[char]| {
                if text {
                    x.iter().collect::<String>().to_lowercase()
                        == y.iter().collect::<String>().to_lowercase()
                } else {
                    x == y
                }
            };
            let mut found = 0;
            if nd.len() <= start.min(h.len()) {
                for i in (0..=start.min(h.len()) - nd.len()).rev() {
                    if eq(&h[i..i + nd.len()], &nd) {
                        found = i + 1;
                        break;
                    }
                }
            }
            V::Num(found as f64)
        }
        "lcase" => V::str(s(it, 0).to_lowercase()),
        "ucase" => V::str(s(it, 0).to_uppercase()),
        "trim" => V::str(s(it, 0).trim_matches(' ')),
        "ltrim" => V::str(s(it, 0).trim_start_matches(' ')),
        "rtrim" => V::str(s(it, 0).trim_end_matches(' ')),
        "replace" => {
            let src = s(it, 0);
            let start = if given(&args, 3) {
                n(it, 3)?.max(1.0) as usize - 1
            } else {
                0
            };
            let count = if given(&args, 4) {
                n(it, 4)? as i64
            } else {
                -1
            };
            let rest: String = src.chars().skip(start).collect();
            V::str(replace_text(
                &rest,
                &s(it, 1),
                &s(it, 2),
                text_cmp(it, 5)?,
                count,
            ))
        }
        "split" => {
            let src = s(it, 0);
            let delim = if given(&args, 1) {
                s(it, 1)
            } else {
                " ".into()
            };
            let limit = if given(&args, 2) {
                n(it, 2)? as i64
            } else {
                -1
            };
            if src.is_empty() {
                V::arr(Vec::new())
            } else if delim.is_empty() {
                V::arr(vec![V::str(src)])
            } else {
                let parts: Vec<V> = if limit > 0 {
                    src.splitn(limit as usize, delim.as_str())
                        .map(V::str)
                        .collect()
                } else {
                    src.split(delim.as_str()).map(V::str).collect()
                };
                V::arr(parts)
            }
        }
        "join" => {
            let delim = if given(&args, 1) {
                s(it, 1)
            } else {
                " ".into()
            };
            match a(0) {
                V::Arr(x) => V::str(
                    x.borrow()
                        .iter()
                        .map(|v| it.to_str(v))
                        .collect::<Vec<_>>()
                        .join(&delim),
                ),
                _ => return Err(Ex::mismatch("Join attend un tableau")),
            }
        }
        "cstr" => V::str(s(it, 0)),
        "cint" | "clng" | "cbyte" => {
            let v = round_even(n(it, 0)?);
            let limit = if name == "cint" {
                32767.0
            } else if name == "cbyte" {
                255.0
            } else {
                2147483647.0
            };
            if v.abs() > limit + 1.0 || name == "cbyte" && v < 0.0 {
                return Err(Ex::new(6, "dépassement de capacité"));
            }
            V::Num(v)
        }
        "cdbl" | "csng" | "ccur" => V::Num(n(it, 0)?),
        "cbool" => V::Bool(it.truthy(&a(0))?),
        "cdate" => V::str(s(it, 0)),
        "int" => V::Num(n(it, 0)?.floor()),
        "fix" => V::Num(n(it, 0)?.trunc()),
        "abs" => V::Num(n(it, 0)?.abs()),
        "sgn" => {
            let x = n(it, 0)?;
            V::Num(if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            })
        }
        "round" => {
            let d = if given(&args, 1) { n(it, 1)? as i32 } else { 0 };
            let f = 10f64.powi(d);
            V::Num(round_even(n(it, 0)? * f) / f)
        }
        "sqr" => V::Num(n(it, 0)?.sqrt()),
        "exp" => V::Num(n(it, 0)?.exp()),
        "log" => V::Num(n(it, 0)?.ln()),
        "rnd" => V::Num(it.random()),
        "isempty" => V::Bool(matches!(a(0), V::Empty)),
        "isnull" => V::Bool(matches!(a(0), V::Null)),
        "isobject" => V::Bool(matches!(a(0), V::Host(_) | V::Nothing | V::Obj(_))),
        "isarray" => V::Bool(matches!(a(0), V::Arr(_))),
        "isdate" => V::Bool(false),
        "isnumeric" => V::Bool(match a(0) {
            V::Num(_) | V::Bool(_) | V::Empty => true,
            V::Str(t) => parse_num(&t).is_some(),
            _ => false,
        }),
        "chr" | "chrw" => {
            let code = n(it, 0)? as i64;
            let c = if name == "chr" && (128..256).contains(&code) {
                crate::dlls::decode_ansi(&[code as u8]).chars().next()
            } else {
                char::from_u32(code as u32)
            };
            V::str(
                c.ok_or_else(|| Ex::new(5, "code de caractère incorrect"))?
                    .to_string(),
            )
        }
        "asc" | "ascw" => {
            let t = s(it, 0);
            let c = t
                .chars()
                .next()
                .ok_or_else(|| Ex::new(5, "chaîne vide passée à Asc"))?;
            let code = if name == "asc" && (c as u32) > 127 {
                crate::dlls::encode_ansi(&c.to_string())
                    .first()
                    .copied()
                    .unwrap_or(b'?') as u32
            } else {
                c as u32
            };
            V::Num(code as f64)
        }
        "ubound" | "lbound" => {
            let dim = if given(&args, 1) {
                n(it, 1)? as usize
            } else {
                1
            };
            let mut cur = a(0);
            for _ in 1..dim {
                cur = match cur {
                    V::Arr(x) => x.borrow().first().cloned().unwrap_or(V::Empty),
                    _ => V::Empty,
                };
            }
            match cur {
                V::Arr(x) => V::Num(if name == "lbound" {
                    0.0
                } else {
                    x.borrow().len() as f64 - 1.0
                }),
                _ => return Err(Ex::mismatch("UBound attend un tableau")),
            }
        }
        "array" => V::arr(args.clone()),
        "createobject" => create_object(&s(it, 0))?,
        "msgbox" => {
            let text = s(it, 0);
            it.messages.push(text.clone());
            eprintln!(
                "msi.dll : MsgBox : {}",
                text.replace("\r\n", " ").replace('\n', " ")
            );
            let buttons = if given(&args, 1) {
                n(it, 1)? as i64 & 7
            } else {
                0
            };
            V::Num(match buttons {
                2 => 3.0,
                3 | 4 => 6.0,
                5 => 4.0,
                _ => 1.0,
            })
        }
        "inputbox" => V::str(if given(&args, 2) {
            s(it, 2)
        } else {
            String::new()
        }),
        "strcomp" => {
            let (x, y) = if text_cmp(it, 2)? {
                (s(it, 0).to_lowercase(), s(it, 1).to_lowercase())
            } else {
                (s(it, 0), s(it, 1))
            };
            V::Num(match x.cmp(&y) {
                std::cmp::Ordering::Less => -1.0,
                std::cmp::Ordering::Equal => 0.0,
                std::cmp::Ordering::Greater => 1.0,
            })
        }
        "string" => {
            let count = n(it, 0)?.max(0.0) as usize;
            let c = match a(1) {
                V::Num(code) => char::from_u32(code as u32).unwrap_or(' '),
                v => it.to_str(&v).chars().next().unwrap_or(' '),
            };
            V::str(std::iter::repeat_n(c, count).collect::<String>())
        }
        "space" => V::str(" ".repeat(n(it, 0)?.max(0.0) as usize)),
        "strreverse" => V::str(s(it, 0).chars().rev().collect::<String>()),
        "hex" => V::str(format!("{:X}", n(it, 0)?.round() as i64 as i32)),
        "oct" => V::str(format!("{:o}", n(it, 0)?.round() as i64 as i32)),
        "typename" => V::str(type_name(it, &a(0))),
        "vartype" => V::Num(var_type(&a(0))),
        "now" | "date" | "time" => V::str(clock_text(name)),
        "timer" => {
            let d = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            V::Num((d.as_secs() % 86400) as f64 + d.subsec_millis() as f64 / 1000.0)
        }
        "filter" => {
            let include = !given(&args, 2) || it.truthy(&a(2))?;
            let needle = s(it, 1);
            let text = text_cmp(it, 3)?;
            match a(0) {
                V::Arr(x) => V::arr(
                    x.borrow()
                        .iter()
                        .filter(|v| (find_from(&it.to_str(v), &needle, 0, text) > 0) == include)
                        .cloned()
                        .collect(),
                ),
                _ => return Err(Ex::mismatch("Filter attend un tableau")),
            }
        }
        "scriptengine" => V::str("VBScript"),
        "scriptenginemajorversion" => V::Num(5.0),
        "scriptengineminorversion" => V::Num(8.0),
        "getlocale" => V::Num(1036.0),
        "formatnumber" => {
            let d = if given(&args, 1) {
                n(it, 1)?.max(0.0) as usize
            } else {
                2
            };
            V::str(format!("{:.*}", d, n(it, 0)?))
        }
        "eval" => return Err(Ex::new(5, "Eval n'est pas pris en charge")),
        other => {
            return Err(Ex::new(
                438,
                format!("fonction « {other} » non prise en charge"),
            ))
        }
    })
}

/// Date et heure locales au format « jj/mm/aaaa hh:mm:ss ».
fn clock_text(which: &str) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Jours depuis 1970 → date civile (algorithme public de H. Hinnant).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    let date = format!("{d:02}/{m:02}/{y:04}");
    let time = format!("{:02}:{:02}:{:02}", rem / 3600, rem / 60 % 60, rem % 60);
    match which {
        "date" => date,
        "time" => time,
        _ => format!("{date} {time}"),
    }
}

pub fn construct_native(it: &mut Interp, name: &str, args: Vec<V>) -> R<V> {
    Ok(match name {
        "ActiveXObject" => create_object(&it.to_str(&arg(&args, 0)))?,
        "Array" => match args.as_slice() {
            [V::Num(n)] => V::arr(vec![V::Empty; (*n).max(0.0) as usize]),
            _ => V::arr(args),
        },
        "Object" => V::Obj(Rc::new(RefCell::new(Vec::new()))),
        "Error" => {
            // new Error(numéro, message) ou new Error(message).
            if args.len() >= 2 {
                make_error(&it.to_str(&arg(&args, 1)), it.to_num(&arg(&args, 0))?)
            } else {
                make_error(&it.to_str(&arg(&args, 0)).replace("undefined", ""), 0.0)
            }
        }
        "RegExp" => {
            let flags = if given(&args, 1) {
                it.to_str(&arg(&args, 1))
            } else {
                String::new()
            };
            match as_regexp(&arg(&args, 0)) {
                Some(r) => regexp(&r.pattern.borrow(), &flags)?,
                None => regexp(&it.to_str(&arg(&args, 0)), &flags)?,
            }
        }
        "String" => V::str(it.to_str(&arg(&args, 0))),
        "Number" => V::Num(it.to_num(&arg(&args, 0))?),
        "Boolean" => V::Bool(it.truthy(&arg(&args, 0))?),
        "Date" => V::str(clock_text("now")),
        _ => return Err(Ex::new(5011, format!("{name} n'est pas un constructeur"))),
    })
}

// ---------- méthodes des chaînes, tableaux et nombres (JScript) ----------

fn clamp_index(i: f64, len: usize) -> usize {
    if i.is_nan() {
        0
    } else if i < 0.0 {
        (len as f64 + i).max(0.0) as usize
    } else {
        (i as usize).min(len)
    }
}

pub fn string_method(it: &mut Interp, s: Rc<str>, name: &str, args: Vec<V>) -> R<V> {
    let c = chars(&s);
    let len = c.len();
    let n = |it: &Interp, i: usize| it.to_num(&arg(&args, i));
    let sub = |a: usize, b: usize| {
        V::str(
            c[a.min(len)..b.min(len).max(a.min(len))]
                .iter()
                .collect::<String>(),
        )
    };
    Ok(match name {
        "charAt" => {
            let i = n(it, 0)?;
            let i = if i.is_nan() { 0.0 } else { i };
            V::str(if i >= 0.0 && (i as usize) < len {
                c[i as usize].to_string()
            } else {
                String::new()
            })
        }
        "charCodeAt" => {
            let i = if given(&args, 0) { n(it, 0)? } else { 0.0 };
            V::Num(if i >= 0.0 && (i as usize) < len {
                c[i as usize] as u32 as f64
            } else {
                f64::NAN
            })
        }
        "indexOf" => {
            let start = if given(&args, 1) {
                n(it, 1)?.max(0.0) as usize
            } else {
                0
            };
            V::Num(find_from(&s, &it.to_str(&arg(&args, 0)), start, false) as f64 - 1.0)
        }
        "lastIndexOf" => {
            let needle = chars(&it.to_str(&arg(&args, 0)));
            let mut found = -1.0;
            if needle.len() <= len {
                for i in (0..=len - needle.len()).rev() {
                    if c[i..i + needle.len()] == needle[..] {
                        found = i as f64;
                        break;
                    }
                }
            }
            V::Num(found)
        }
        "substring" => {
            let a = clamp_index(n(it, 0)?.max(0.0), len);
            let b = if given(&args, 1) {
                clamp_index(n(it, 1)?.max(0.0), len)
            } else {
                len
            };
            sub(a.min(b), a.max(b))
        }
        "substr" => {
            let a = clamp_index(n(it, 0)?, len);
            let l = if given(&args, 1) {
                n(it, 1)?.max(0.0) as usize
            } else {
                len
            };
            sub(a, a + l)
        }
        "slice" => {
            let a = clamp_index(n(it, 0)?, len);
            let b = if given(&args, 1) {
                clamp_index(n(it, 1)?, len)
            } else {
                len
            };
            sub(a, b)
        }
        "toLowerCase" | "toLocaleLowerCase" => V::str(s.to_lowercase()),
        "toUpperCase" | "toLocaleUpperCase" => V::str(s.to_uppercase()),
        "trim" => V::str(s.trim()),
        "toString" | "valueOf" => V::Str(s),
        "concat" => {
            let mut out = s.to_string();
            for a in &args {
                out.push_str(&it.to_str(a));
            }
            V::str(out)
        }
        "split" => {
            let limit = if given(&args, 1) {
                n(it, 1)? as usize
            } else {
                usize::MAX
            };
            let parts: Vec<V> = match arg(&args, 0) {
                V::Empty => vec![V::Str(s.clone())],
                sep => {
                    if let Some(re) = as_regexp(&sep) {
                        let r = re.compiled()?;
                        let mut out = Vec::new();
                        let mut last = 0;
                        for caps in r.find_all(&c, true) {
                            let (a, b) = caps[0].unwrap();
                            if b == a && (a == 0 || a >= len) {
                                continue;
                            }
                            out.push(V::str(c[last..a].iter().collect::<String>()));
                            last = b;
                        }
                        out.push(V::str(c[last..].iter().collect::<String>()));
                        out
                    } else {
                        let sep = it.to_str(&sep);
                        if sep.is_empty() {
                            c.iter().map(|ch| V::str(ch.to_string())).collect()
                        } else {
                            s.split(sep.as_str()).map(V::str).collect()
                        }
                    }
                }
            };
            V::arr(parts.into_iter().take(limit).collect())
        }
        "replace" => {
            let pat = arg(&args, 0);
            let with = arg(&args, 1);
            if let Some(re) = as_regexp(&pat) {
                let r = re.compiled()?;
                let global = *re.global.borrow();
                if let V::Func(_) = with {
                    let mut out = String::new();
                    let mut last = 0;
                    for caps in r.find_all(&c, global) {
                        let (a, b) = caps[0].unwrap();
                        out.extend(&c[last..a]);
                        let mut fargs: Vec<V> = caps
                            .iter()
                            .map(|x| {
                                x.map(|(p, q)| V::str(c[p..q].iter().collect::<String>()))
                                    .unwrap_or(V::Empty)
                            })
                            .collect();
                        fargs.push(V::Num(a as f64));
                        fargs.push(V::Str(s.clone()));
                        let r = it.call_value(with.clone(), fargs, V::Empty)?;
                        out.push_str(&it.to_str(&r));
                        last = b;
                    }
                    out.extend(&c[last..]);
                    V::str(out)
                } else {
                    V::str(r.replace(&s, &it.to_str(&with), global))
                }
            } else {
                let p = it.to_str(&pat);
                match s.find(p.as_str()) {
                    Some(i) => {
                        let rep = if let V::Func(_) = with {
                            let r = it.call_value(
                                with.clone(),
                                vec![
                                    V::str(&p),
                                    V::Num(s[..i].chars().count() as f64),
                                    V::Str(s.clone()),
                                ],
                                V::Empty,
                            )?;
                            it.to_str(&r)
                        } else {
                            it.to_str(&with).replace("$&", &p).replace("$$", "$")
                        };
                        V::str(format!("{}{}{}", &s[..i], rep, &s[i + p.len()..]))
                    }
                    None => V::Str(s),
                }
            }
        }
        "match" => {
            let re = match as_regexp(&arg(&args, 0)) {
                Some(r) => (r.compiled()?, *r.global.borrow()),
                None => (
                    Regex::new(&it.to_str(&arg(&args, 0)), "").map_err(|e| Ex::new(5017, e))?,
                    false,
                ),
            };
            let found = re.0.find_all(&c, re.1);
            if found.is_empty() {
                V::Null
            } else if re.1 {
                V::arr(
                    found
                        .iter()
                        .map(|caps| {
                            let (a, b) = caps[0].unwrap();
                            V::str(c[a..b].iter().collect::<String>())
                        })
                        .collect(),
                )
            } else {
                V::arr(
                    found[0]
                        .iter()
                        .map(|x| {
                            x.map(|(p, q)| V::str(c[p..q].iter().collect::<String>()))
                                .unwrap_or(V::Empty)
                        })
                        .collect(),
                )
            }
        }
        "search" => {
            let re = match as_regexp(&arg(&args, 0)) {
                Some(r) => r.compiled()?,
                None => Regex::new(&it.to_str(&arg(&args, 0)), "").map_err(|e| Ex::new(5017, e))?,
            };
            V::Num(
                re.find_at(&c, 0)
                    .and_then(|caps| caps[0])
                    .map(|(a, _)| a as f64)
                    .unwrap_or(-1.0),
            )
        }
        "localeCompare" => {
            let o = it.to_str(&arg(&args, 0));
            V::Num(match (*s).cmp(o.as_str()) {
                std::cmp::Ordering::Less => -1.0,
                std::cmp::Ordering::Equal => 0.0,
                std::cmp::Ordering::Greater => 1.0,
            })
        }
        _ => {
            return Err(Ex::new(
                438,
                format!("les chaînes n'ont pas de méthode « {name} »"),
            ))
        }
    })
}

pub fn array_method(it: &mut Interp, a: Rc<RefCell<Vec<V>>>, name: &str, args: Vec<V>) -> R<V> {
    let len = a.borrow().len();
    Ok(match name {
        "push" => {
            a.borrow_mut().extend(args);
            V::Num(a.borrow().len() as f64)
        }
        "pop" => a.borrow_mut().pop().unwrap_or(V::Empty),
        "shift" => {
            if len == 0 {
                V::Empty
            } else {
                a.borrow_mut().remove(0)
            }
        }
        "unshift" => {
            let mut v = a.borrow_mut();
            for (i, x) in args.into_iter().enumerate() {
                v.insert(i, x);
            }
            V::Num(v.len() as f64)
        }
        "join" | "toString" => {
            let sep = if name == "join" && given(&args, 0) {
                it.to_str(&arg(&args, 0))
            } else {
                ",".into()
            };
            let parts: Vec<String> = a
                .borrow()
                .iter()
                .map(|v| match v {
                    V::Empty | V::Null => String::new(),
                    o => it.to_str(o),
                })
                .collect();
            V::str(parts.join(&sep))
        }
        "slice" => {
            let s = if given(&args, 0) {
                clamp_index(it.to_num(&arg(&args, 0))?, len)
            } else {
                0
            };
            let e = if given(&args, 1) {
                clamp_index(it.to_num(&arg(&args, 1))?, len)
            } else {
                len
            };
            V::arr(a.borrow()[s.min(e)..e.max(s)].to_vec())
        }
        "splice" => {
            let s = clamp_index(it.to_num(&arg(&args, 0))?, len);
            let count = if args.len() >= 2 {
                (it.to_num(&arg(&args, 1))?.max(0.0) as usize).min(len - s)
            } else {
                len - s
            };
            let removed: Vec<V> = a
                .borrow_mut()
                .splice(s..s + count, args.into_iter().skip(2))
                .collect();
            V::arr(removed)
        }
        "concat" => {
            let mut out = a.borrow().clone();
            for x in args {
                match x {
                    V::Arr(o) => out.extend(o.borrow().iter().cloned()),
                    o => out.push(o),
                }
            }
            V::arr(out)
        }
        "reverse" => {
            a.borrow_mut().reverse();
            V::Arr(a)
        }
        "indexOf" => {
            let x = arg(&args, 0);
            let pos = a.borrow().iter().position(|v| {
                it.binary(super::ast::BOp::StrictEq, v.clone(), x.clone())
                    .map(|r| matches!(r, V::Bool(true)))
                    .unwrap_or(false)
            });
            V::Num(pos.map(|p| p as f64).unwrap_or(-1.0))
        }
        "sort" => {
            let mut items = a.borrow().clone();
            let cmp = arg(&args, 0);
            // Tri par insertion : stable, et l'ordre d'appel du comparateur
            // reste simple ; les tableaux des scripts d'installation sont courts.
            for i in 1..items.len() {
                let mut j = i;
                while j > 0 {
                    let before = match &cmp {
                        V::Func(_) => {
                            let r = it.call_value(
                                cmp.clone(),
                                vec![items[j].clone(), items[j - 1].clone()],
                                V::Empty,
                            )?;
                            it.to_num(&r)? < 0.0
                        }
                        _ => it.to_str(&items[j]) < it.to_str(&items[j - 1]),
                    };
                    if !before {
                        break;
                    }
                    items.swap(j, j - 1);
                    j -= 1;
                }
            }
            *a.borrow_mut() = items;
            V::Arr(a)
        }
        _ => {
            return Err(Ex::new(
                438,
                format!("les tableaux n'ont pas de méthode « {name} »"),
            ))
        }
    })
}

pub fn number_method(it: &mut Interp, n: f64, name: &str, args: Vec<V>) -> R<V> {
    Ok(match name {
        "toString" => {
            let radix = if given(&args, 0) {
                it.to_num(&arg(&args, 0))? as u32
            } else {
                10
            };
            if radix == 10 || n != n.trunc() {
                V::str(num_str(n))
            } else {
                let mut v = n.abs() as u64;
                let mut digits = Vec::new();
                loop {
                    digits.push(
                        std::char::from_digit((v % radix as u64) as u32, radix).unwrap_or('0'),
                    );
                    v /= radix as u64;
                    if v == 0 {
                        break;
                    }
                }
                if n < 0.0 {
                    digits.push('-');
                }
                V::str(digits.iter().rev().collect::<String>())
            }
        }
        "toFixed" => {
            let d = if given(&args, 0) {
                it.to_num(&arg(&args, 0))?.max(0.0) as usize
            } else {
                0
            };
            V::str(format!("{n:.d$}"))
        }
        "valueOf" => V::Num(n),
        _ => {
            return Err(Ex::new(
                438,
                format!("les nombres n'ont pas de méthode « {name} »"),
            ))
        }
    })
}
