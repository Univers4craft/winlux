//! Fichiers .ini : famille GetPrivateProfile* / WritePrivateProfile* et
//! leurs variantes win.ini (GetProfile*, WriteProfile*), en A et en W.
//!
//! Écrit d'après la documentation publique de Microsoft (salle blanche,
//! voir AGENTS.md). Sections et clés sont insensibles à la casse ; un nom
//! de fichier sans dossier désigne un fichier de C:\Windows, comme sous
//! Windows. L'encodage du fichier est conservé : UTF-16 avec marque
//! d'ordre, UTF-8, ou page ANSI.

use crate::dlls::{cstr, decode_ansi, encode_ansi, wstr, Exports};
use std::path::PathBuf;
use std::sync::Mutex;

/// Un seul accès aux fichiers .ini à la fois (lecture puis réécriture).
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, PartialEq, Debug)]
enum Encoding {
    Utf16,
    Utf8,
    Ansi,
}

struct Ini {
    lines: Vec<String>,
    encoding: Encoding,
}

fn ini_path(name: &str) -> PathBuf {
    let name = name.trim();
    let full = if name.contains(['\\', '/']) {
        name.to_string()
    } else {
        format!("C:\\Windows\\{name}")
    };
    crate::paths::to_linux(&full)
}

fn parse(bytes: &[u8]) -> Ini {
    let (text, encoding) = if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        (String::from_utf16_lossy(&units), Encoding::Utf16)
    } else if let Ok(s) = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
    {
        let enc = if s.is_ascii() {
            Encoding::Ansi
        } else {
            Encoding::Utf8
        };
        (s.to_string(), enc)
    } else {
        (decode_ansi(bytes), Encoding::Ansi)
    };
    Ini {
        lines: text.lines().map(str::to_string).collect(),
        encoding,
    }
}

fn load(file: &str) -> Ini {
    match std::fs::read(ini_path(file)) {
        Ok(b) => parse(&b),
        Err(_) => Ini {
            lines: Vec::new(),
            encoding: Encoding::Ansi,
        },
    }
}

fn save(file: &str, ini: &Ini) -> bool {
    let mut text = ini.lines.join("\r\n");
    if !text.is_empty() {
        text.push_str("\r\n");
    }
    let bytes = match ini.encoding {
        Encoding::Utf16 => {
            let mut b = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                b.extend_from_slice(&u.to_le_bytes());
            }
            b
        }
        Encoding::Utf8 => text.into_bytes(),
        Encoding::Ansi => encode_ansi(&text),
    };
    std::fs::write(ini_path(file), bytes).is_ok()
}

fn section_name(line: &str) -> Option<&str> {
    let t = line.trim();
    let inner = t.strip_prefix('[')?;
    Some(inner.split(']').next().unwrap_or(inner).trim())
}

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.is_empty() || t.starts_with(';')
}

fn split_key(line: &str) -> Option<(&str, &str)> {
    if is_comment(line) || section_name(line).is_some() {
        return None;
    }
    let (k, v) = line.split_once('=')?;
    Some((k.trim(), v.trim()))
}

impl Ini {
    /// Lignes du corps de la section : (début, fin exclue).
    fn section(&self, name: &str) -> Option<(usize, usize)> {
        let start = self
            .lines
            .iter()
            .position(|l| section_name(l).is_some_and(|s| s.eq_ignore_ascii_case(name)))?;
        let end = self.lines[start + 1..]
            .iter()
            .position(|l| section_name(l).is_some())
            .map_or(self.lines.len(), |i| start + 1 + i);
        Some((start + 1, end))
    }

    fn sections(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|l| section_name(l).map(str::to_string))
            .collect()
    }

    fn keys(&self, sect: &str) -> Vec<String> {
        let Some((a, b)) = self.section(sect) else {
            return Vec::new();
        };
        self.lines[a..b]
            .iter()
            .filter_map(|l| split_key(l).map(|(k, _)| k.to_string()))
            .collect()
    }

    fn body(&self, sect: &str) -> Vec<String> {
        let Some((a, b)) = self.section(sect) else {
            return Vec::new();
        };
        self.lines[a..b]
            .iter()
            .filter(|l| !is_comment(l))
            .map(|l| match split_key(l) {
                Some((k, v)) => format!("{k}={v}"),
                None => l.trim().to_string(),
            })
            .collect()
    }

    fn get(&self, sect: &str, key: &str) -> Option<String> {
        let (a, b) = self.section(sect)?;
        self.lines[a..b].iter().find_map(|l| {
            let (k, v) = split_key(l)?;
            k.eq_ignore_ascii_case(key).then(|| unquote(v).to_string())
        })
    }

    fn set(&mut self, sect: &str, key: &str, value: Option<&str>) {
        let (a, b) = match self.section(sect) {
            Some(r) => r,
            None => {
                let Some(v) = value else {
                    return;
                };
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.push(format!("[{sect}]"));
                self.lines.push(format!("{key}={v}"));
                return;
            }
        };
        let found = (a..b)
            .find(|&i| split_key(&self.lines[i]).is_some_and(|(k, _)| k.eq_ignore_ascii_case(key)));
        match (found, value) {
            (Some(i), Some(v)) => {
                let k = split_key(&self.lines[i])
                    .map(|(k, _)| k.to_string())
                    .unwrap_or_default();
                self.lines[i] = format!("{k}={v}");
            }
            (Some(i), None) => {
                self.lines.remove(i);
            }
            (None, Some(v)) => {
                // Après la dernière ligne non vide de la section.
                let mut at = b;
                while at > a && self.lines[at - 1].trim().is_empty() {
                    at -= 1;
                }
                self.lines.insert(at, format!("{key}={v}"));
            }
            (None, None) => {}
        }
    }

    fn remove_section(&mut self, sect: &str) {
        if let Some((a, b)) = self.section(sect) {
            self.lines.drain(a - 1..b);
        }
    }

    fn replace_section(&mut self, sect: &str, body: &[String]) {
        match self.section(sect) {
            Some((a, b)) => {
                self.lines.splice(a..b, body.iter().cloned());
            }
            None => {
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.push(format!("[{sect}]"));
                self.lines.extend(body.iter().cloned());
            }
        }
    }
}

fn unquote(v: &str) -> &str {
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return &v[1..v.len() - 1];
        }
    }
    v
}

/// Valeur entière à la manière de GetPrivateProfileInt : décimal, ou
/// hexadécimal avec « 0x » ; s'arrête au premier caractère étranger.
fn parse_int(v: &str) -> i64 {
    let v = v.trim();
    let (neg, digits) = match v.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let (radix, digits) = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(r) => (16, r),
        None => (10, digits),
    };
    let mut n: i64 = 0;
    for c in digits.chars() {
        let Some(d) = c.to_digit(radix) else {
            break;
        };
        n = n.wrapping_mul(radix as i64).wrapping_add(d as i64);
    }
    if neg {
        -n
    } else {
        n
    }
}

// ── Sorties : tampons A (octets ANSI) et W (UTF-16) ─────────────────────

trait Out {
    fn units(s: &str) -> Vec<Self>
    where
        Self: Sized;
}
impl Out for u8 {
    fn units(s: &str) -> Vec<u8> {
        encode_ansi(s)
    }
}
impl Out for u16 {
    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }
}

/// Une chaîne simple, tronquée ; renvoie le nombre d'unités copiées.
unsafe fn put_one<T: Out + Copy + Default>(s: &str, out: *mut T, size: u32) -> u32 {
    if out.is_null() || size == 0 {
        return 0;
    }
    let u = T::units(s);
    let n = u.len().min(size as usize - 1);
    std::ptr::copy_nonoverlapping(u.as_ptr(), out, n);
    *out.add(n) = T::default();
    n as u32
}

/// Liste de chaînes terminées chacune par 0, puis un 0 final. Si elle ne
/// tient pas, elle est tronquée et la fonction rend `size − 2`.
unsafe fn put_list<T: Out + Copy + Default>(items: &[String], out: *mut T, size: u32) -> u32 {
    if out.is_null() || size == 0 {
        return 0;
    }
    if size == 1 {
        *out = T::default();
        return 0;
    }
    let mut all: Vec<T> = Vec::new();
    for s in items {
        all.extend(T::units(s));
        all.push(T::default());
    }
    let room = size as usize - 1;
    if all.len() <= room {
        std::ptr::copy_nonoverlapping(all.as_ptr(), out, all.len());
        *out.add(all.len()) = T::default();
        if all.is_empty() {
            *out.add(1.min(room)) = T::default();
        }
        return all.len() as u32;
    }
    std::ptr::copy_nonoverlapping(all.as_ptr(), out, room - 1);
    *out.add(room - 1) = T::default();
    *out.add(room) = T::default();
    size - 2
}

// ── Cœur commun ─────────────────────────────────────────────────────────

fn guard() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe fn get_string<T: Out + Copy + Default>(
    sect: Option<String>,
    key: Option<String>,
    default: Option<String>,
    out: *mut T,
    size: u32,
    file: &str,
) -> u32 {
    let _g = guard();
    let ini = load(file);
    match (sect, key) {
        (None, _) => put_list(&ini.sections(), out, size),
        (Some(s), None) => put_list(&ini.keys(&s), out, size),
        (Some(s), Some(k)) => {
            let v = ini.get(&s, &k).unwrap_or_else(|| {
                default
                    .map(|d| d.trim_end().to_string())
                    .unwrap_or_default()
            });
            put_one(&v, out, size)
        }
    }
}

fn get_int(sect: &str, key: &str, default: i32, file: &str) -> u32 {
    let _g = guard();
    match load(file).get(sect, key) {
        Some(v) => parse_int(&v) as u32,
        None => default as u32,
    }
}

fn write_string(
    sect: Option<String>,
    key: Option<String>,
    value: Option<String>,
    file: &str,
) -> i32 {
    let Some(sect) = sect else {
        // Section nulle : vide le cache de Windows, rien à écrire ici.
        return 1;
    };
    let _g = guard();
    let mut ini = load(file);
    match key {
        None => ini.remove_section(&sect),
        Some(k) => ini.set(&sect, &k, value.as_deref()),
    }
    if save(file, &ini) {
        1
    } else {
        crate::process::set_last_error(5);
        0
    }
}

fn read_section(sect: &str, file: &str) -> Vec<String> {
    let _g = guard();
    load(file).body(sect)
}

fn write_section(sect: &str, body: Option<Vec<String>>, file: &str) -> i32 {
    let _g = guard();
    let mut ini = load(file);
    match body {
        None => ini.remove_section(sect),
        Some(b) => ini.replace_section(sect, &b),
    }
    save(file, &ini) as i32
}

/// GetPrivateProfileStruct : octets en hexadécimal suivis d'une somme de
/// contrôle (somme des octets modulo 256).
fn get_struct(sect: &str, key: &str, out: *mut u8, size: u32, file: &str) -> i32 {
    let _g = guard();
    let Some(v) = load(file).get(sect, key) else {
        crate::process::set_last_error(2);
        return 0;
    };
    let bytes: Option<Vec<u8>> = (0..v.len() / 2)
        .map(|i| u8::from_str_radix(v.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect();
    let Some(bytes) = bytes else {
        return 0;
    };
    if v.len() % 2 != 0 || bytes.len() != size as usize + 1 || out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let sum = bytes[..size as usize]
        .iter()
        .fold(0u8, |a, &b| a.wrapping_add(b));
    if sum != bytes[size as usize] {
        crate::process::set_last_error(87);
        return 0;
    }
    // SAFETY: l'appelant fournit `size` octets.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, size as usize) };
    1
}

fn write_struct(sect: &str, key: &str, data: *const u8, size: u32, file: &str) -> i32 {
    let value = (!data.is_null()).then(|| {
        // SAFETY: l'appelant fournit `size` octets.
        let b = unsafe { std::slice::from_raw_parts(data, size as usize) };
        let sum = b.iter().fold(0u8, |a, &x| a.wrapping_add(x));
        b.iter()
            .chain(std::iter::once(&sum))
            .map(|x| format!("{x:02X}"))
            .collect::<String>()
    });
    write_string(Some(sect.into()), Some(key.into()), value, file)
}

// ── Exports W ───────────────────────────────────────────────────────────

unsafe fn ow(p: *const u16) -> Option<String> {
    (!p.is_null()).then(|| wstr(p))
}

unsafe fn oa(p: *const u8) -> Option<String> {
    (!p.is_null()).then(|| cstr(p))
}

/// Liste « k=v\0k=v\0\0 » fournie par WritePrivateProfileSection.
unsafe fn list_w(p: *const u16) -> Option<Vec<String>> {
    if p.is_null() {
        return None;
    }
    let mut v = Vec::new();
    let mut q = p;
    while *q != 0 {
        let s = wstr(q);
        q = q.add(s.encode_utf16().count() + 1);
        v.push(s);
    }
    Some(v)
}

unsafe fn list_a(p: *const u8) -> Option<Vec<String>> {
    if p.is_null() {
        return None;
    }
    let mut v = Vec::new();
    let mut q = p;
    while *q != 0 {
        let n = crate::printf::narrow_len(q);
        v.push(decode_ansi(std::slice::from_raw_parts(q, n)));
        q = q.add(n + 1);
    }
    Some(v)
}

const WIN_INI: &str = "win.ini";

unsafe extern "win64" fn get_private_profile_string_w(
    sect: *const u16,
    key: *const u16,
    def: *const u16,
    out: *mut u16,
    size: u32,
    file: *const u16,
) -> u32 {
    let f = ow(file).unwrap_or_else(|| WIN_INI.into());
    get_string(ow(sect), ow(key), ow(def), out, size, &f)
}

unsafe extern "win64" fn get_private_profile_string_a(
    sect: *const u8,
    key: *const u8,
    def: *const u8,
    out: *mut u8,
    size: u32,
    file: *const u8,
) -> u32 {
    let f = oa(file).unwrap_or_else(|| WIN_INI.into());
    get_string(oa(sect), oa(key), oa(def), out, size, &f)
}

unsafe extern "win64" fn get_profile_string_w(
    sect: *const u16,
    key: *const u16,
    def: *const u16,
    out: *mut u16,
    size: u32,
) -> u32 {
    get_string(ow(sect), ow(key), ow(def), out, size, WIN_INI)
}

unsafe extern "win64" fn get_profile_string_a(
    sect: *const u8,
    key: *const u8,
    def: *const u8,
    out: *mut u8,
    size: u32,
) -> u32 {
    get_string(oa(sect), oa(key), oa(def), out, size, WIN_INI)
}

unsafe extern "win64" fn get_private_profile_int_w(
    sect: *const u16,
    key: *const u16,
    def: i32,
    file: *const u16,
) -> u32 {
    let f = ow(file).unwrap_or_else(|| WIN_INI.into());
    get_int(&wstr(sect), &wstr(key), def, &f)
}

unsafe extern "win64" fn get_private_profile_int_a(
    sect: *const u8,
    key: *const u8,
    def: i32,
    file: *const u8,
) -> u32 {
    let f = oa(file).unwrap_or_else(|| WIN_INI.into());
    get_int(&cstr(sect), &cstr(key), def, &f)
}

unsafe extern "win64" fn get_profile_int_w(sect: *const u16, key: *const u16, def: i32) -> u32 {
    get_int(&wstr(sect), &wstr(key), def, WIN_INI)
}

unsafe extern "win64" fn get_profile_int_a(sect: *const u8, key: *const u8, def: i32) -> u32 {
    get_int(&cstr(sect), &cstr(key), def, WIN_INI)
}

unsafe extern "win64" fn write_private_profile_string_w(
    sect: *const u16,
    key: *const u16,
    value: *const u16,
    file: *const u16,
) -> i32 {
    let f = ow(file).unwrap_or_else(|| WIN_INI.into());
    write_string(ow(sect), ow(key), ow(value), &f)
}

unsafe extern "win64" fn write_private_profile_string_a(
    sect: *const u8,
    key: *const u8,
    value: *const u8,
    file: *const u8,
) -> i32 {
    let f = oa(file).unwrap_or_else(|| WIN_INI.into());
    write_string(oa(sect), oa(key), oa(value), &f)
}

unsafe extern "win64" fn write_profile_string_w(
    sect: *const u16,
    key: *const u16,
    value: *const u16,
) -> i32 {
    write_string(ow(sect), ow(key), ow(value), WIN_INI)
}

unsafe extern "win64" fn write_profile_string_a(
    sect: *const u8,
    key: *const u8,
    value: *const u8,
) -> i32 {
    write_string(oa(sect), oa(key), oa(value), WIN_INI)
}

unsafe extern "win64" fn get_private_profile_section_w(
    sect: *const u16,
    out: *mut u16,
    size: u32,
    file: *const u16,
) -> u32 {
    let f = ow(file).unwrap_or_else(|| WIN_INI.into());
    put_list(&read_section(&wstr(sect), &f), out, size)
}

unsafe extern "win64" fn get_private_profile_section_a(
    sect: *const u8,
    out: *mut u8,
    size: u32,
    file: *const u8,
) -> u32 {
    let f = oa(file).unwrap_or_else(|| WIN_INI.into());
    put_list(&read_section(&cstr(sect), &f), out, size)
}

unsafe extern "win64" fn get_profile_section_w(sect: *const u16, out: *mut u16, size: u32) -> u32 {
    put_list(&read_section(&wstr(sect), WIN_INI), out, size)
}

unsafe extern "win64" fn get_profile_section_a(sect: *const u8, out: *mut u8, size: u32) -> u32 {
    put_list(&read_section(&cstr(sect), WIN_INI), out, size)
}

unsafe extern "win64" fn get_private_profile_section_names_w(
    out: *mut u16,
    size: u32,
    file: *const u16,
) -> u32 {
    let f = ow(file).unwrap_or_else(|| WIN_INI.into());
    let names = {
        let _g = guard();
        load(&f).sections()
    };
    put_list(&names, out, size)
}

unsafe extern "win64" fn get_private_profile_section_names_a(
    out: *mut u8,
    size: u32,
    file: *const u8,
) -> u32 {
    let f = oa(file).unwrap_or_else(|| WIN_INI.into());
    let names = {
        let _g = guard();
        load(&f).sections()
    };
    put_list(&names, out, size)
}

unsafe extern "win64" fn write_private_profile_section_w(
    sect: *const u16,
    data: *const u16,
    file: *const u16,
) -> i32 {
    let f = ow(file).unwrap_or_else(|| WIN_INI.into());
    write_section(&wstr(sect), list_w(data), &f)
}

unsafe extern "win64" fn write_private_profile_section_a(
    sect: *const u8,
    data: *const u8,
    file: *const u8,
) -> i32 {
    let f = oa(file).unwrap_or_else(|| WIN_INI.into());
    write_section(&cstr(sect), list_a(data), &f)
}

unsafe extern "win64" fn write_profile_section_w(sect: *const u16, data: *const u16) -> i32 {
    write_section(&wstr(sect), list_w(data), WIN_INI)
}

unsafe extern "win64" fn write_profile_section_a(sect: *const u8, data: *const u8) -> i32 {
    write_section(&cstr(sect), list_a(data), WIN_INI)
}

unsafe extern "win64" fn get_private_profile_struct_w(
    sect: *const u16,
    key: *const u16,
    out: *mut u8,
    size: u32,
    file: *const u16,
) -> i32 {
    let f = ow(file).unwrap_or_else(|| WIN_INI.into());
    get_struct(&wstr(sect), &wstr(key), out, size, &f)
}

unsafe extern "win64" fn get_private_profile_struct_a(
    sect: *const u8,
    key: *const u8,
    out: *mut u8,
    size: u32,
    file: *const u8,
) -> i32 {
    let f = oa(file).unwrap_or_else(|| WIN_INI.into());
    get_struct(&cstr(sect), &cstr(key), out, size, &f)
}

unsafe extern "win64" fn write_private_profile_struct_w(
    sect: *const u16,
    key: *const u16,
    data: *const u8,
    size: u32,
    file: *const u16,
) -> i32 {
    let f = ow(file).unwrap_or_else(|| WIN_INI.into());
    write_struct(&wstr(sect), &wstr(key), data, size, &f)
}

unsafe extern "win64" fn write_private_profile_struct_a(
    sect: *const u8,
    key: *const u8,
    data: *const u8,
    size: u32,
    file: *const u8,
) -> i32 {
    let f = oa(file).unwrap_or_else(|| WIN_INI.into());
    write_struct(&cstr(sect), &cstr(key), data, size, &f)
}

/// Remplace les anciennes versions vides : on les pose avec `insert`.
pub fn kernel32_overrides() -> Exports {
    crate::exports! {
        "GetPrivateProfileStringW" => fn get_private_profile_string_w,
        "GetPrivateProfileStringA" => fn get_private_profile_string_a,
        "GetProfileStringW" => fn get_profile_string_w,
        "GetProfileStringA" => fn get_profile_string_a,
        "GetPrivateProfileIntW" => fn get_private_profile_int_w,
        "GetPrivateProfileIntA" => fn get_private_profile_int_a,
        "GetProfileIntW" => fn get_profile_int_w,
        "GetProfileIntA" => fn get_profile_int_a,
        "WritePrivateProfileStringW" => fn write_private_profile_string_w,
        "WritePrivateProfileStringA" => fn write_private_profile_string_a,
        "WriteProfileStringW" => fn write_profile_string_w,
        "WriteProfileStringA" => fn write_profile_string_a,
        "GetPrivateProfileSectionW" => fn get_private_profile_section_w,
        "GetPrivateProfileSectionA" => fn get_private_profile_section_a,
        "GetProfileSectionW" => fn get_profile_section_w,
        "GetProfileSectionA" => fn get_profile_section_a,
        "GetPrivateProfileSectionNamesW" => fn get_private_profile_section_names_w,
        "GetPrivateProfileSectionNamesA" => fn get_private_profile_section_names_a,
        "WritePrivateProfileSectionW" => fn write_private_profile_section_w,
        "WritePrivateProfileSectionA" => fn write_private_profile_section_a,
        "WriteProfileSectionW" => fn write_profile_section_w,
        "WriteProfileSectionA" => fn write_profile_section_a,
        "GetPrivateProfileStructW" => fn get_private_profile_struct_w,
        "GetPrivateProfileStructA" => fn get_private_profile_struct_a,
        "WritePrivateProfileStructW" => fn write_private_profile_struct_w,
        "WritePrivateProfileStructA" => fn write_private_profile_struct_a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ini(text: &str) -> Ini {
        parse(text.as_bytes())
    }

    #[test]
    fn lecture_insensible_a_la_casse() {
        let i = ini("; commentaire\n[Options]\nLangue = fr\nTaille=\"12\"\n\n[Autre]\nx=1\n");
        assert_eq!(i.get("options", "LANGUE").as_deref(), Some("fr"));
        assert_eq!(i.get("Options", "Taille").as_deref(), Some("12"));
        assert_eq!(i.get("Options", "x"), None);
        assert_eq!(i.sections(), ["Options", "Autre"]);
        assert_eq!(i.keys("Options"), ["Langue", "Taille"]);
        assert_eq!(i.body("Options"), ["Langue=fr", "Taille=\"12\""]);
    }

    #[test]
    fn ecriture_et_suppression() {
        let mut i = ini("[A]\nk=1\n\n[B]\nz=9\n");
        i.set("a", "K", Some("2"));
        i.set("A", "n", Some("3"));
        i.set("C", "c", Some("4"));
        assert_eq!(i.get("A", "k").as_deref(), Some("2"));
        assert_eq!(i.keys("A"), ["k", "n"]);
        assert_eq!(i.get("C", "c").as_deref(), Some("4"));
        i.set("A", "k", None);
        assert_eq!(i.keys("A"), ["n"]);
        i.remove_section("B");
        assert_eq!(i.sections(), ["A", "C"]);
        i.replace_section("A", &["p=1".into(), "q=2".into()]);
        assert_eq!(i.keys("A"), ["p", "q"]);
    }

    #[test]
    fn entiers() {
        assert_eq!(parse_int("42"), 42);
        assert_eq!(parse_int(" -7 "), -7);
        assert_eq!(parse_int("0x1F"), 31);
        assert_eq!(parse_int("12abc"), 12);
        assert_eq!(parse_int("abc"), 0);
    }

    #[test]
    fn encodage_utf16_conserve() {
        let mut b = vec![0xFF, 0xFE];
        for u in "[S]\r\nk=é\r\n".encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        let i = parse(&b);
        assert_eq!(i.encoding, Encoding::Utf16);
        assert_eq!(i.get("S", "k").as_deref(), Some("é"));
    }

    #[test]
    fn listes_et_troncature() {
        unsafe {
            let items = vec!["ab".to_string(), "cd".to_string()];
            let mut buf = [0xFFFFu16; 16];
            assert_eq!(put_list(&items, buf.as_mut_ptr(), 16), 6);
            assert_eq!(&buf[..7], &[97, 98, 0, 99, 100, 0, 0]);
            let mut small = [0xFFFFu16; 5];
            assert_eq!(put_list(&items, small.as_mut_ptr(), 5), 3);
            assert_eq!(&small[3..5], &[0, 0]);
            let mut one = [0u8; 4];
            assert_eq!(put_one::<u8>("bonjour", one.as_mut_ptr(), 4), 3);
            assert_eq!(&one, b"bon\0");
        }
    }

    #[test]
    fn fichier_reel_aller_retour() {
        let dir = std::env::temp_dir().join(format!("winlux-ini-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.ini");
        let f = path.to_string_lossy().to_string();
        assert_eq!(
            write_string(
                Some("Fenêtre".into()),
                Some("Largeur".into()),
                Some("800".into()),
                &f
            ),
            1
        );
        assert_eq!(get_int("fenêtre", "largeur", 5, &f), 800);
        assert_eq!(get_int("fenêtre", "hauteur", 5, &f), 5);
        let data = [1u8, 2, 250];
        assert_eq!(write_struct("S", "bin", data.as_ptr(), 3, &f), 1);
        let mut back = [0u8; 3];
        assert_eq!(get_struct("S", "bin", back.as_mut_ptr(), 3, &f), 1);
        assert_eq!(back, data);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
