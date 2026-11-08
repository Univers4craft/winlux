//! Base du paquet vue par une action personnalisée DLL (#833) :
//! `MsiGetActiveDatabase`, `MsiDatabaseOpenView`, `MsiViewExecute`,
//! `MsiViewFetch`… en lecture seule, d'après la documentation publique du
//! SQL de Windows Installer.
//!
//! Requêtes reconnues : `SELECT [DISTINCT] colonnes|* FROM t1[, t2…]
//! [WHERE condition] [ORDER BY colonnes]`, noms entre accents graves ou
//! non, colonnes `table.colonne`, chaînes `'…'`, entiers, paramètres `?`
//! (champs de l'enregistrement passé à MsiViewExecute), `=`, `<>`, `<`,
//! `>`, `<=`, `>=`, `IS [NOT] NULL`, `AND`, `OR`, `NOT`, parenthèses. Les
//! écritures (`MsiViewModify`, `INSERT`, `UPDATE`…) sont refusées : la base
//! d'un paquet en cours d'installation est en lecture seule pour Winlux.
//!
//! Le paquet est relu depuis la propriété `OriginalDatabase`. Une action
//! différée n'a pas de base (comme sous Windows).

use super::db::{Database, Value};
use super::session::{self, Field};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const DB_HANDLE: u32 = 0x4D54;
const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_BAD_QUERY_SYNTAX: u32 = 1615;
const ERROR_FUNCTION_FAILED: u32 = 1627;
const ERROR_INVALID_TABLE: u32 = 1628;
const ERROR_INVALID_HANDLE_STATE: u32 = 1609;

fn database() -> Option<&'static Database> {
    static DB: OnceLock<Option<Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let path = session::property("OriginalDatabase")?;
        let data = std::fs::read(crate::paths::to_linux(&path)).ok()?;
        Database::parse(&data).ok()
    })
    .as_ref()
}

// ---------- analyse ----------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Ident(String),
    Str(String),
    Int(i32),
    Param,
    Sym(&'static str),
}

fn tokens(sql: &str) -> Result<Vec<Tok>, ()> {
    let c: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '`' {
            let end = c[i + 1..].iter().position(|&x| x == '`').ok_or(())? + i + 1;
            out.push(Tok::Ident(c[i + 1..end].iter().collect()));
            i = end + 1;
        } else if ch == '\'' {
            let end = c[i + 1..].iter().position(|&x| x == '\'').ok_or(())? + i + 1;
            out.push(Tok::Str(c[i + 1..end].iter().collect()));
            i = end + 1;
        } else if ch == '?' {
            out.push(Tok::Param);
            i += 1;
        } else if ch.is_ascii_digit()
            || (ch == '-' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            i += 1;
            while i < c.len() && c[i].is_ascii_digit() {
                i += 1;
            }
            let n: String = c[start..i].iter().collect();
            out.push(Tok::Int(n.parse().map_err(|_| ())?));
        } else if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(Tok::Word(c[start..i].iter().collect()));
        } else {
            let two: String = c[i..(i + 2).min(c.len())].iter().collect();
            let sym = match two.as_str() {
                "<>" => Some("<>"),
                "<=" => Some("<="),
                ">=" => Some(">="),
                _ => None,
            };
            if let Some(s) = sym {
                out.push(Tok::Sym(s));
                i += 2;
                continue;
            }
            out.push(Tok::Sym(match ch {
                '=' => "=",
                '<' => "<",
                '>' => ">",
                ',' => ",",
                '*' => "*",
                '(' => "(",
                ')' => ")",
                '.' => ".",
                _ => return Err(()),
            }));
            i += 1;
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq)]
struct ColRef {
    table: Option<String>,
    column: String,
}

#[derive(Clone, Debug, PartialEq)]
enum Operand {
    Col(ColRef),
    Str(String),
    Int(i32),
    Param(usize),
    Null,
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Cmp(Operand, &'static str, Operand),
    IsNull(Operand, bool),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
struct Query {
    distinct: bool,
    columns: Option<Vec<ColRef>>,
    tables: Vec<String>,
    filter: Option<Expr>,
    order: Vec<ColRef>,
    params: usize,
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
    params: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn keyword(&mut self, k: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Word(w)) if w.eq_ignore_ascii_case(k)) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn sym(&mut self, s: &str) -> bool {
        if self.peek()
            == Some(&Tok::Sym(match s {
                "," => ",",
                "*" => "*",
                "(" => "(",
                ")" => ")",
                "." => ".",
                _ => return false,
            }))
        {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn name(&mut self) -> Result<String, ()> {
        match self.t.get(self.i).cloned() {
            Some(Tok::Ident(n)) | Some(Tok::Word(n)) => {
                self.i += 1;
                Ok(n)
            }
            _ => Err(()),
        }
    }
    fn colref(&mut self) -> Result<ColRef, ()> {
        let first = self.name()?;
        if self.sym(".") {
            Ok(ColRef {
                table: Some(first),
                column: self.name()?,
            })
        } else {
            Ok(ColRef {
                table: None,
                column: first,
            })
        }
    }
    fn operand(&mut self) -> Result<Operand, ()> {
        match self.t.get(self.i).cloned() {
            Some(Tok::Str(s)) => {
                self.i += 1;
                Ok(Operand::Str(s))
            }
            Some(Tok::Int(n)) => {
                self.i += 1;
                Ok(Operand::Int(n))
            }
            Some(Tok::Param) => {
                self.i += 1;
                self.params += 1;
                Ok(Operand::Param(self.params))
            }
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("NULL") => {
                self.i += 1;
                Ok(Operand::Null)
            }
            _ => Ok(Operand::Col(self.colref()?)),
        }
    }
    fn primary(&mut self) -> Result<Expr, ()> {
        if self.keyword("NOT") {
            return Ok(Expr::Not(Box::new(self.primary()?)));
        }
        if self.sym("(") {
            let e = self.or()?;
            if !self.sym(")") {
                return Err(());
            }
            return Ok(e);
        }
        let left = self.operand()?;
        if self.keyword("IS") {
            let not = self.keyword("NOT");
            if !self.keyword("NULL") {
                return Err(());
            }
            return Ok(Expr::IsNull(left, !not));
        }
        let op = match self.peek() {
            Some(Tok::Sym(s)) if ["=", "<>", "<", ">", "<=", ">="].contains(s) => *s,
            _ => return Err(()),
        };
        self.i += 1;
        let right = self.operand()?;
        Ok(Expr::Cmp(left, op, right))
    }
    fn and(&mut self) -> Result<Expr, ()> {
        let mut e = self.primary()?;
        while self.keyword("AND") {
            e = Expr::And(Box::new(e), Box::new(self.primary()?));
        }
        Ok(e)
    }
    fn or(&mut self) -> Result<Expr, ()> {
        let mut e = self.and()?;
        while self.keyword("OR") {
            e = Expr::Or(Box::new(e), Box::new(self.and()?));
        }
        Ok(e)
    }
}

fn parse(sql: &str) -> Result<Query, ()> {
    let mut p = Parser {
        t: tokens(sql)?,
        i: 0,
        params: 0,
    };
    if !p.keyword("SELECT") {
        return Err(());
    }
    let distinct = p.keyword("DISTINCT");
    let columns = if p.sym("*") {
        None
    } else {
        let mut cols = vec![p.colref()?];
        while p.sym(",") {
            cols.push(p.colref()?);
        }
        Some(cols)
    };
    if !p.keyword("FROM") {
        return Err(());
    }
    let mut tables = vec![p.name()?];
    while p.sym(",") {
        tables.push(p.name()?);
    }
    let filter = if p.keyword("WHERE") {
        Some(p.or()?)
    } else {
        None
    };
    let mut order = Vec::new();
    if p.keyword("ORDER") {
        if !p.keyword("BY") {
            return Err(());
        }
        order.push(p.colref()?);
        while p.sym(",") {
            order.push(p.colref()?);
        }
    }
    if p.i != p.t.len() {
        return Err(());
    }
    Ok(Query {
        distinct,
        columns,
        tables,
        filter,
        order,
        params: p.params,
    })
}

// ---------- exécution ----------

/// Colonnes visibles d'une jointure : (table, colonne).
fn layout(db: &Database, q: &Query) -> Result<Vec<(String, String)>, u32> {
    let mut all = Vec::new();
    for t in &q.tables {
        let table = db.tables.get(t).ok_or(ERROR_INVALID_TABLE)?;
        all.extend(table.columns.iter().map(|c| (t.clone(), c.clone())));
    }
    Ok(all)
}

fn locate(layout: &[(String, String)], c: &ColRef) -> Result<usize, u32> {
    layout
        .iter()
        .position(|(t, col)| col == &c.column && c.table.as_ref().is_none_or(|x| x == t))
        .ok_or(ERROR_BAD_QUERY_SYNTAX)
}

fn is_null(v: &Value) -> bool {
    v.is_null()
}

fn value(op: &Operand, layout: &[(String, String)], row: &[Value], params: &[Value]) -> Value {
    match op {
        Operand::Col(c) => locate(layout, c)
            .ok()
            .and_then(|i| row.get(i).cloned())
            .unwrap_or(Value::Null),
        Operand::Str(s) => Value::Str(s.clone()),
        Operand::Int(n) => Value::Int(*n),
        Operand::Param(i) => params.get(i - 1).cloned().unwrap_or(Value::Null),
        Operand::Null => Value::Null,
    }
}

fn compare(a: &Value, op: &str, b: &Value) -> bool {
    if is_null(a) || is_null(b) {
        // Windows Installer : NULL = NULL est faux, comme en SQL.
        return false;
    }
    let ord = match (a.int(), b.int(), a, b) {
        (Some(x), Some(y), Value::Int(_), _) | (Some(x), Some(y), _, Value::Int(_)) => x.cmp(&y),
        _ => a.text().cmp(&b.text()),
    };
    match op {
        "=" => ord.is_eq(),
        "<>" => ord.is_ne(),
        "<" => ord.is_lt(),
        ">" => ord.is_gt(),
        "<=" => ord.is_le(),
        _ => ord.is_ge(),
    }
}

fn eval(e: &Expr, layout: &[(String, String)], row: &[Value], params: &[Value]) -> bool {
    match e {
        Expr::Cmp(a, op, b) => compare(
            &value(a, layout, row, params),
            op,
            &value(b, layout, row, params),
        ),
        Expr::IsNull(a, want) => is_null(&value(a, layout, row, params)) == *want,
        Expr::And(a, b) => eval(a, layout, row, params) && eval(b, layout, row, params),
        Expr::Or(a, b) => eval(a, layout, row, params) || eval(b, layout, row, params),
        Expr::Not(a) => !eval(a, layout, row, params),
    }
}

/// Lignes du résultat (au plus 100 000 combinaisons de jointure).
fn run(db: &Database, q: &Query, params: &[Value]) -> Result<Vec<Vec<Value>>, u32> {
    let lay = layout(db, q)?;
    let mut joined: Vec<Vec<Value>> = vec![Vec::new()];
    for t in &q.tables {
        let table = &db.tables[t];
        let mut next = Vec::new();
        for prefix in &joined {
            for r in &table.rows {
                if next.len() >= 100_000 {
                    return Err(ERROR_FUNCTION_FAILED);
                }
                let mut row = prefix.clone();
                row.extend(r.iter().cloned());
                next.push(row);
            }
        }
        joined = next;
    }
    let mut rows: Vec<Vec<Value>> = joined
        .into_iter()
        .filter(|r| q.filter.as_ref().is_none_or(|f| eval(f, &lay, r, params)))
        .collect();
    for c in q.order.iter().rev() {
        let i = locate(&lay, c)?;
        rows.sort_by(|a, b| match (a[i].int(), b[i].int()) {
            (Some(x), Some(y)) => x.cmp(&y),
            _ => a[i].text().cmp(&b[i].text()),
        });
    }
    let picked: Vec<usize> = match &q.columns {
        None => (0..lay.len()).collect(),
        Some(cols) => cols
            .iter()
            .map(|c| locate(&lay, c))
            .collect::<Result<_, _>>()?,
    };
    let mut out: Vec<Vec<Value>> = rows
        .into_iter()
        .map(|r| picked.iter().map(|&i| r[i].clone()).collect())
        .collect();
    if q.distinct {
        let mut seen = Vec::new();
        out.retain(|r| {
            if seen.contains(r) {
                false
            } else {
                seen.push(r.clone());
                true
            }
        });
    }
    Ok(out)
}

// ---------- poignées de vue ----------

struct View {
    query: Query,
    rows: Option<Vec<Vec<Value>>>,
    pos: usize,
}

static VIEWS: Mutex<Option<HashMap<u32, View>>> = Mutex::new(None);

fn views<T>(f: impl FnOnce(&mut HashMap<u32, View>) -> T) -> T {
    let mut g = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

/// MsiCloseHandle d'une vue.
pub fn close(h: u32) -> bool {
    views(|v| v.remove(&h).is_some())
}

fn to_field(v: &Value) -> Field {
    match v {
        Value::Null => Field::Null,
        Value::Int(n) => Field::Int(*n),
        Value::Str(s) if s.is_empty() => Field::Null,
        Value::Str(s) => Field::Str(s.clone()),
    }
}

fn from_field(f: &Field) -> Value {
    match f {
        Field::Null => Value::Null,
        Field::Int(n) => Value::Int(*n),
        Field::Str(s) => Value::Str(s.clone()),
    }
}

pub extern "win64" fn msi_get_active_database(h: usize) -> u32 {
    if h as u32 != session::SESSION_HANDLE || session::scheduled() {
        return 0;
    }
    if database().is_some() {
        DB_HANDLE
    } else {
        0
    }
}

fn open_view(db: usize, query: usize, out: usize, wide: bool) -> u32 {
    if db as u32 != DB_HANDLE || database().is_none() {
        return ERROR_INVALID_HANDLE;
    }
    if query == 0 || out == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let sql = unsafe {
        if wide {
            crate::dlls::wstr(query as *const u16)
        } else {
            crate::dlls::cstr(query as *const u8)
        }
    };
    let Ok(q) = parse(&sql) else {
        eprintln!("msi.dll : requête non prise en charge : {sql}");
        return ERROR_BAD_QUERY_SYNTAX;
    };
    if let Err(e) = layout(database().unwrap(), &q) {
        return e;
    }
    let h = session::new_handle();
    views(|v| {
        v.insert(
            h,
            View {
                query: q,
                rows: None,
                pos: 0,
            },
        )
    });
    unsafe { *(out as *mut u32) = h };
    ERROR_SUCCESS
}

pub extern "win64" fn msi_database_open_view_w(db: usize, query: usize, out: usize) -> u32 {
    open_view(db, query, out, true)
}
pub extern "win64" fn msi_database_open_view_a(db: usize, query: usize, out: usize) -> u32 {
    open_view(db, query, out, false)
}

pub extern "win64" fn msi_view_execute(view: usize, rec: usize) -> u32 {
    let params: Vec<Value> = session::record_fields(rec as u32)
        .map(|f| f.iter().skip(1).map(from_field).collect())
        .unwrap_or_default();
    let Some(db) = database() else {
        return ERROR_INVALID_HANDLE;
    };
    views(|v| {
        let Some(view) = v.get_mut(&(view as u32)) else {
            return ERROR_INVALID_HANDLE;
        };
        if params.len() < view.query.params {
            return ERROR_INVALID_PARAMETER;
        }
        match run(db, &view.query, &params) {
            Ok(rows) => {
                view.rows = Some(rows);
                view.pos = 0;
                ERROR_SUCCESS
            }
            Err(e) => e,
        }
    })
}

pub extern "win64" fn msi_view_fetch(view: usize, out: usize) -> u32 {
    if out == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let row = views(|v| {
        let view = v.get_mut(&(view as u32)).ok_or(ERROR_INVALID_HANDLE)?;
        let rows = view.rows.as_ref().ok_or(ERROR_INVALID_HANDLE_STATE)?;
        let row = rows.get(view.pos).cloned().ok_or(ERROR_NO_MORE_ITEMS)?;
        view.pos += 1;
        Ok::<_, u32>(row)
    });
    match row {
        Ok(row) => {
            let mut fields = vec![Field::Null];
            fields.extend(row.iter().map(to_field));
            unsafe { *(out as *mut u32) = session::new_record(fields) };
            ERROR_SUCCESS
        }
        Err(e) => {
            unsafe { *(out as *mut u32) = 0 };
            e
        }
    }
}

pub extern "win64" fn msi_view_close(view: usize) -> u32 {
    views(|v| match v.get_mut(&(view as u32)) {
        Some(view) => {
            view.rows = None;
            view.pos = 0;
            ERROR_SUCCESS
        }
        None => ERROR_INVALID_HANDLE,
    })
}

/// Écritures refusées : la base du paquet reste en lecture seule.
pub extern "win64" fn msi_view_modify(_view: usize, _mode: i32, _rec: usize) -> u32 {
    ERROR_FUNCTION_FAILED
}

/// MsiViewGetColumnInfo : noms (0) ou types (1) des colonnes du résultat.
pub extern "win64" fn msi_view_get_column_info(view: usize, kind: i32, out: usize) -> u32 {
    if out == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let Some(db) = database() else {
        return ERROR_INVALID_HANDLE;
    };
    let info = views(|v| {
        let view = v.get(&(view as u32))?;
        let lay = layout(db, &view.query).ok()?;
        let cols: Vec<(String, String)> = match &view.query.columns {
            None => lay.clone(),
            Some(c) => c
                .iter()
                .filter_map(|c| locate(&lay, c).ok().map(|i| lay[i].clone()))
                .collect(),
        };
        Some(
            cols.iter()
                .map(|(t, c)| {
                    if kind == 0 {
                        return c.clone();
                    }
                    // Type déduit des valeurs : entier ou chaîne.
                    let table = &db.tables[t];
                    let i = table.col(c).unwrap_or(0);
                    let int = table
                        .rows
                        .iter()
                        .find(|r| !r[i].is_null())
                        .is_some_and(|r| matches!(r[i], Value::Int(_)));
                    if int { "i4" } else { "s255" }.to_string()
                })
                .collect::<Vec<_>>(),
        )
    });
    let Some(info) = info else {
        return ERROR_INVALID_HANDLE;
    };
    let mut fields = vec![Field::Null];
    fields.extend(info.into_iter().map(Field::Str));
    unsafe { *(out as *mut u32) = session::new_record(fields) };
    ERROR_SUCCESS
}

fn table_persistent(db: usize, table: usize, wide: bool) -> i32 {
    if db as u32 != DB_HANDLE {
        return 3; // MSICONDITION_ERROR
    }
    let name = unsafe {
        if wide {
            crate::dlls::wstr(table as *const u16)
        } else {
            crate::dlls::cstr(table as *const u8)
        }
    };
    match database() {
        Some(d) if d.tables.contains_key(&name) => 1, // MSICONDITION_TRUE
        Some(_) => 2,                                 // MSICONDITION_NONE
        None => 3,
    }
}

pub extern "win64" fn msi_database_is_table_persistent_w(db: usize, table: usize) -> i32 {
    table_persistent(db, table, true)
}
pub extern "win64" fn msi_database_is_table_persistent_a(db: usize, table: usize) -> i32 {
    table_persistent(db, table, false)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "MsiGetActiveDatabase" => fn msi_get_active_database,
        "MsiDatabaseOpenViewW" => fn msi_database_open_view_w,
        "MsiDatabaseOpenViewA" => fn msi_database_open_view_a,
        "MsiViewExecute" => fn msi_view_execute,
        "MsiViewFetch" => fn msi_view_fetch,
        "MsiViewClose" => fn msi_view_close,
        "MsiViewModify" => fn msi_view_modify,
        "MsiViewGetColumnInfo" => fn msi_view_get_column_info,
        "MsiDatabaseIsTablePersistentW" => fn msi_database_is_table_persistent_w,
        "MsiDatabaseIsTablePersistentA" => fn msi_database_is_table_persistent_a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msi::db::Table;

    fn sample() -> Database {
        let mut db = Database::default();
        db.tables.insert(
            "File".into(),
            Table {
                columns: vec![
                    "File".into(),
                    "Component_".into(),
                    "FileName".into(),
                    "Sequence".into(),
                ],
                rows: vec![
                    vec![
                        Value::Str("a".into()),
                        Value::Str("C1".into()),
                        Value::Str("a.exe".into()),
                        Value::Int(2),
                    ],
                    vec![
                        Value::Str("b".into()),
                        Value::Str("C2".into()),
                        Value::Str("b.dll".into()),
                        Value::Int(1),
                    ],
                    vec![
                        Value::Str("c".into()),
                        Value::Str("C1".into()),
                        Value::Null,
                        Value::Int(3),
                    ],
                ],
            },
        );
        db.tables.insert(
            "Component".into(),
            Table {
                columns: vec!["Component".into(), "Directory_".into()],
                rows: vec![
                    vec![Value::Str("C1".into()), Value::Str("INSTALLDIR".into())],
                    vec![Value::Str("C2".into()), Value::Str("BIN".into())],
                ],
            },
        );
        db
    }

    #[test]
    fn select_where_param_order() {
        let db = sample();
        let q = parse(
            "SELECT `File`, `FileName` FROM `File` WHERE `Component_` = ? ORDER BY `Sequence`",
        )
        .unwrap();
        let rows = run(&db, &q, &[Value::Str("C1".into())]).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], Value::Str("a".into()));
        let q = parse("SELECT * FROM File WHERE FileName IS NULL").unwrap();
        assert_eq!(run(&db, &q, &[]).unwrap().len(), 1);
        let q = parse("SELECT `File` FROM `File` WHERE `Sequence` >= 2 AND NOT (`File` = 'c')")
            .unwrap();
        assert_eq!(
            run(&db, &q, &[]).unwrap(),
            vec![vec![Value::Str("a".into())]]
        );
    }

    #[test]
    fn join_and_distinct() {
        let db = sample();
        let q = parse(
            "SELECT DISTINCT `Component`.`Directory_` FROM `File`, `Component` \
             WHERE `File`.`Component_` = `Component`.`Component` ORDER BY `Component`.`Directory_`",
        )
        .unwrap();
        let rows = run(&db, &q, &[]).unwrap();
        assert_eq!(
            rows,
            vec![
                vec![Value::Str("BIN".into())],
                vec![Value::Str("INSTALLDIR".into())]
            ]
        );
    }

    #[test]
    fn refuse_writes_and_unknown_tables() {
        assert!(parse("INSERT INTO `File` (`File`) VALUES ('x')").is_err());
        let db = sample();
        let q = parse("SELECT * FROM `Absente`").unwrap();
        assert_eq!(run(&db, &q, &[]), Err(ERROR_INVALID_TABLE));
    }
}
