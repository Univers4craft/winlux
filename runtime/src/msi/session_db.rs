//! Base du paquet vue par une action personnalisée DLL (#833) :
//! `MsiGetActiveDatabase`, `MsiDatabaseOpenView`, `MsiViewExecute`,
//! `MsiViewFetch`, `MsiViewModify`… d'après la documentation publique du
//! SQL de Windows Installer.
//!
//! Requêtes reconnues : `SELECT [DISTINCT] colonnes|* FROM t1[, t2…]
//! [WHERE condition] [ORDER BY colonnes]`, noms entre accents graves ou
//! non, colonnes `table.colonne`, chaînes `'…'`, entiers, paramètres `?`
//! (champs de l'enregistrement passé à MsiViewExecute), `=`, `<>`, `<`,
//! `>`, `<=`, `>=`, `IS [NOT] NULL`, `AND`, `OR`, `NOT`, parenthèses.
//!
//! Écritures : `MsiViewModify` (insertion, insertion temporaire, mise à
//! jour, affectation, remplacement, fusion, suppression) sur une vue d'une
//! seule table. Comme dans une session d'installation de Windows, où la
//! base du paquet n'accepte que des changements temporaires, elles vivent
//! en mémoire le temps de l'action et ne sont jamais écrites dans le .msi.
//! Les colonnes de flux binaire (table `Binary`, `Icon`…) se lisent par
//! `MsiRecordReadStream`.
//!
//! Le paquet est relu depuis la propriété `OriginalDatabase`. Une action
//! différée n'a pas de base (comme sous Windows).

use super::db::{Database, Value};
use super::session::{self, Field};
use std::collections::HashMap;
use std::sync::Mutex;

pub const DB_HANDLE: u32 = 0x4D54;
const ERROR_SUCCESS: u32 = 0;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_BAD_QUERY_SYNTAX: u32 = 1615;
const ERROR_FUNCTION_FAILED: u32 = 1627;
const ERROR_INVALID_TABLE: u32 = 1628;
const ERROR_INVALID_HANDLE_STATE: u32 = 1609;
const ERROR_INVALID_DATA: u32 = 13;

/// Base du paquet, chargée à la première demande (`None` : illisible).
static DB: Mutex<Option<Option<Database>>> = Mutex::new(None);

fn with_db<T>(f: impl FnOnce(&mut Database) -> T) -> Option<T> {
    let mut g = DB.lock().unwrap_or_else(|e| e.into_inner());
    let db = g.get_or_insert_with(|| {
        let path = session::property("OriginalDatabase")?;
        let data = std::fs::read(crate::paths::to_linux(&path)).ok()?;
        Database::parse(&data).ok()
    });
    db.as_mut().map(f)
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
    if a.is_null() || b.is_null() {
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
        Expr::IsNull(a, want) => value(a, layout, row, params).is_null() == *want,
        Expr::And(a, b) => eval(a, layout, row, params) && eval(b, layout, row, params),
        Expr::Or(a, b) => eval(a, layout, row, params) || eval(b, layout, row, params),
        Expr::Not(a) => !eval(a, layout, row, params),
    }
}

/// Ligne du résultat : valeurs choisies et, pour chaque table de la
/// jointure, l'indice de la ligne d'origine.
#[derive(Clone, Debug, PartialEq)]
struct Hit {
    values: Vec<Value>,
    origin: Vec<usize>,
}

/// Colonnes choisies : (indice de table dans la jointure, indice de colonne).
fn picked(db: &Database, q: &Query) -> Result<Vec<(usize, usize)>, u32> {
    let lay = layout(db, q)?;
    let mut owner = Vec::new();
    for (k, t) in q.tables.iter().enumerate() {
        owner.extend((0..db.tables[t].columns.len()).map(|c| (k, c)));
    }
    Ok(match &q.columns {
        None => owner,
        Some(cols) => cols
            .iter()
            .map(|c| locate(&lay, c).map(|i| owner[i]))
            .collect::<Result<_, _>>()?,
    })
}

/// Lignes du résultat (au plus 100 000 combinaisons de jointure).
fn run_hits(db: &Database, q: &Query, params: &[Value]) -> Result<Vec<Hit>, u32> {
    let lay = layout(db, q)?;
    let mut joined: Vec<(Vec<Value>, Vec<usize>)> = vec![(Vec::new(), Vec::new())];
    for t in &q.tables {
        let table = &db.tables[t];
        let mut next = Vec::new();
        for (prefix, origin) in &joined {
            for (i, r) in table.rows.iter().enumerate() {
                if next.len() >= 100_000 {
                    return Err(ERROR_FUNCTION_FAILED);
                }
                let mut row = prefix.clone();
                row.extend(r.iter().cloned());
                let mut o = origin.clone();
                o.push(i);
                next.push((row, o));
            }
        }
        joined = next;
    }
    let mut rows: Vec<(Vec<Value>, Vec<usize>)> = joined
        .into_iter()
        .filter(|(r, _)| q.filter.as_ref().is_none_or(|f| eval(f, &lay, r, params)))
        .collect();
    for c in q.order.iter().rev() {
        let i = locate(&lay, c)?;
        rows.sort_by(|(a, _), (b, _)| match (a[i].int(), b[i].int()) {
            (Some(x), Some(y)) => x.cmp(&y),
            _ => a[i].text().cmp(&b[i].text()),
        });
    }
    let cols = picked(db, q)?;
    let starts: Vec<usize> = q
        .tables
        .iter()
        .scan(0, |acc, t| {
            let s = *acc;
            *acc += db.tables[t].columns.len();
            Some(s)
        })
        .collect();
    let mut out: Vec<Hit> = rows
        .into_iter()
        .map(|(r, origin)| Hit {
            values: cols
                .iter()
                .map(|&(k, c)| r[starts[k] + c].clone())
                .collect(),
            origin,
        })
        .collect();
    if q.distinct {
        let mut seen: Vec<Vec<Value>> = Vec::new();
        out.retain(|h| {
            if seen.contains(&h.values) {
                false
            } else {
                seen.push(h.values.clone());
                true
            }
        });
    }
    Ok(out)
}

#[cfg(test)]
fn run(db: &Database, q: &Query, params: &[Value]) -> Result<Vec<Vec<Value>>, u32> {
    Ok(run_hits(db, q, params)?
        .into_iter()
        .map(|h| h.values)
        .collect())
}

// ---------- poignées de vue ----------

struct View {
    query: Query,
    rows: Option<Vec<Hit>>,
    pos: usize,
}

#[derive(Default)]
struct Views {
    views: HashMap<u32, View>,
    /// Enregistrement rendu par MsiViewFetch → (vue, ligne d'origine).
    fetched: HashMap<u32, (u32, Vec<usize>)>,
}

static VIEWS: Mutex<Option<Views>> = Mutex::new(None);

fn views<T>(f: impl FnOnce(&mut Views) -> T) -> T {
    let mut g = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Views::default))
}

/// MsiCloseHandle d'une vue ou d'un enregistrement lu.
pub fn close(h: u32) -> bool {
    views(|v| {
        v.fetched.remove(&h);
        v.views.remove(&h).is_some()
    })
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
        Field::Null | Field::Stream(..) => Value::Null,
        Field::Int(n) => Value::Int(*n),
        Field::Str(s) => Value::Str(s.clone()),
    }
}

/// Flux d'une ligne : `Table.clé`, la clé étant la première colonne.
fn stream_of(db: &Database, table: &str, row: usize) -> Option<Vec<u8>> {
    let key = db.tables.get(table)?.rows.get(row)?.first()?.text();
    db.streams.get(&format!("{table}.{key}")).cloned()
}

pub extern "win64" fn msi_get_active_database(h: usize) -> u32 {
    if h as u32 != session::SESSION_HANDLE || session::scheduled() {
        return 0;
    }
    if with_db(|_| ()).is_some() {
        DB_HANDLE
    } else {
        0
    }
}

fn open_view(db: usize, query: usize, out: usize, wide: bool) -> u32 {
    if db as u32 != DB_HANDLE {
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
    match with_db(|d| layout(d, &q).map(|_| ())) {
        None => return ERROR_INVALID_HANDLE,
        Some(Err(e)) => return e,
        Some(Ok(())) => {}
    }
    let h = session::new_handle();
    views(|v| {
        v.views.insert(
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
    let Some(query) = views(|v| v.views.get(&(view as u32)).map(|w| w.query.clone())) else {
        return ERROR_INVALID_HANDLE;
    };
    if params.len() < query.params {
        return ERROR_INVALID_PARAMETER;
    }
    let Some(result) = with_db(|d| run_hits(d, &query, &params)) else {
        return ERROR_INVALID_HANDLE;
    };
    match result {
        Ok(rows) => views(|v| match v.views.get_mut(&(view as u32)) {
            Some(w) => {
                w.rows = Some(rows);
                w.pos = 0;
                ERROR_SUCCESS
            }
            None => ERROR_INVALID_HANDLE,
        }),
        Err(e) => e,
    }
}

pub extern "win64" fn msi_view_fetch(view: usize, out: usize) -> u32 {
    if out == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let row = views(|v| {
        let w = v
            .views
            .get_mut(&(view as u32))
            .ok_or(ERROR_INVALID_HANDLE)?;
        let rows = w.rows.as_ref().ok_or(ERROR_INVALID_HANDLE_STATE)?;
        let hit = rows.get(w.pos).cloned().ok_or(ERROR_NO_MORE_ITEMS)?;
        w.pos += 1;
        Ok::<_, u32>((hit, w.query.clone()))
    });
    let (hit, query) = match row {
        Ok(r) => r,
        Err(e) => {
            unsafe { *(out as *mut u32) = 0 };
            return e;
        }
    };
    let fields = with_db(|d| {
        let cols = picked(d, &query).unwrap_or_default();
        let mut fields = vec![Field::Null];
        for (i, v) in hit.values.iter().enumerate() {
            let stream = cols.get(i).and_then(|&(k, c)| {
                let t = &query.tables[k];
                let binary = d.tables[t].binary.get(c).copied().unwrap_or(false);
                (binary && !v.is_null())
                    .then(|| stream_of(d, t, hit.origin[k]))
                    .flatten()
            });
            fields.push(match stream {
                Some(data) => Field::Stream(data, 0),
                None => to_field(v),
            });
        }
        fields
    })
    .unwrap_or_default();
    let h = session::new_record(fields);
    views(|v| v.fetched.insert(h, (view as u32, hit.origin)));
    unsafe { *(out as *mut u32) = h };
    ERROR_SUCCESS
}

pub extern "win64" fn msi_view_close(view: usize) -> u32 {
    views(|v| match v.views.get_mut(&(view as u32)) {
        Some(w) => {
            w.rows = None;
            w.pos = 0;
            ERROR_SUCCESS
        }
        None => ERROR_INVALID_HANDLE,
    })
}

/// Applique un MsiViewModify à la base en mémoire. `origin` : ligne d'où
/// vient l'enregistrement (lu par MsiViewFetch), sinon recherche par la
/// première colonne (la clé).
fn modify(
    db: &mut Database,
    q: &Query,
    mode: i32,
    values: &[Value],
    origin: Option<usize>,
) -> Result<(), u32> {
    if q.tables.len() != 1 {
        return Err(ERROR_FUNCTION_FAILED);
    }
    let cols = picked(db, q)?;
    let table = db.tables.get_mut(&q.tables[0]).ok_or(ERROR_INVALID_TABLE)?;
    let width = table.columns.len();
    let key = cols
        .iter()
        .position(|&(_, c)| c == 0)
        .and_then(|i| values.get(i).cloned());
    let found = origin.or_else(|| {
        let k = key.as_ref().filter(|k| !k.is_null())?;
        table.rows.iter().position(|r| r[0].text() == k.text())
    });
    let apply = |row: &mut Vec<Value>| {
        for (i, &(_, c)) in cols.iter().enumerate() {
            if let Some(v) = values.get(i) {
                row[c] = v.clone();
            }
        }
    };
    match mode {
        // MSIMODIFY_SEEK, REFRESH, VALIDATE*
        -1 | 0 | 8..=10 => Ok(()),
        // INSERT, INSERT_TEMPORARY : refusé si la clé existe déjà.
        1 | 7 => {
            if key.as_ref().is_none_or(|k| k.is_null()) {
                return Err(ERROR_INVALID_DATA);
            }
            if found.is_some() && origin.is_none() {
                return Err(ERROR_FUNCTION_FAILED);
            }
            let mut row = vec![Value::Null; width];
            apply(&mut row);
            table.rows.push(row);
            Ok(())
        }
        // UPDATE : la ligne doit avoir été lue.
        2 => {
            let i = origin.ok_or(ERROR_FUNCTION_FAILED)?;
            apply(table.rows.get_mut(i).ok_or(ERROR_FUNCTION_FAILED)?);
            Ok(())
        }
        // ASSIGN, REPLACE, MERGE : mise à jour ou insertion.
        3..=5 => {
            match found {
                Some(i) => {
                    let row = table.rows.get_mut(i).ok_or(ERROR_FUNCTION_FAILED)?;
                    if mode == 5 {
                        let same = cols.iter().enumerate().all(|(n, &(_, c))| {
                            values.get(n).is_none_or(|v| v.text() == row[c].text())
                        });
                        if !same {
                            return Err(ERROR_FUNCTION_FAILED);
                        }
                    }
                    apply(row);
                }
                None => {
                    let mut row = vec![Value::Null; width];
                    apply(&mut row);
                    table.rows.push(row);
                }
            }
            Ok(())
        }
        // DELETE
        6 => {
            let i = found.ok_or(ERROR_FUNCTION_FAILED)?;
            if i < table.rows.len() {
                table.rows.remove(i);
            }
            Ok(())
        }
        _ => Err(ERROR_INVALID_PARAMETER),
    }
}

/// MsiViewModify : changements en mémoire seulement (voir l'en-tête).
pub extern "win64" fn msi_view_modify(view: usize, mode: i32, rec: usize) -> u32 {
    let Some(fields) = session::record_fields(rec as u32) else {
        return ERROR_INVALID_HANDLE;
    };
    let values: Vec<Value> = fields.iter().skip(1).map(from_field).collect();
    let info = views(|v| {
        let q = v.views.get(&(view as u32))?.query.clone();
        let origin = v
            .fetched
            .get(&(rec as u32))
            .filter(|(from, _)| *from == view as u32)
            .and_then(|(_, o)| o.first().copied());
        Some((q, origin))
    });
    let Some((query, origin)) = info else {
        return ERROR_INVALID_HANDLE;
    };
    let result = with_db(|d| modify(d, &query, mode, &values, origin));
    match result {
        Some(Ok(())) => {
            if mode == 6 {
                // Les lignes suivantes ont changé d'indice.
                views(|v| v.fetched.retain(|_, (from, _)| *from != view as u32));
            }
            ERROR_SUCCESS
        }
        Some(Err(e)) => e,
        None => ERROR_INVALID_HANDLE,
    }
}

/// MsiViewGetColumnInfo : noms (0) ou types (1) des colonnes du résultat.
pub extern "win64" fn msi_view_get_column_info(view: usize, kind: i32, out: usize) -> u32 {
    if out == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    let Some(query) = views(|v| v.views.get(&(view as u32)).map(|w| w.query.clone())) else {
        return ERROR_INVALID_HANDLE;
    };
    let info = with_db(|d| {
        let cols = picked(d, &query).ok()?;
        Some(
            cols.iter()
                .map(|&(k, c)| {
                    let table = &d.tables[&query.tables[k]];
                    if kind == 0 {
                        return table.columns[c].clone();
                    }
                    if table.binary.get(c).copied().unwrap_or(false) {
                        return "v0".to_string();
                    }
                    // Type déduit des valeurs : entier ou chaîne.
                    let int = table
                        .rows
                        .iter()
                        .find(|r| !r[c].is_null())
                        .is_some_and(|r| matches!(r[c], Value::Int(_)));
                    if int { "i4" } else { "s255" }.to_string()
                })
                .collect::<Vec<_>>(),
        )
    })
    .flatten();
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
    match with_db(|d| d.tables.contains_key(&name)) {
        Some(true) => 1,  // MSICONDITION_TRUE
        Some(false) => 2, // MSICONDITION_NONE
        None => 3,
    }
}

pub extern "win64" fn msi_database_is_table_persistent_w(db: usize, table: usize) -> i32 {
    table_persistent(db, table, true)
}
pub extern "win64" fn msi_database_is_table_persistent_a(db: usize, table: usize) -> i32 {
    table_persistent(db, table, false)
}

/// Colonnes de la clé primaire d'une table, dans l'ordre des colonnes.
fn key_columns(d: &Database, table: &str) -> Option<Vec<String>> {
    let t = d.tables.get(table)?;
    Some(
        t.columns
            .iter()
            .zip(&t.keys)
            .filter(|(_, &k)| k)
            .map(|(c, _)| c.clone())
            .collect(),
    )
}

fn primary_keys(db: usize, table: usize, out: usize, wide: bool) -> u32 {
    if out == 0 || table == 0 {
        return ERROR_INVALID_PARAMETER;
    }
    unsafe { *(out as *mut u32) = 0 };
    if db as u32 != DB_HANDLE {
        return ERROR_INVALID_HANDLE;
    }
    let name = unsafe {
        if wide {
            crate::dlls::wstr(table as *const u16)
        } else {
            crate::dlls::cstr(table as *const u8)
        }
    };
    let keys = with_db(|d| key_columns(d, &name));
    match keys {
        None => ERROR_INVALID_HANDLE,
        Some(None) => ERROR_INVALID_TABLE,
        Some(Some(keys)) => {
            // Champ 0 : nom de la table ; champs 1..n : colonnes de la clé.
            let mut fields = vec![Field::Str(name)];
            fields.extend(keys.into_iter().map(Field::Str));
            unsafe { *(out as *mut u32) = session::new_record(fields) };
            ERROR_SUCCESS
        }
    }
}

/// MsiDatabaseGetPrimaryKeys : enregistrement des colonnes de la clé primaire.
pub extern "win64" fn msi_database_get_primary_keys_w(db: usize, table: usize, out: usize) -> u32 {
    primary_keys(db, table, out, true)
}
pub extern "win64" fn msi_database_get_primary_keys_a(db: usize, table: usize, out: usize) -> u32 {
    primary_keys(db, table, out, false)
}

const MSIDBERROR_INVALIDARG: i32 = -3;
const MSIDBERROR_NOERROR: i32 = 0;

/// MsiViewGetError : MsiViewModify ne valide pas les lignes (pas de
/// MSIMODIFY_VALIDATE), il n'y a donc jamais d'erreur de colonne à
/// signaler : MSIDBERROR_NOERROR et un nom de colonne vide.
fn view_error(view: usize, buf: usize, size: usize, wide: bool) -> i32 {
    if size == 0 || !views(|v| v.views.contains_key(&(view as u32))) {
        return MSIDBERROR_INVALIDARG;
    }
    unsafe {
        if buf != 0 && *(size as *const u32) > 0 {
            if wide {
                *(buf as *mut u16) = 0;
            } else {
                *(buf as *mut u8) = 0;
            }
        }
        *(size as *mut u32) = 0;
    }
    MSIDBERROR_NOERROR
}
pub extern "win64" fn msi_view_get_error_w(view: usize, buf: usize, size: usize) -> i32 {
    view_error(view, buf, size, true)
}
pub extern "win64" fn msi_view_get_error_a(view: usize, buf: usize, size: usize) -> i32 {
    view_error(view, buf, size, false)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "MsiDatabaseGetPrimaryKeysW" => fn msi_database_get_primary_keys_w,
        "MsiDatabaseGetPrimaryKeysA" => fn msi_database_get_primary_keys_a,
        "MsiViewGetErrorW" => fn msi_view_get_error_w,
        "MsiViewGetErrorA" => fn msi_view_get_error_a,
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

    fn s(v: &str) -> Value {
        Value::Str(v.into())
    }

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
                    vec![s("a"), s("C1"), s("a.exe"), Value::Int(2)],
                    vec![s("b"), s("C2"), s("b.dll"), Value::Int(1)],
                    vec![s("c"), s("C1"), Value::Null, Value::Int(3)],
                ],
                ..Default::default()
            },
        );
        db.tables.insert(
            "Component".into(),
            Table {
                columns: vec!["Component".into(), "Directory_".into()],
                rows: vec![vec![s("C1"), s("INSTALLDIR")], vec![s("C2"), s("BIN")]],
                ..Default::default()
            },
        );
        db.tables.insert(
            "Binary".into(),
            Table {
                columns: vec!["Name".into(), "Data".into()],
                rows: vec![vec![s("Outil"), Value::Int(1)]],
                binary: vec![false, true],
                ..Default::default()
            },
        );
        db.streams
            .insert("Binary.Outil".into(), b"MZ\x90\0".to_vec());
        db
    }

    #[test]
    fn select_where_param_order() {
        let db = sample();
        let q = parse(
            "SELECT `File`, `FileName` FROM `File` WHERE `Component_` = ? ORDER BY `Sequence`",
        )
        .unwrap();
        let rows = run(&db, &q, &[s("C1")]).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], s("a"));
        let q = parse("SELECT * FROM File WHERE FileName IS NULL").unwrap();
        assert_eq!(run(&db, &q, &[]).unwrap().len(), 1);
        let q = parse("SELECT `File` FROM `File` WHERE `Sequence` >= 2 AND NOT (`File` = 'c')")
            .unwrap();
        assert_eq!(run(&db, &q, &[]).unwrap(), vec![vec![s("a")]]);
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
        assert_eq!(rows, vec![vec![s("BIN")], vec![s("INSTALLDIR")]]);
    }

    #[test]
    fn refuse_bad_sql_and_unknown_tables() {
        assert!(parse("INSERT INTO `File` (`File`) VALUES ('x')").is_err());
        let db = sample();
        let q = parse("SELECT * FROM `Absente`").unwrap();
        assert_eq!(run(&db, &q, &[]), Err(ERROR_INVALID_TABLE));
    }

    #[test]
    fn modify_in_memory() {
        let mut db = sample();
        let q = parse("SELECT `File`, `FileName` FROM `File`").unwrap();
        // Insertion temporaire, puis doublon refusé.
        assert_eq!(modify(&mut db, &q, 7, &[s("d"), s("d.txt")], None), Ok(()));
        assert_eq!(
            modify(&mut db, &q, 1, &[s("d"), s("autre")], None),
            Err(ERROR_FUNCTION_FAILED)
        );
        let all = parse("SELECT `Component_` FROM `File` WHERE `File` = 'd'").unwrap();
        assert_eq!(run(&db, &all, &[]).unwrap(), vec![vec![Value::Null]]);
        // Mise à jour d'une ligne lue, affectation par la clé, suppression.
        assert_eq!(
            modify(&mut db, &q, 2, &[s("a"), s("A.EXE")], Some(0)),
            Ok(())
        );
        assert_eq!(modify(&mut db, &q, 3, &[s("b"), s("B.DLL")], None), Ok(()));
        assert_eq!(modify(&mut db, &q, 6, &[s("c"), Value::Null], None), Ok(()));
        let names = parse("SELECT `FileName` FROM `File` ORDER BY `File`").unwrap();
        assert_eq!(
            run(&db, &names, &[]).unwrap(),
            vec![vec![s("A.EXE")], vec![s("B.DLL")], vec![s("d.txt")]]
        );
        // Jointure : pas d'écriture.
        let j = parse("SELECT `File`.`File` FROM `File`, `Component`").unwrap();
        assert_eq!(
            modify(&mut db, &j, 7, &[s("x")], None),
            Err(ERROR_FUNCTION_FAILED)
        );
    }

    #[test]
    fn binary_column_reads_its_stream() {
        let db = sample();
        let q = parse("SELECT `Data` FROM `Binary` WHERE `Name` = 'Outil'").unwrap();
        let hits = run_hits(&db, &q, &[]).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            stream_of(&db, "Binary", hits[0].origin[0]).unwrap(),
            b"MZ\x90\0"
        );
    }

    #[test]
    fn cles_primaires_lues_dans_columns() {
        // Paquet réel produit par msitools : clés déclarées dans les tables
        // standard (#1607).
        let db = Database::parse(include_bytes!("testdata/sample.msi")).unwrap();
        assert_eq!(key_columns(&db, "File").unwrap(), vec!["File".to_string()]);
        assert_eq!(
            key_columns(&db, "Component").unwrap(),
            vec!["Component".to_string()]
        );
        assert_eq!(
            key_columns(&db, "FeatureComponents").unwrap(),
            vec!["Feature_".to_string(), "Component_".to_string()]
        );
        assert!(key_columns(&db, "Absente").is_none());
    }
}
