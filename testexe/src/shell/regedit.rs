//! Éditeur du Registre façon Windows 11 : arbre des clés à gauche, valeurs
//! (Nom, Type, Données) à droite, barre d'adresse, modification des
//! chaînes, nombres, chaînes multiples et données binaires, création,
//! renommage, suppression, recherche, import et export de fichiers .reg.
//!
//! Tout passe par les API Win32 du runtime (RegOpenKeyExA, RegEnumValueA…) :
//! l'éditeur voit et modifie exactement le registre des applications.

use crate::admin_ui::{button, button_w, chip, dialog, input, Field, Hits};
use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const PATH_MAX: usize = 400;
const NAME_MAX: usize = 128;
const SHOW_MAX: usize = 200;
const MAX_NODES: usize = 640;
const MAX_VALUES: usize = 256;
const ROW_H: i32 = 26;
const BAR_H: i32 = 44;
const ADDR_H: i32 = 36;
const STATUS_H: i32 = 26;

const HIVES: [(&[u8], usize); 5] = [
    (b"HKEY_CLASSES_ROOT", 0x8000_0000),
    (b"HKEY_CURRENT_USER", 0x8000_0001),
    (b"HKEY_LOCAL_MACHINE", 0x8000_0002),
    (b"HKEY_USERS", 0x8000_0003),
    (b"HKEY_CURRENT_CONFIG", 0x8000_0005),
];
const SHORT: [&[u8]; 5] = [b"HKCR", b"HKCU", b"HKLM", b"HKU", b"HKCC"];

const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const REG_BINARY: u32 = 3;
const REG_DWORD: u32 = 4;
const REG_MULTI_SZ: u32 = 7;
const REG_QWORD: u32 = 11;

// ---------- Textes ----------

const T_TITLE: Tr4 = [
    b"\xC9diteur du Registre",
    b"Registry Editor",
    b"Editor del Registro",
    b"Registrierungs-Editor",
];
const T_COMPUTER: Tr4 = [b"Ordinateur", b"Computer", b"Equipo", b"Computer"];
const T_NEW: Tr4 = [b"Nouveau", b"New", b"Nuevo", b"Neu"];
const T_EDIT: Tr4 = [b"Modifier", b"Modify", b"Modificar", b"\xC4ndern"];
const T_RENAME: Tr4 = [b"Renommer", b"Rename", b"Cambiar nombre", b"Umbenennen"];
const T_DELETE: Tr4 = [b"Supprimer", b"Delete", b"Eliminar", b"L\xF6schen"];
const T_FIND: Tr4 = [b"Rechercher", b"Find", b"Buscar", b"Suchen"];
const T_FIND_NEXT: Tr4 = [
    b"Suivant",
    b"Find next",
    b"Buscar siguiente",
    b"Weitersuchen",
];
const T_IMPORT: Tr4 = [b"Importer", b"Import", b"Importar", b"Importieren"];
const T_EXPORT: Tr4 = [b"Exporter", b"Export", b"Exportar", b"Exportieren"];
const T_REFRESH: Tr4 = [b"Actualiser", b"Refresh", b"Actualizar", b"Aktualisieren"];
const T_KEY: Tr4 = [b"Cl\xE9", b"Key", b"Clave", b"Schl\xFCssel"];
const T_SZ: Tr4 = [
    b"Valeur cha\xEEne",
    b"String Value",
    b"Valor de cadena",
    b"Zeichenfolge",
];
const T_DWORD: Tr4 = [
    b"Valeur DWORD (32 bits)",
    b"DWORD (32-bit) Value",
    b"Valor de DWORD (32 bits)",
    b"DWORD-Wert (32-Bit)",
];
const T_QWORD: Tr4 = [
    b"Valeur QWORD (64 bits)",
    b"QWORD (64-bit) Value",
    b"Valor de QWORD (64 bits)",
    b"QWORD-Wert (64-Bit)",
];
const T_MULTI: Tr4 = [
    b"Valeur de cha\xEEnes multiples",
    b"Multi-String Value",
    b"Valor de cadena m\xFAltiple",
    b"Wert der mehrteiligen Zeichenfolge",
];
const T_EXPAND: Tr4 = [
    b"Valeur de cha\xEEne extensible",
    b"Expandable String Value",
    b"Valor de cadena expandible",
    b"Wert der erweiterbaren Zeichenfolge",
];
const T_BINARY: Tr4 = [
    b"Valeur binaire",
    b"Binary Value",
    b"Valor binario",
    b"Bin\xE4rwert",
];
const T_NAME: Tr4 = [b"Nom", b"Name", b"Nombre", b"Name"];
const T_TYPE: Tr4 = [b"Type", b"Type", b"Tipo", b"Typ"];
const T_DATA: Tr4 = [b"Donn\xE9es", b"Data", b"Datos", b"Daten"];
const T_DEFAULT: Tr4 = [
    b"(par d\xE9faut)",
    b"(Default)",
    b"(predeterminado)",
    b"(Standard)",
];
const T_UNSET: Tr4 = [
    b"(valeur non d\xE9finie)",
    b"(value not set)",
    b"(valor no establecido)",
    b"(Wert nicht festgelegt)",
];
const T_OK: Tr4 = [b"OK", b"OK", b"Aceptar", b"OK"];
const T_CANCEL: Tr4 = [b"Annuler", b"Cancel", b"Cancelar", b"Abbrechen"];
const T_YES: Tr4 = [b"Oui", b"Yes", b"S\xED", b"Ja"];
const T_NO: Tr4 = [b"Non", b"No", b"No", b"Nein"];
const T_HEX: Tr4 = [
    b"Hexad\xE9cimale",
    b"Hexadecimal",
    b"Hexadecimal",
    b"Hexadezimal",
];
const T_DEC: Tr4 = [b"D\xE9cimale", b"Decimal", b"Decimal", b"Dezimal"];
const T_VALUE_NAME: Tr4 = [
    b"Nom de la valeur",
    b"Value name",
    b"Nombre de valor",
    b"Name",
];
const T_VALUE_DATA: Tr4 = [
    b"Donn\xE9es de la valeur",
    b"Value data",
    b"Informaci\xF3n del valor",
    b"Wert",
];
const T_EDIT_SZ: Tr4 = [
    b"Modifier la cha\xEEne",
    b"Edit String",
    b"Editar cadena",
    b"Zeichenfolge bearbeiten",
];
const T_EDIT_NUM: Tr4 = [
    b"Modifier la valeur",
    b"Edit Value",
    b"Editar valor",
    b"Wert bearbeiten",
];
const T_EDIT_MULTI: Tr4 = [
    b"Modifier la cha\xEEne multiple",
    b"Edit Multi-String",
    b"Editar cadena m\xFAltiple",
    b"Mehrteilige Zeichenfolge bearbeiten",
];
const T_EDIT_BIN: Tr4 = [
    b"Modifier la valeur binaire",
    b"Edit Binary Value",
    b"Editar valor binario",
    b"Bin\xE4rwert bearbeiten",
];
const T_MULTI_HINT: Tr4 = [
    b"Une valeur par ligne (Entr\xE9e pour aller \xE0 la ligne).",
    b"One value per line (Enter starts a new line).",
    b"Un valor por l\xEDnea (Intro para una l\xEDnea nueva).",
    b"Ein Wert pro Zeile (Eingabe f\xFCr neue Zeile).",
];
const T_BIN_HINT: Tr4 = [
    b"Octets en hexad\xE9cimal, par exemple : 01 a0 ff",
    b"Bytes in hexadecimal, for example: 01 a0 ff",
    b"Bytes en hexadecimal, por ejemplo: 01 a0 ff",
    b"Bytes hexadezimal, zum Beispiel: 01 a0 ff",
];
const T_NEW_NAME: Tr4 = [b"Nouveau nom", b"New name", b"Nombre nuevo", b"Neuer Name"];
const T_CONFIRM_KEY: Tr4 = [
    b"Voulez-vous vraiment supprimer cette cl\xE9 et toutes ses sous-cl\xE9s ?",
    b"Are you sure you want to permanently delete this key and all of its subkeys?",
    b"\xBFEst\xE1 seguro de que desea eliminar esta clave y todas sus subclaves?",
    b"Soll dieser Schl\xFCssel mit allen Unterschl\xFCsseln gel\xF6scht werden?",
];
const T_CONFIRM_VALUE: Tr4 = [
    b"La suppression de certaines valeurs peut rendre une application instable. Supprimer cette valeur ?",
    b"Deleting certain registry values could cause system instability. Delete this value?",
    b"Eliminar ciertos valores puede causar inestabilidad. \xBFEliminar este valor?",
    b"Das L\xF6schen bestimmter Werte kann zu Instabilit\xE4t f\xFChren. Wert l\xF6schen?",
];
const T_CONFIRM_TITLE: Tr4 = [
    b"Confirmer la suppression",
    b"Confirm Delete",
    b"Confirmar eliminaci\xF3n",
    b"L\xF6schen best\xE4tigen",
];
const T_FIND_WHAT: Tr4 = [b"Rechercher :", b"Find what:", b"Buscar:", b"Suchen nach:"];
const T_LOOK_KEYS: Tr4 = [b"Cl\xE9s", b"Keys", b"Claves", b"Schl\xFCssel"];
const T_LOOK_VALUES: Tr4 = [b"Valeurs", b"Values", b"Valores", b"Werte"];
const T_LOOK_DATA: Tr4 = [b"Donn\xE9es", b"Data", b"Datos", b"Daten"];
const T_NOT_FOUND: Tr4 = [
    b"Recherche termin\xE9e : aucun autre r\xE9sultat.",
    b"Finished searching through the registry.",
    b"Termin\xF3 la b\xFAsqueda en el Registro.",
    b"Die Registrierung wurde vollst\xE4ndig durchsucht.",
];
const T_IMPORT_TITLE: Tr4 = [
    b"Importer un fichier du Registre",
    b"Import Registry File",
    b"Importar archivo del Registro",
    b"Registrierungsdatei importieren",
];
const T_EXPORT_TITLE: Tr4 = [
    b"Exporter un fichier du Registre",
    b"Export Registry File",
    b"Exportar archivo del Registro",
    b"Registrierungsdatei exportieren",
];
const T_FILE: Tr4 = [
    b"Chemin du fichier .reg",
    b"Path of the .reg file",
    b"Ruta del archivo .reg",
    b"Pfad der .reg-Datei",
];
const T_SELECTED_BRANCH: Tr4 = [
    b"Branche s\xE9lectionn\xE9e",
    b"Selected branch",
    b"Rama seleccionada",
    b"Ausgew\xE4hlte Teilstruktur",
];
const T_ALL: Tr4 = [b"Tout", b"All", b"Todo", b"Alles"];
const T_IMPORT_ASK: Tr4 = [
    b"Ajouter au Registre les informations de ce fichier ? Ne le faites que si vous faites confiance \xE0 sa source.",
    b"Adding information can unintentionally change or delete values. Continue only if you trust the source of this file.",
    b"Agregar informaci\xF3n puede cambiar o eliminar valores. Contin\xFAe solo si conf\xEDa en el origen del archivo.",
    b"Durch das Hinzuf\xFCgen k\xF6nnen Werte ge\xE4ndert oder gel\xF6scht werden. Nur fortfahren, wenn Sie der Quelle vertrauen.",
];
const T_IMPORTED: Tr4 = [
    b"Les informations du fichier ont \xE9t\xE9 ajout\xE9es au Registre.",
    b"The keys and values have been successfully added to the registry.",
    b"Las claves y valores se agregaron correctamente al Registro.",
    b"Die Schl\xFCssel und Werte wurden der Registrierung hinzugef\xFCgt.",
];
const T_IMPORT_FAIL: Tr4 = [
    b"Impossible d'importer ce fichier : ce n'est pas un fichier .reg valide.",
    b"Cannot import the file: it is not a valid registry script.",
    b"No se puede importar: no es un script de Registro v\xE1lido.",
    b"Import nicht m\xF6glich: keine g\xFCltige Registrierungsdatei.",
];
const T_EXPORTED: Tr4 = [
    b"Export termin\xE9.",
    b"Export complete.",
    b"Exportaci\xF3n completada.",
    b"Export abgeschlossen.",
];
const T_EXPORT_FAIL: Tr4 = [
    b"Export impossible : v\xE9rifiez le chemin du fichier.",
    b"Export failed: check the file path.",
    b"Error al exportar: compruebe la ruta.",
    b"Export fehlgeschlagen: Pfad pr\xFCfen.",
];
const T_FAIL: Tr4 = [
    b"L'op\xE9ration a \xE9chou\xE9 (nom d\xE9j\xE0 utilis\xE9 ou cl\xE9 prot\xE9g\xE9e).",
    b"The operation failed (name already in use or protected key).",
    b"La operaci\xF3n fall\xF3 (nombre en uso o clave protegida).",
    b"Vorgang fehlgeschlagen (Name vergeben oder gesch\xFCtzter Schl\xFCssel).",
];
const T_BAD_DATA: Tr4 = [
    b"Donn\xE9es non valides pour ce type de valeur.",
    b"Invalid data for this value type.",
    b"Datos no v\xE1lidos para este tipo.",
    b"Ung\xFCltige Daten f\xFCr diesen Typ.",
];
const T_NEW_KEY_NAME: Tr4 = [
    b"Nouvelle cl\xE9 #",
    b"New Key #",
    b"Clave nueva #",
    b"Neuer Schl\xFCssel #",
];
const T_NEW_VALUE_NAME: Tr4 = [
    b"Nouvelle valeur #",
    b"New Value #",
    b"Nuevo valor #",
    b"Neuer Wert #",
];

// ---------- État ----------

#[derive(Clone, Copy)]
struct Node {
    /// 255 = « Ordinateur » ; sinon indice dans HIVES.
    hive: u8,
    depth: u8,
    expanded: bool,
    children: bool,
    len: u16,
    path: [u8; PATH_MAX],
}

const EMPTY_NODE: Node = Node {
    hive: 255,
    depth: 0,
    expanded: false,
    children: true,
    len: 0,
    path: [0; PATH_MAX],
};

#[derive(Clone, Copy)]
struct Val {
    kind: u32,
    synthetic: bool,
    nlen: u8,
    name: [u8; NAME_MAX],
    slen: u8,
    show: [u8; SHOW_MAX],
}

const EMPTY_VAL: Val = Val {
    kind: 0,
    synthetic: false,
    nlen: 0,
    name: [0; NAME_MAX],
    slen: 0,
    show: [0; SHOW_MAX],
};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Browse,
    EditString,
    EditNumber,
    EditMulti,
    EditBinary,
    RenameKey,
    RenameValue,
    ConfirmDeleteKey,
    ConfirmDeleteValue,
    Find,
    Import,
    Export,
    ConfirmImport,
}

static mut NODES: [Node; MAX_NODES] = [EMPTY_NODE; MAX_NODES];
static mut NODE_COUNT: usize = 0;
static mut SEL: usize = 0;
static mut VALUES: [Val; MAX_VALUES] = [EMPTY_VAL; MAX_VALUES];
static mut VALUE_COUNT: usize = 0;
static mut VSEL: usize = usize::MAX;
static mut FOCUS_VALUES: bool = false;
static mut TREE_SCROLL: usize = 0;
static mut VAL_SCROLL: usize = 0;
static mut MODE: Mode = Mode::Browse;
static mut MENU_OPEN: bool = false;
static mut HITS: Hits = Hits::new();
static mut READY: bool = false;

static mut ADDR: Field<PATH_MAX> = Field::new();
static mut ADDR_FOCUS: bool = false;
/// Texte prérempli « sélectionné » : la première frappe le remplace.
static mut FRESH: bool = false;
/// Champ principal d'un dialogue (données, nom, recherche, fichier).
static mut EDIT: Field<2048> = Field::new();
static mut EDIT_NAME: [u8; NAME_MAX] = [0; NAME_MAX];
static mut EDIT_NAME_LEN: usize = 0;
static mut EDIT_KIND: u32 = 0;
static mut HEX_BASE: bool = true;
static mut EXPORT_ALL: bool = false;
static mut LOOK: [bool; 3] = [true, true, true];

static mut MESSAGE: &[u8] = b"";
static mut MESSAGE_TICK: u32 = 0;
static mut LAST_TREE: Rect = rect(0, 0, 0, 0);
static mut LAST_VALUES: Rect = rect(0, 0, 0, 0);

const ID_NEW: u16 = 1;
const ID_EDIT: u16 = 2;
const ID_RENAME: u16 = 3;
const ID_DELETE: u16 = 4;
const ID_FIND: u16 = 5;
const ID_IMPORT: u16 = 6;
const ID_EXPORT: u16 = 7;
const ID_REFRESH: u16 = 8;
const ID_ADDR: u16 = 9;
const ID_MENU: u16 = 20; // + 0..7 : clé, chaîne, binaire, DWORD, QWORD, multi, extensible
const ID_OK: u16 = 40;
const ID_CANCEL: u16 = 41;
const ID_HEX: u16 = 42;
const ID_DEC: u16 = 43;
const ID_FIELD: u16 = 44;
const ID_LOOK: u16 = 45; // + 0..2
const ID_SCOPE_SEL: u16 = 50;
const ID_SCOPE_ALL: u16 = 51;
const ID_TREE: u16 = 1000; // + ligne visible
const ID_TOGGLE: u16 = 3000; // + ligne visible
const ID_VAL: u16 = 5000; // + valeur

fn say(m: &'static [u8]) {
    unsafe {
        MESSAGE = m;
        MESSAGE_TICK = GetTickCount();
    }
}

// ---------- Petits outils sans allocation ----------

fn lower(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' | 0xC0..=0xDE if c != 0xD7 => c + 32,
        _ => c,
    }
}

fn eq_nocase(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| lower(*x) == lower(*y))
}

fn contains_nocase(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || (needle.len() <= hay.len()
            && (0..=hay.len() - needle.len()).any(|i| eq_nocase(&hay[i..i + needle.len()], needle)))
}

struct Buf<const N: usize> {
    b: [u8; N],
    n: usize,
}

impl<const N: usize> Buf<N> {
    const fn new() -> Self {
        Buf { b: [0; N], n: 0 }
    }
    fn push(&mut self, s: &[u8]) -> &mut Self {
        for &c in s {
            if self.n + 1 < N {
                self.b[self.n] = c;
                self.n += 1;
            }
        }
        self
    }
    fn hex(&mut self, v: u64, digits: usize) -> &mut Self {
        const D: &[u8; 16] = b"0123456789abcdef";
        for i in (0..digits).rev() {
            self.push(&[D[((v >> (i * 4)) & 15) as usize]]);
        }
        self
    }
    fn dec(&mut self, mut v: u64) -> &mut Self {
        let mut tmp = [0u8; 20];
        let mut i = 20;
        loop {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        self.push(&tmp[i..])
    }
    fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
    /// Pointeur vers le texte terminé par un zéro.
    fn cstr(&mut self) -> *const i8 {
        self.b[self.n.min(N - 1)] = 0;
        self.b.as_ptr().cast()
    }
}

fn parse_u64(s: &[u8], hex: bool) -> Option<u64> {
    let s = trim(s);
    let (s, hex) = match s {
        [b'0', b'x' | b'X', rest @ ..] => (rest, true),
        _ => (s, hex),
    };
    if s.is_empty() {
        return None;
    }
    let mut v: u64 = 0;
    for &c in s {
        let d = match c {
            b'0'..=b'9' => (c - b'0') as u64,
            b'a'..=b'f' if hex => (c - b'a' + 10) as u64,
            b'A'..=b'F' if hex => (c - b'A' + 10) as u64,
            _ => return None,
        };
        v = v.checked_mul(if hex { 16 } else { 10 })?.checked_add(d)?;
    }
    Some(v)
}

fn trim(s: &[u8]) -> &[u8] {
    let a = s.iter().position(|c| *c != b' ').unwrap_or(s.len());
    let b = s.iter().rposition(|c| *c != b' ').map_or(a, |i| i + 1);
    &s[a..b]
}

fn type_name(kind: u32) -> &'static [u8] {
    match kind {
        0 => b"REG_NONE",
        1 => b"REG_SZ",
        2 => b"REG_EXPAND_SZ",
        3 => b"REG_BINARY",
        4 => b"REG_DWORD",
        5 => b"REG_DWORD_BIG_ENDIAN",
        6 => b"REG_LINK",
        7 => b"REG_MULTI_SZ",
        8 => b"REG_RESOURCE_LIST",
        11 => b"REG_QWORD",
        _ => b"REG_UNKNOWN",
    }
}

fn is_text(kind: u32) -> bool {
    matches!(kind, REG_SZ | REG_EXPAND_SZ | REG_MULTI_SZ)
}

// ---------- Accès au registre ----------

unsafe fn node_path(i: usize) -> &'static [u8] {
    &NODES[i].path[..NODES[i].len as usize]
}

/// Ouvre la clé d'un nœud ; `None` pour « Ordinateur » ou si elle a disparu.
unsafe fn open(hive: u8, path: &[u8]) -> Option<Handle> {
    if hive as usize >= HIVES.len() {
        return None;
    }
    let root = HIVES[hive as usize].1 as Handle;
    if path.is_empty() {
        return Some(root);
    }
    let mut p = Buf::<{ PATH_MAX + 1 }>::new();
    p.push(path);
    let mut h: Handle = core::ptr::null_mut();
    (RegOpenKeyExA(root, p.cstr(), 0, 0x2_0019, &mut h) == 0).then_some(h)
}

unsafe fn close(h: Handle) {
    if (h as usize) < 0x8000_0000 || (h as usize) > 0x8000_00FF {
        RegCloseKey(h);
    }
}

unsafe fn has_subkeys(h: Handle) -> bool {
    let mut n = 0u32;
    RegQueryInfoKeyA(
        h,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        &mut n,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    ) == 0
        && n > 0
}

/// Nom de la sous-clé `index`, ou `None` à la fin.
unsafe fn subkey(h: Handle, index: u32, out: &mut [u8; NAME_MAX * 2]) -> Option<usize> {
    let mut len = out.len() as u32;
    (RegEnumKeyExA(
        h,
        index,
        out.as_mut_ptr().cast(),
        &mut len,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    ) == 0)
        .then_some(len as usize)
}

unsafe fn init() {
    NODE_COUNT = 1 + HIVES.len();
    NODES[0] = EMPTY_NODE;
    NODES[0].expanded = true;
    for (i, _) in HIVES.iter().enumerate() {
        NODES[i + 1] = Node {
            hive: i as u8,
            depth: 1,
            expanded: false,
            children: true,
            len: 0,
            path: [0; PATH_MAX],
        };
    }
    SEL = 0;
    load_values();
    READY = true;
    // `regedit fichier.reg` : import proposé avec confirmation.
    let mut v = [0u8; 1024];
    let n = GetEnvironmentVariableA(
        c"WINLUX_REGEDIT_IMPORT".as_ptr(),
        v.as_mut_ptr().cast(),
        v.len() as u32,
    ) as usize;
    if n > 0 && n < v.len() {
        EDIT.set(&v[..n]);
        MODE = Mode::ConfirmImport;
    }
}

unsafe fn expand(i: usize) {
    let node = NODES[i];
    if node.expanded || node.hive == 255 {
        NODES[i].expanded = true;
        return;
    }
    let Some(h) = open(node.hive, node_path(i)) else {
        return;
    };
    let mut name = [0u8; NAME_MAX * 2];
    let mut index = 0;
    let mut at = i + 1;
    while let Some(n) = subkey(h, index, &mut name) {
        index += 1;
        if NODE_COUNT >= MAX_NODES || node.len as usize + n + 1 >= PATH_MAX {
            continue;
        }
        let mut child = Node {
            hive: node.hive,
            depth: node.depth + 1,
            expanded: false,
            children: false,
            len: 0,
            path: [0; PATH_MAX],
        };
        let mut l = node.len as usize;
        child.path[..l].copy_from_slice(&node.path[..l]);
        if l > 0 {
            child.path[l] = b'\\';
            l += 1;
        }
        child.path[l..l + n].copy_from_slice(&name[..n]);
        child.len = (l + n) as u16;
        if let Some(c) = open(child.hive, &child.path[..child.len as usize]) {
            child.children = has_subkeys(c);
            close(c);
        }
        let mut j = NODE_COUNT;
        while j > at {
            NODES[j] = NODES[j - 1];
            j -= 1;
        }
        NODES[at] = child;
        NODE_COUNT += 1;
        if SEL >= at && SEL != i {
            SEL += 1;
        }
        at += 1;
    }
    close(h);
    NODES[i].expanded = true;
    NODES[i].children = index > 0;
}

unsafe fn collapse(i: usize) {
    let depth = NODES[i].depth;
    let mut end = i + 1;
    while end < NODE_COUNT && NODES[end].depth > depth {
        end += 1;
    }
    let removed = end - i - 1;
    if SEL > i && SEL < end {
        SEL = i;
    } else if SEL >= end {
        SEL -= removed;
    }
    for j in end..NODE_COUNT {
        NODES[j - removed] = NODES[j];
    }
    NODE_COUNT -= removed;
    if NODES[i].hive != 255 {
        NODES[i].expanded = false;
    }
}

/// Relit les enfants d'un nœud déplié (après création, renommage…).
unsafe fn reload(i: usize) {
    let was = NODES[i].expanded;
    collapse(i);
    NODES[i].expanded = false;
    NODES[i].children = true;
    if was || NODES[i].hive == 255 {
        expand(i);
    }
}

unsafe fn format_value(v: &mut Val, data: &[u8]) {
    let mut s = Buf::<SHOW_MAX>::new();
    match v.kind {
        REG_SZ | REG_EXPAND_SZ => {
            let end = data.iter().position(|c| *c == 0).unwrap_or(data.len());
            s.push(&data[..end]);
        }
        REG_MULTI_SZ => {
            let mut first = true;
            for part in data.split(|c| *c == 0).filter(|p| !p.is_empty()) {
                if !first {
                    s.push(b" ");
                }
                s.push(part);
                first = false;
            }
        }
        REG_DWORD if data.len() == 4 => {
            let n = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as u64;
            s.push(b"0x").hex(n, 8).push(b" (").dec(n).push(b")");
        }
        REG_QWORD if data.len() == 8 => {
            let mut b = [0u8; 8];
            b.copy_from_slice(data);
            let n = u64::from_le_bytes(b);
            s.push(b"0x").hex(n, 16).push(b" (").dec(n).push(b")");
        }
        _ => {
            if data.is_empty() {
                s.push(tr4(&[
                    b"(valeur binaire de longueur nulle)",
                    b"(zero-length binary value)",
                    b"(valor binario de longitud cero)",
                    b"(Bin\xE4rwert der L\xE4nge null)",
                ]));
            }
            for (i, b) in data.iter().take(64).enumerate() {
                if i > 0 {
                    s.push(b" ");
                }
                s.hex(*b as u64, 2);
            }
        }
    }
    v.slen = s.n as u8;
    v.show[..s.n].copy_from_slice(s.get());
}

unsafe fn load_values() {
    VALUE_COUNT = 0;
    VAL_SCROLL = 0;
    VSEL = usize::MAX;
    let node = NODES[SEL];
    let Some(h) = open(node.hive, node_path(SEL)) else {
        return;
    };
    let mut data = [0u8; 4096];
    let mut has_default = false;
    let mut index = 0;
    loop {
        let mut v = EMPTY_VAL;
        let mut nlen = NAME_MAX as u32;
        let mut dlen = data.len() as u32;
        let r = RegEnumValueA(
            h,
            index,
            v.name.as_mut_ptr().cast(),
            &mut nlen,
            core::ptr::null_mut(),
            &mut v.kind,
            data.as_mut_ptr(),
            &mut dlen,
        );
        index += 1;
        if r == 259 || index > 4096 {
            break;
        }
        if r != 0 && r != 234 {
            continue;
        }
        v.nlen = nlen as u8;
        let dlen = (dlen as usize).min(data.len());
        format_value(&mut v, &data[..dlen]);
        if v.nlen == 0 {
            has_default = true;
        }
        if VALUE_COUNT < MAX_VALUES {
            VALUES[VALUE_COUNT] = v;
            VALUE_COUNT += 1;
        }
    }
    close(h);
    if !has_default {
        // Comme regedit : la valeur par défaut apparaît même vide.
        let mut v = EMPTY_VAL;
        v.kind = REG_SZ;
        v.synthetic = true;
        let unset = tr4(&T_UNSET);
        v.show[..unset.len()].copy_from_slice(unset);
        v.slen = unset.len() as u8;
        let mut j = VALUE_COUNT.min(MAX_VALUES - 1);
        while j > 0 {
            VALUES[j] = VALUES[j - 1];
            j -= 1;
        }
        VALUES[0] = v;
        VALUE_COUNT = (VALUE_COUNT + 1).min(MAX_VALUES);
    }
}

unsafe fn select(i: usize) {
    if i < NODE_COUNT {
        SEL = i;
        load_values();
        sync_address();
    }
}

unsafe fn sync_address() {
    let mut b = Buf::<PATH_MAX>::new();
    b.push(tr4(&T_COMPUTER));
    let node = NODES[SEL];
    if node.hive != 255 {
        b.push(b"\\").push(HIVES[node.hive as usize].0);
        if node.len > 0 {
            b.push(b"\\").push(node_path(SEL));
        }
    }
    ADDR.set(b.get());
}

/// Va à une adresse saisie (`Ordinateur\HKEY_…\…`, `HKLM\…`).
unsafe fn navigate(text: &[u8]) -> bool {
    let mut parts = text
        .split(|c| *c == b'\\' || *c == b'/')
        .map(trim)
        .filter(|p| !p.is_empty())
        .peekable();
    if let Some(first) = parts.peek() {
        if [T_COMPUTER[0], T_COMPUTER[1], T_COMPUTER[2]]
            .iter()
            .any(|c| eq_nocase(first, c))
        {
            parts.next();
        }
    }
    let Some(root) = parts.next() else {
        select(0);
        return true;
    };
    let Some(h) =
        (0..HIVES.len()).find(|&h| eq_nocase(root, HIVES[h].0) || eq_nocase(root, SHORT[h]))
    else {
        return false;
    };
    let mut at = h + 1;
    for part in parts {
        expand(at);
        let depth = NODES[at].depth + 1;
        let mut j = at + 1;
        let mut found = None;
        while j < NODE_COUNT && NODES[j].depth >= depth {
            if NODES[j].depth == depth {
                let p = node_path(j);
                let name = p.rsplit(|c| *c == b'\\').next().unwrap_or(p);
                if eq_nocase(name, part) {
                    found = Some(j);
                    break;
                }
            }
            j += 1;
        }
        match found {
            Some(j) => at = j,
            None => {
                select(at);
                return false;
            }
        }
    }
    select(at);
    ensure_visible();
    true
}

unsafe fn ensure_visible() {
    let rows = ((LAST_TREE.bottom - LAST_TREE.top) / ROW_H).max(1) as usize;
    if SEL < TREE_SCROLL {
        TREE_SCROLL = SEL;
    } else if SEL >= TREE_SCROLL + rows {
        TREE_SCROLL = SEL + 1 - rows;
    }
}

unsafe fn value_name(i: usize) -> &'static [u8] {
    &VALUES[i].name[..VALUES[i].nlen as usize]
}

/// Lit les données brutes d'une valeur de la clé sélectionnée.
unsafe fn read_value(name: &[u8], out: &mut [u8]) -> Option<(u32, usize)> {
    let h = open(NODES[SEL].hive, node_path(SEL))?;
    let mut n = Buf::<{ NAME_MAX + 1 }>::new();
    n.push(name);
    let mut kind = 0u32;
    let mut size = out.len() as u32;
    let r = RegQueryValueExA(
        h,
        n.cstr(),
        core::ptr::null_mut(),
        &mut kind,
        out.as_mut_ptr(),
        &mut size,
    );
    close(h);
    (r == 0).then_some((kind, size as usize))
}

unsafe fn write_value(name: &[u8], kind: u32, data: &[u8]) -> bool {
    let Some(h) = open(NODES[SEL].hive, node_path(SEL)) else {
        return false;
    };
    let mut n = Buf::<{ NAME_MAX + 1 }>::new();
    n.push(name);
    let r = RegSetValueExA(h, n.cstr(), 0, kind, data.as_ptr(), data.len() as u32);
    close(h);
    r == 0
}

// ---------- Commandes ----------

unsafe fn start_edit(i: usize) {
    if i >= VALUE_COUNT || NODES[SEL].hive == 255 {
        return;
    }
    VSEL = i;
    let name = value_name(i);
    EDIT_NAME[..name.len()].copy_from_slice(name);
    EDIT_NAME_LEN = name.len();
    let mut data = [0u8; 2048];
    let (kind, len) = if VALUES[i].synthetic {
        (REG_SZ, 0)
    } else {
        match read_value(name, &mut data) {
            Some(v) => v,
            None => return,
        }
    };
    let len = len.min(data.len());
    EDIT_KIND = kind;
    EDIT.len = 0;
    MODE = match kind {
        REG_SZ | REG_EXPAND_SZ => {
            let end = data[..len].iter().position(|c| *c == 0).unwrap_or(len);
            EDIT.set(&data[..end]);
            Mode::EditString
        }
        REG_MULTI_SZ => {
            let mut first = true;
            for part in data[..len].split(|c| *c == 0).filter(|p| !p.is_empty()) {
                if !first {
                    EDIT.push(b"\n");
                }
                EDIT.push(part);
                first = false;
            }
            Mode::EditMulti
        }
        REG_DWORD | REG_QWORD if len == 4 || len == 8 => {
            let mut b = [0u8; 8];
            b[..len].copy_from_slice(&data[..len]);
            let v = u64::from_le_bytes(b);
            let mut s = Buf::<24>::new();
            if HEX_BASE {
                // Sans zéros de tête, comme regedit.
                let digits = 64 - v.leading_zeros().min(63) as usize;
                s.hex(v, digits.div_ceil(4));
            } else {
                s.dec(v);
            }
            EDIT.set(s.get());
            Mode::EditNumber
        }
        _ => {
            for (k, b) in data[..len].iter().enumerate() {
                let mut s = Buf::<4>::new();
                if k > 0 {
                    s.push(b" ");
                }
                s.hex(*b as u64, 2);
                EDIT.push(s.get());
            }
            Mode::EditBinary
        }
    };
}

unsafe fn save_edit() {
    let name = &EDIT_NAME[..EDIT_NAME_LEN];
    let text = EDIT.get();
    let mut data = [0u8; 2100];
    let len = match MODE {
        Mode::EditString => {
            data[..text.len()].copy_from_slice(text);
            text.len() + 1
        }
        Mode::EditMulti => {
            let mut n = 0;
            for line in text.split(|c| *c == b'\n').filter(|l| !l.is_empty()) {
                data[n..n + line.len()].copy_from_slice(line);
                n += line.len() + 1;
            }
            n + 1
        }
        Mode::EditNumber => {
            let Some(v) = parse_u64(text, HEX_BASE) else {
                say(tr4(&T_BAD_DATA));
                return;
            };
            if EDIT_KIND == REG_DWORD {
                if v > u32::MAX as u64 {
                    say(tr4(&T_BAD_DATA));
                    return;
                }
                data[..4].copy_from_slice(&(v as u32).to_le_bytes());
                4
            } else {
                data[..8].copy_from_slice(&v.to_le_bytes());
                8
            }
        }
        _ => {
            let digits: [u8; 2048] = {
                let mut d = [0u8; 2048];
                let mut n = 0;
                for &c in text {
                    if c.is_ascii_hexdigit() {
                        d[n] = c;
                        n += 1;
                    } else if c != b' ' && c != b',' {
                        say(tr4(&T_BAD_DATA));
                        return;
                    }
                }
                if n % 2 == 1 {
                    say(tr4(&T_BAD_DATA));
                    return;
                }
                d[n] = 0;
                d
            };
            let n = digits.iter().position(|c| *c == 0).unwrap_or(0);
            for k in 0..n / 2 {
                data[k] = parse_u64(&digits[k * 2..k * 2 + 2], true).unwrap_or(0) as u8;
            }
            n / 2
        }
    };
    if write_value(name, EDIT_KIND, &data[..len]) {
        MODE = Mode::Browse;
        let keep = VSEL;
        load_values();
        VSEL = find_value(name).unwrap_or(keep);
    } else {
        say(tr4(&T_FAIL));
    }
}

unsafe fn find_value(name: &[u8]) -> Option<usize> {
    (0..VALUE_COUNT).find(|&i| !VALUES[i].synthetic && eq_nocase(value_name(i), name))
}

/// Nom libre « Nouvelle clé #N ».
unsafe fn free_name(prefix: &'static [u8], exists: impl Fn(&[u8]) -> bool) -> Buf<NAME_MAX> {
    let mut n = 1;
    loop {
        let mut b = Buf::<NAME_MAX>::new();
        b.push(prefix).dec(n);
        if !exists(b.get()) || n > 999 {
            return b;
        }
        n += 1;
    }
}

unsafe fn child_index(parent: usize, name: &[u8]) -> Option<usize> {
    let depth = NODES[parent].depth + 1;
    let mut j = parent + 1;
    while j < NODE_COUNT && NODES[j].depth >= depth {
        let p = node_path(j);
        let last = p.rsplit(|c| *c == b'\\').next().unwrap_or(p);
        if NODES[j].depth == depth && eq_nocase(last, name) {
            return Some(j);
        }
        j += 1;
    }
    None
}

unsafe fn new_item(which: u16) {
    MENU_OPEN = false;
    if NODES[SEL].hive == 255 {
        return;
    }
    if which == 0 {
        let parent = SEL;
        expand(parent);
        let name = free_name(tr4(&T_NEW_KEY_NAME), |n| child_index(parent, n).is_some());
        let Some(h) = open(NODES[parent].hive, node_path(parent)) else {
            return;
        };
        let mut nb = Buf::<{ NAME_MAX + 1 }>::new();
        nb.push(name.get());
        let mut out: Handle = core::ptr::null_mut();
        let r = RegCreateKeyExA(
            h,
            nb.cstr(),
            0,
            core::ptr::null(),
            0,
            0xF003F,
            core::ptr::null_mut(),
            &mut out,
            core::ptr::null_mut(),
        );
        close(h);
        if r != 0 {
            say(tr4(&T_FAIL));
            return;
        }
        close(out);
        NODES[parent].expanded = true;
        reload(parent);
        if let Some(j) = child_index(parent, name.get()) {
            select(j);
            ensure_visible();
            EDIT.set(name.get());
            MODE = Mode::RenameKey;
            FRESH = true;
        }
        return;
    }
    let (kind, data): (u32, &[u8]) = match which {
        1 => (REG_SZ, b"\0"),
        2 => (REG_BINARY, b""),
        3 => (REG_DWORD, &[0, 0, 0, 0]),
        4 => (REG_QWORD, &[0; 8]),
        5 => (REG_MULTI_SZ, b"\0\0"),
        _ => (REG_EXPAND_SZ, b"\0"),
    };
    let name = free_name(tr4(&T_NEW_VALUE_NAME), |n| find_value(n).is_some());
    if !write_value(name.get(), kind, data) {
        say(tr4(&T_FAIL));
        return;
    }
    load_values();
    if let Some(i) = find_value(name.get()) {
        VSEL = i;
        FOCUS_VALUES = true;
        EDIT_NAME[..name.n].copy_from_slice(name.get());
        EDIT_NAME_LEN = name.n;
        EDIT.set(name.get());
        MODE = Mode::RenameValue;
        FRESH = true;
    }
}

unsafe fn start_rename() {
    if FOCUS_VALUES {
        if VSEL < VALUE_COUNT && VALUES[VSEL].nlen > 0 {
            let name = value_name(VSEL);
            EDIT_NAME[..name.len()].copy_from_slice(name);
            EDIT_NAME_LEN = name.len();
            EDIT.set(name);
            MODE = Mode::RenameValue;
            FRESH = true;
        }
    } else if NODES[SEL].hive != 255 && NODES[SEL].len > 0 {
        let p = node_path(SEL);
        EDIT.set(p.rsplit(|c| *c == b'\\').next().unwrap_or(p));
        MODE = Mode::RenameKey;
        FRESH = true;
    }
}

unsafe fn parent_of(i: usize) -> usize {
    let depth = NODES[i].depth;
    let mut j = i;
    while j > 0 && NODES[j].depth >= depth {
        j -= 1;
    }
    j
}

unsafe fn save_rename() {
    let new = trim(EDIT.get());
    if new.is_empty() || new.contains(&b'\\') {
        say(tr4(&T_BAD_DATA));
        return;
    }
    let mut nb = Buf::<{ NAME_MAX + 1 }>::new();
    nb.push(new);
    if MODE == Mode::RenameValue {
        let Some(h) = open(NODES[SEL].hive, node_path(SEL)) else {
            return;
        };
        let mut old = Buf::<{ NAME_MAX + 1 }>::new();
        old.push(&EDIT_NAME[..EDIT_NAME_LEN]);
        let r = WinluxRegRenameValueA(h, old.cstr(), nb.cstr());
        close(h);
        if r != 0 && !eq_nocase(&EDIT_NAME[..EDIT_NAME_LEN], new) {
            say(tr4(&T_FAIL));
            return;
        }
        MODE = Mode::Browse;
        load_values();
        VSEL = find_value(new).unwrap_or(usize::MAX);
        return;
    }
    let node = NODES[SEL];
    let mut p = Buf::<{ PATH_MAX + 1 }>::new();
    p.push(node_path(SEL));
    let r = WinluxRegRenameKeyA(HIVES[node.hive as usize].1 as Handle, p.cstr(), nb.cstr());
    if r != 0 {
        say(tr4(&T_FAIL));
        return;
    }
    MODE = Mode::Browse;
    let parent = parent_of(SEL);
    reload(parent);
    if let Some(j) = child_index(parent, new) {
        select(j);
    }
}

unsafe fn delete_selected() {
    MODE = Mode::Browse;
    if FOCUS_VALUES {
        if VSEL >= VALUE_COUNT || VALUES[VSEL].synthetic {
            return;
        }
        let Some(h) = open(NODES[SEL].hive, node_path(SEL)) else {
            return;
        };
        let mut n = Buf::<{ NAME_MAX + 1 }>::new();
        n.push(value_name(VSEL));
        let r = RegDeleteValueA(h, n.cstr());
        close(h);
        if r != 0 {
            say(tr4(&T_FAIL));
        }
        load_values();
        return;
    }
    let node = NODES[SEL];
    if node.hive == 255 || node.len == 0 {
        return;
    }
    let mut p = Buf::<{ PATH_MAX + 1 }>::new();
    p.push(node_path(SEL));
    if RegDeleteTreeA(HIVES[node.hive as usize].1 as Handle, p.cstr()) != 0 {
        say(tr4(&T_FAIL));
        return;
    }
    let parent = parent_of(SEL);
    reload(parent);
    select(parent);
}

unsafe fn ask_delete() {
    if FOCUS_VALUES {
        if VSEL < VALUE_COUNT && !VALUES[VSEL].synthetic {
            MODE = Mode::ConfirmDeleteValue;
        }
    } else if NODES[SEL].hive != 255 && NODES[SEL].len > 0 {
        MODE = Mode::ConfirmDeleteKey;
    }
}

// ---------- Recherche ----------

const FIND_DEPTH: usize = 40;
static mut F_HIVE: u8 = 0;
static mut F_ALL: bool = false;
static mut F_BASE: usize = 0;
static mut F_DEPTH: usize = 0;
static mut F_CHILD: [u32; FIND_DEPTH] = [0; FIND_DEPTH];
static mut F_VAL: [u32; FIND_DEPTH] = [0; FIND_DEPTH];
static mut F_LEN: [u16; FIND_DEPTH] = [0; FIND_DEPTH];
static mut F_PATH: [u8; PATH_MAX] = [0; PATH_MAX];
static mut F_ACTIVE: bool = false;
static mut F_TEXT: Field<128> = Field::new();

unsafe fn find_start() {
    F_TEXT.set(EDIT.get());
    let node = NODES[SEL];
    F_ALL = node.hive == 255;
    F_HIVE = if F_ALL { 0 } else { node.hive };
    let base = if F_ALL { 0 } else { node.len as usize };
    F_PATH[..base].copy_from_slice(&node.path[..base]);
    F_BASE = base;
    F_DEPTH = 0;
    F_CHILD[0] = 0;
    F_VAL[0] = 0;
    F_LEN[0] = base as u16;
    F_ACTIVE = true;
    find_next();
}

/// Reprend le parcours en profondeur là où la recherche précédente s'est
/// arrêtée ; s'arrête au premier résultat.
unsafe fn find_next() {
    if !F_ACTIVE {
        MODE = Mode::Find;
        EDIT.set(F_TEXT.get());
        return;
    }
    let needle = F_TEXT.get();
    let mut name = [0u8; NAME_MAX * 2];
    let mut data = [0u8; 2048];
    let mut budget = 200_000u32;
    while budget > 0 {
        budget -= 1;
        let len = F_LEN[F_DEPTH] as usize;
        let Some(h) = open(F_HIVE, &F_PATH[..len]) else {
            if !pop() {
                break;
            }
            continue;
        };
        // Valeurs de la clé courante.
        if F_VAL[F_DEPTH] != u32::MAX && (LOOK[1] || LOOK[2]) {
            let mut nlen = NAME_MAX as u32;
            let mut dlen = data.len() as u32;
            let mut kind = 0u32;
            let r = RegEnumValueA(
                h,
                F_VAL[F_DEPTH],
                name.as_mut_ptr().cast(),
                &mut nlen,
                core::ptr::null_mut(),
                &mut kind,
                data.as_mut_ptr(),
                &mut dlen,
            );
            if r == 0 || r == 234 {
                F_VAL[F_DEPTH] += 1;
                close(h);
                let n = &name[..nlen as usize];
                let d = &data[..(dlen as usize).min(data.len())];
                if (LOOK[1] && contains_nocase(n, needle))
                    || (LOOK[2] && is_text(kind) && contains_nocase(d, needle))
                {
                    found(len, Some(n));
                    return;
                }
                continue;
            }
            F_VAL[F_DEPTH] = u32::MAX;
        }
        // Sous-clé suivante.
        match subkey(h, F_CHILD[F_DEPTH], &mut name) {
            Some(n) if F_DEPTH + 1 < FIND_DEPTH && len + n + 1 < PATH_MAX => {
                close(h);
                F_CHILD[F_DEPTH] += 1;
                let mut l = len;
                if l > 0 {
                    F_PATH[l] = b'\\';
                    l += 1;
                }
                F_PATH[l..l + n].copy_from_slice(&name[..n]);
                F_DEPTH += 1;
                F_LEN[F_DEPTH] = (l + n) as u16;
                F_CHILD[F_DEPTH] = 0;
                F_VAL[F_DEPTH] = 0;
                if LOOK[0] && contains_nocase(&name[..n], needle) {
                    found(l + n, None);
                    return;
                }
            }
            Some(_) => {
                close(h);
                F_CHILD[F_DEPTH] += 1;
            }
            None => {
                close(h);
                if !pop() {
                    break;
                }
            }
        }
    }
    F_ACTIVE = false;
    say(tr4(&T_NOT_FOUND));
}

/// Remonte d'un niveau ; passe à la ruche suivante si on cherche partout.
unsafe fn pop() -> bool {
    if F_DEPTH > 0 {
        F_DEPTH -= 1;
        return true;
    }
    if F_ALL && (F_HIVE as usize) + 1 < HIVES.len() {
        F_HIVE += 1;
        F_CHILD[0] = 0;
        F_VAL[0] = 0;
        F_LEN[0] = 0;
        return true;
    }
    false
}

unsafe fn found(len: usize, value: Option<&[u8]>) {
    let mut addr = Buf::<{ PATH_MAX + 32 }>::new();
    addr.push(HIVES[F_HIVE as usize].0);
    if len > 0 {
        addr.push(b"\\").push(&F_PATH[..len]);
    }
    let mut copy = [0u8; PATH_MAX + 32];
    copy[..addr.n].copy_from_slice(addr.get());
    navigate(&copy[..addr.n]);
    FOCUS_VALUES = value.is_some();
    if let Some(v) = value {
        VSEL = find_value(v).unwrap_or(if v.is_empty() { 0 } else { usize::MAX });
        let rows = ((LAST_VALUES.bottom - LAST_VALUES.top) / ROW_H - 1).max(1) as usize;
        if VSEL != usize::MAX && VSEL >= VAL_SCROLL + rows {
            VAL_SCROLL = VSEL + 1 - rows;
        }
    }
}

// ---------- Import et export ----------

unsafe fn default_export_path() {
    let mut b = Buf::<PATH_MAX>::new();
    b.push(b"C:\\Documents\\");
    let node = NODES[SEL];
    if EXPORT_ALL || node.hive == 255 {
        b.push(b"registre");
    } else if node.len == 0 {
        b.push(HIVES[node.hive as usize].0);
    } else {
        let p = node_path(SEL);
        b.push(p.rsplit(|c| *c == b'\\').next().unwrap_or(p));
    }
    b.push(b".reg");
    EDIT.set(b.get());
}

unsafe fn do_import() {
    let mut p = Buf::<2100>::new();
    p.push(trim(EDIT.get()));
    let r = WinluxRegImportA(p.cstr());
    MODE = Mode::Browse;
    if r < 0 {
        say(tr4(&T_IMPORT_FAIL));
        return;
    }
    say(tr4(&T_IMPORTED));
    refresh_all();
}

unsafe fn do_export() {
    let mut p = Buf::<2100>::new();
    p.push(trim(EDIT.get()));
    let mut key = Buf::<{ PATH_MAX + 32 }>::new();
    let node = NODES[SEL];
    if !EXPORT_ALL && node.hive != 255 {
        key.push(HIVES[node.hive as usize].0);
        if node.len > 0 {
            key.push(b"\\").push(node_path(SEL));
        }
    }
    let r = WinluxRegExportA(p.cstr(), key.cstr());
    MODE = Mode::Browse;
    say(tr4(if r < 0 { &T_EXPORT_FAIL } else { &T_EXPORTED }));
}

/// Relit l'arbre déplié et les valeurs (F5, après un import).
unsafe fn refresh_all() {
    let mut addr = [0u8; PATH_MAX];
    let n = ADDR.len;
    addr[..n].copy_from_slice(ADDR.get());
    for h in 0..HIVES.len() {
        if let Some(i) = (0..NODE_COUNT).find(|&i| NODES[i].hive == h as u8 && NODES[i].depth == 1)
        {
            if NODES[i].expanded {
                collapse(i);
            }
        }
    }
    navigate(&addr[..n]);
}

// ---------- Dessin ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    if !READY {
        init();
        sync_address();
    }
    let h = &mut HITS;
    h.clear();
    let c = f.client;
    let p = pal();
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, p.window);

    // Barre de commandes.
    fill(g.hdc, g.b, c.left, c.top, c.right, c.top + BAR_H, p.mica);
    let has_key = NODES[SEL].hive != 255;
    let can_edit = has_key
        && if FOCUS_VALUES {
            VSEL < VALUE_COUNT
        } else {
            NODES[SEL].len > 0
        };
    let mut x = c.left + 12;
    let y = c.top + 7;
    let commands: [(u16, &[u8], bool, bool); 8] = [
        (ID_NEW, tr4(&T_NEW), has_key, true),
        (
            ID_EDIT,
            tr4(&T_EDIT),
            FOCUS_VALUES && VSEL < VALUE_COUNT,
            false,
        ),
        (
            ID_RENAME,
            tr4(&T_RENAME),
            can_edit && !(FOCUS_VALUES && VSEL < VALUE_COUNT && VALUES[VSEL].nlen == 0),
            false,
        ),
        (
            ID_DELETE,
            tr4(&T_DELETE),
            can_edit && !(FOCUS_VALUES && VSEL < VALUE_COUNT && VALUES[VSEL].synthetic),
            false,
        ),
        (ID_FIND, tr4(&T_FIND), true, false),
        (ID_IMPORT, tr4(&T_IMPORT), true, false),
        (ID_EXPORT, tr4(&T_EXPORT), true, false),
        (ID_REFRESH, tr4(&T_REFRESH), true, false),
    ];
    let mut new_x = x;
    for (id, label, enabled, primary) in commands {
        let w = button_w(label) + if id == ID_NEW { 14 } else { 0 };
        if x + w > c.right - 8 {
            break;
        }
        if id == ID_NEW {
            new_x = x;
        }
        button(g, h, rect(x, y, x + w, y + 30), id, label, primary, enabled);
        if id == ID_NEW {
            set_glyph_bg(Some(p.accent));
            set_glyph_fg(Some(p.on_accent));
            glyph(g.hdc, g.b, g.f, Glyph::ChevronDown, x + w - 18, y + 10, 10);
            set_glyph_fg(None);
            set_glyph_bg(None);
        }
        x += w + 6;
    }

    // Barre d'adresse.
    let ay = c.top + BAR_H;
    fill(g.hdc, g.b, c.left, ay, c.right, ay + 1, p.border);
    input(
        g,
        h,
        rect(c.left + 12, ay + 4, c.right - 12, ay + ADDR_H - 4),
        ID_ADDR,
        ADDR.get(),
        ADDR_FOCUS,
        b"",
    );

    // Volets.
    let top = ay + ADDR_H;
    let bottom = c.bottom - STATUS_H;
    let tree_w = ((c.right - c.left) * 32 / 100).clamp(200, 420);
    let split = c.left + tree_w;
    LAST_TREE = rect(c.left, top, split, bottom);
    LAST_VALUES = rect(split + 1, top, c.right, bottom);
    fill(g.hdc, g.b, c.left, top, split, bottom, p.mica);
    fill(g.hdc, g.b, split, top, split + 1, bottom, p.border);
    draw_tree(g, h, &LAST_TREE);
    draw_values(g, h, &LAST_VALUES);

    // Barre d'état : chemin complet ou dernier message.
    fill(g.hdc, g.b, c.left, bottom, c.right, bottom + 1, p.border);
    fill(g.hdc, g.b, c.left, bottom + 1, c.right, c.bottom, p.mica);
    let recent = !MESSAGE.is_empty() && GetTickCount().wrapping_sub(MESSAGE_TICK) < 8000;
    text_fit(
        g.hdc,
        g.f.caption,
        12,
        if recent { p.accent } else { p.text2 },
        c.left + 12,
        bottom + 6,
        if recent { MESSAGE } else { ADDR.get() },
        c.right - c.left - 24,
    );

    if MENU_OPEN {
        draw_menu(g, h, new_x, y + 32);
    }
    match MODE {
        Mode::Browse => {}
        Mode::ConfirmDeleteKey | Mode::ConfirmDeleteValue | Mode::ConfirmImport => {
            draw_confirm(g, h, &c)
        }
        _ => draw_dialog(g, h, &c),
    }
}

unsafe fn draw_tree(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let rows = ((a.bottom - a.top - 8) / ROW_H).max(1) as usize;
    if TREE_SCROLL + rows > NODE_COUNT {
        TREE_SCROLL = NODE_COUNT.saturating_sub(rows);
    }
    for row in 0..rows {
        let i = TREE_SCROLL + row;
        if i >= NODE_COUNT {
            break;
        }
        let node = NODES[i];
        let y = a.top + 4 + row as i32 * ROW_H;
        let r = rect(a.left + 4, y, a.right - 4, y + ROW_H - 2);
        let hover = h.add(r, ID_TREE + row as u16);
        let selected = i == SEL;
        let bg = if selected {
            if FOCUS_VALUES {
                p.hover
            } else {
                p.selected
            }
        } else if hover {
            p.hover
        } else {
            p.mica
        };
        if selected || hover {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, bg);
        }
        if selected && !FOCUS_VALUES {
            round_rect(
                g.hdc,
                g.b,
                r.left,
                r.top + 6,
                r.left + 3,
                r.bottom - 6,
                2,
                p.accent,
            );
        }
        let indent = a.left + 10 + node.depth as i32 * 16;
        set_glyph_bg(Some(bg));
        if node.children {
            h.add(
                rect(indent - 2, y, indent + 16, y + ROW_H - 2),
                ID_TOGGLE + row as u16,
            );
            let gl = if node.expanded {
                Glyph::ChevronDown
            } else {
                Glyph::Chevron
            };
            glyph(g.hdc, g.b, g.f, gl, indent + 2, y + 8, 9);
        }
        let icon = if node.hive == 255 {
            Glyph::Computer
        } else {
            Glyph::Folder
        };
        glyph(g.hdc, g.b, g.f, icon, indent + 18, y + 4, 16);
        set_glyph_bg(None);
        let label: &[u8] = if node.hive == 255 {
            tr4(&T_COMPUTER)
        } else if node.len == 0 {
            HIVES[node.hive as usize].0
        } else {
            let path = node_path(i);
            path.rsplit(|c| *c == b'\\').next().unwrap_or(path)
        };
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            indent + 40,
            y + 5,
            label,
            a.right - indent - 48,
        );
    }
    scrollbar(g, a, TREE_SCROLL, rows, NODE_COUNT);
}

unsafe fn scrollbar(g: &mut Gfx, a: &Rect, first: usize, rows: usize, total: usize) {
    if total <= rows {
        return;
    }
    let p = pal();
    let track = a.bottom - a.top - 8;
    let thumb = (track * rows as i32 / total as i32).max(24);
    let y = a.top + 4 + (track - thumb) * first as i32 / (total - rows).max(1) as i32;
    round_rect(
        g.hdc,
        g.b,
        a.right - 6,
        y,
        a.right - 3,
        y + thumb,
        2,
        p.text3,
    );
}

unsafe fn draw_values(g: &mut Gfx, h: &mut Hits, a: &Rect) {
    let p = pal();
    let w = a.right - a.left;
    let name_w = (w * 30 / 100).clamp(140, 280);
    let type_w = 150;
    let x = a.left + 16;
    let cols: [(&[u8], i32); 3] = [
        (tr4(&T_NAME), name_w),
        (tr4(&T_TYPE), type_w),
        (tr4(&T_DATA), w - name_w - type_w - 24),
    ];
    let mut cx = x;
    for (label, cw) in cols {
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            p.text2,
            cx,
            a.top + 8,
            label,
            cw - 10,
        );
        cx += cw;
    }
    fill(
        g.hdc,
        g.b,
        a.left + 8,
        a.top + 30,
        a.right - 8,
        a.top + 31,
        p.border,
    );
    if NODES[SEL].hive == 255 {
        return;
    }
    let first = a.top + 34;
    let rows = ((a.bottom - first) / ROW_H).max(1) as usize;
    if VAL_SCROLL + rows > VALUE_COUNT {
        VAL_SCROLL = VALUE_COUNT.saturating_sub(rows);
    }
    for row in 0..rows {
        let i = VAL_SCROLL + row;
        if i >= VALUE_COUNT {
            break;
        }
        let v = &VALUES[i];
        let y = first + row as i32 * ROW_H;
        let r = rect(a.left + 6, y, a.right - 8, y + ROW_H - 2);
        let hover = h.add(r, ID_VAL + i as u16);
        let selected = i == VSEL;
        let bg = if selected && FOCUS_VALUES {
            p.selected
        } else if hover || selected {
            p.hover
        } else {
            p.window
        };
        if selected || hover {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, bg);
        }
        set_glyph_bg(Some(bg));
        let icon = if is_text(v.kind) {
            Glyph::Text
        } else {
            Glyph::Code
        };
        glyph(g.hdc, g.b, g.f, icon, x - 2, y + 4, 16);
        set_glyph_bg(None);
        let name: &[u8] = if v.nlen == 0 {
            tr4(&T_DEFAULT)
        } else {
            &v.name[..v.nlen as usize]
        };
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text,
            x + 22,
            y + 5,
            name,
            name_w - 30,
        );
        text_fit(
            g.hdc,
            g.f.body,
            13,
            p.text2,
            x + name_w,
            y + 5,
            type_name(v.kind),
            type_w - 10,
        );
        text_fit(
            g.hdc,
            g.f.body,
            13,
            if v.synthetic { p.text3 } else { p.text },
            x + name_w + type_w,
            y + 5,
            &v.show[..v.slen as usize],
            a.right - x - name_w - type_w - 16,
        );
    }
    scrollbar(
        g,
        &rect(a.left, first, a.right, a.bottom),
        VAL_SCROLL,
        rows,
        VALUE_COUNT,
    );
}

const MENU_ITEMS: [&Tr4; 7] = [
    &T_KEY, &T_SZ, &T_BINARY, &T_DWORD, &T_QWORD, &T_MULTI, &T_EXPAND,
];

unsafe fn draw_menu(g: &mut Gfx, h: &mut Hits, x: i32, y: i32) {
    let p = pal();
    let w = MENU_ITEMS
        .iter()
        .map(|t| text_width(tr4(t), 13))
        .max()
        .unwrap_or(120)
        + 48;
    let ht = MENU_ITEMS.len() as i32 * 32 + 8 + 9;
    panel(g.hdc, g.b, x, y, x + w, y + ht, 8, p.surface, p.border);
    h.add(rect(x, y, x + w, y + ht), 0xFFFE);
    let mut iy = y + 4;
    for (k, t) in MENU_ITEMS.iter().enumerate() {
        let r = rect(x + 4, iy, x + w - 4, iy + 30);
        let hover = h.add(r, ID_MENU + k as u16);
        if hover {
            round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, p.hover);
        }
        let icon = match k {
            0 => Glyph::Folder,
            1 | 5 | 6 => Glyph::Text,
            _ => Glyph::Code,
        };
        set_glyph_bg(Some(if hover { p.hover } else { p.surface }));
        glyph(g.hdc, g.b, g.f, icon, r.left + 10, iy + 7, 16);
        set_glyph_bg(None);
        text(g.hdc, g.f.body, p.text, r.left + 36, iy + 7, tr4(t));
        iy += 32;
        if k == 0 {
            fill(g.hdc, g.b, x + 8, iy + 4, x + w - 8, iy + 5, p.border);
            iy += 9;
        }
    }
}

unsafe fn ok_cancel(g: &mut Gfx, h: &mut Hits, d: &Rect, ok: &'static [u8], cancel: &'static [u8]) {
    let wo = button_w(ok).max(96);
    let wc = button_w(cancel).max(96);
    let y = d.bottom - 52;
    button(
        g,
        h,
        rect(d.right - 24 - wc, y, d.right - 24, y + 32),
        ID_CANCEL,
        cancel,
        false,
        true,
    );
    button(
        g,
        h,
        rect(d.right - 32 - wc - wo, y, d.right - 32 - wc, y + 32),
        ID_OK,
        ok,
        true,
        true,
    );
}

unsafe fn draw_confirm(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let (title, body): (&[u8], &[u8]) = match MODE {
        Mode::ConfirmDeleteKey => (tr4(&T_CONFIRM_TITLE), tr4(&T_CONFIRM_KEY)),
        Mode::ConfirmDeleteValue => (tr4(&T_CONFIRM_TITLE), tr4(&T_CONFIRM_VALUE)),
        _ => (tr4(&T_TITLE), tr4(&T_IMPORT_ASK)),
    };
    let d = dialog(g, h, c, 520, 230, title);
    let mut y = d.top + 64;
    if MODE == Mode::ConfirmImport {
        text_fit(
            g.hdc,
            g.f.semibold,
            13,
            p.text,
            d.left + 24,
            y,
            EDIT.get(),
            d.right - d.left - 48,
        );
        y += 26;
    }
    wrap(g, d.left + 24, y, d.right - d.left - 48, body, p.text2);
    ok_cancel(g, h, &d, tr4(&T_YES), tr4(&T_NO));
}

/// Texte sur plusieurs lignes, coupé aux espaces.
unsafe fn wrap(g: &mut Gfx, x: i32, mut y: i32, w: i32, s: &[u8], color: u32) {
    let mut start = 0;
    while start < s.len() {
        let mut end = s.len();
        while end > start && text_width(&s[start..end], 13) > w {
            end = s[start..end]
                .iter()
                .rposition(|c| *c == b' ')
                .map_or(start, |i| start + i);
        }
        if end == start {
            end = s.len();
        }
        text(g.hdc, g.f.body, color, x, y, &s[start..end]);
        y += 20;
        start = end + 1;
    }
}

unsafe fn draw_dialog(g: &mut Gfx, h: &mut Hits, c: &Rect) {
    let p = pal();
    let multi = MODE == Mode::EditMulti;
    let (title, height): (&[u8], i32) = match MODE {
        Mode::EditString => (tr4(&T_EDIT_SZ), 260),
        Mode::EditNumber => (tr4(&T_EDIT_NUM), 290),
        Mode::EditMulti => (tr4(&T_EDIT_MULTI), 400),
        Mode::EditBinary => (tr4(&T_EDIT_BIN), 280),
        Mode::RenameKey | Mode::RenameValue => (tr4(&T_RENAME), 200),
        Mode::Find => (tr4(&T_FIND), 250),
        Mode::Import => (tr4(&T_IMPORT_TITLE), 200),
        _ => (tr4(&T_EXPORT_TITLE), 250),
    };
    let d = dialog(g, h, c, 540, height, title);
    let x = d.left + 24;
    let w = d.right - d.left - 48;
    let mut y = d.top + 62;
    let editing = matches!(
        MODE,
        Mode::EditString | Mode::EditNumber | Mode::EditMulti | Mode::EditBinary
    );
    if editing {
        text(g.hdc, g.f.caption, p.text2, x, y, tr4(&T_VALUE_NAME));
        y += 20;
        let name: &[u8] = if EDIT_NAME_LEN == 0 {
            tr4(&T_DEFAULT)
        } else {
            &EDIT_NAME[..EDIT_NAME_LEN]
        };
        panel(g.hdc, g.b, x, y, x + w, y + 32, 4, p.mica, p.border);
        text_fit(g.hdc, g.f.body, 13, p.text2, x + 10, y + 8, name, w - 20);
        y += 44;
    }
    let label: &[u8] = match MODE {
        Mode::RenameKey | Mode::RenameValue => tr4(&T_NEW_NAME),
        Mode::Find => tr4(&T_FIND_WHAT),
        Mode::Import | Mode::Export => tr4(&T_FILE),
        _ => tr4(&T_VALUE_DATA),
    };
    text(g.hdc, g.f.caption, p.text2, x, y, label);
    y += 20;
    if multi {
        let box_h = 150;
        h.add(rect(x, y, x + w, y + box_h), ID_FIELD);
        panel(g.hdc, g.b, x, y, x + w, y + box_h, 4, p.surface, p.border);
        fill(
            g.hdc,
            g.b,
            x + 2,
            y + box_h - 2,
            x + w - 2,
            y + box_h,
            p.accent,
        );
        let text_all = EDIT.get();
        let lines = text_all.split(|c| *c == b'\n').count();
        let shown = ((box_h - 12) / 18) as usize;
        let skip = lines.saturating_sub(shown);
        let mut ly = y + 6;
        let mut last_w = 0;
        for line in text_all.split(|c| *c == b'\n').skip(skip) {
            text_fit(g.hdc, g.f.body, 13, p.text, x + 10, ly, line, w - 20);
            last_w = text_width(line, 13).min(w - 22);
            ly += 18;
        }
        let cy = ly - 18;
        fill(
            g.hdc,
            g.b,
            x + 11 + last_w,
            cy,
            x + 12 + last_w,
            cy + 16,
            p.text,
        );
        y += box_h + 8;
        text(g.hdc, g.f.caption, p.text3, x, y, tr4(&T_MULTI_HINT));
    } else {
        input(
            g,
            h,
            rect(x, y, x + w, y + 34),
            ID_FIELD,
            EDIT.get(),
            true,
            b"",
        );
        y += 44;
        match MODE {
            Mode::EditNumber => {
                let mut cx = x;
                cx += chip(g, h, cx, y, ID_HEX, tr4(&T_HEX), HEX_BASE);
                chip(g, h, cx, y, ID_DEC, tr4(&T_DEC), !HEX_BASE);
            }
            Mode::EditBinary => text(g.hdc, g.f.caption, p.text3, x, y, tr4(&T_BIN_HINT)),
            Mode::Find => {
                let mut cx = x;
                for (k, t) in [&T_LOOK_KEYS, &T_LOOK_VALUES, &T_LOOK_DATA]
                    .iter()
                    .enumerate()
                {
                    cx += chip(g, h, cx, y, ID_LOOK + k as u16, tr4(t), LOOK[k]);
                }
            }
            Mode::Export => {
                let mut cx = x;
                cx += chip(
                    g,
                    h,
                    cx,
                    y,
                    ID_SCOPE_SEL,
                    tr4(&T_SELECTED_BRANCH),
                    !EXPORT_ALL,
                );
                chip(g, h, cx, y, ID_SCOPE_ALL, tr4(&T_ALL), EXPORT_ALL);
            }
            _ => {}
        }
    }
    let ok: &[u8] = match MODE {
        Mode::Find => tr4(&T_FIND_NEXT),
        Mode::Import => tr4(&T_IMPORT),
        Mode::Export => tr4(&T_EXPORT),
        _ => tr4(&T_OK),
    };
    ok_cancel(g, h, &d, ok, tr4(&T_CANCEL));
}

// ---------- Souris et clavier ----------

unsafe fn confirm() {
    match MODE {
        Mode::EditString | Mode::EditNumber | Mode::EditMulti | Mode::EditBinary => save_edit(),
        Mode::RenameKey | Mode::RenameValue => save_rename(),
        Mode::ConfirmDeleteKey | Mode::ConfirmDeleteValue => delete_selected(),
        Mode::Find => {
            MODE = Mode::Browse;
            if !EDIT.get().is_empty() {
                find_start();
            }
        }
        Mode::Import | Mode::ConfirmImport => do_import(),
        Mode::Export => do_export(),
        Mode::Browse => {}
    }
}

unsafe fn switch_base(hex: bool) {
    if HEX_BASE == hex {
        return;
    }
    if let Some(v) = parse_u64(EDIT.get(), HEX_BASE) {
        let mut s = Buf::<24>::new();
        if hex {
            let digits = 64 - v.leading_zeros().min(63) as usize;
            s.hex(v, digits.div_ceil(4));
        } else {
            s.dec(v);
        }
        EDIT.set(s.get());
    }
    HEX_BASE = hex;
}

unsafe fn command(id: u16) -> bool {
    match id {
        ID_NEW => MENU_OPEN = !MENU_OPEN && NODES[SEL].hive != 255,
        ID_EDIT => start_edit(VSEL),
        ID_RENAME => start_rename(),
        ID_DELETE => ask_delete(),
        ID_FIND => {
            EDIT.set(F_TEXT.get());
            MODE = Mode::Find;
            FRESH = true;
        }
        ID_IMPORT => {
            EDIT.set(b"C:\\Documents\\");
            MODE = Mode::Import;
        }
        ID_EXPORT => {
            default_export_path();
            MODE = Mode::Export;
        }
        ID_REFRESH => refresh_all(),
        _ => return false,
    }
    true
}

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, _f: &Frame) -> bool {
    if !READY {
        return false;
    }
    match ev {
        Mouse::Move => {
            HITS.mx = x;
            HITS.my = y;
            true
        }
        Mouse::Down | Mouse::RightDown => {
            let id = HITS.find(x, y);
            if MENU_OPEN {
                MENU_OPEN = false;
                if let Some(k @ ID_MENU..=26) = id {
                    new_item(k - ID_MENU);
                }
                return true;
            }
            if MODE != Mode::Browse {
                match id {
                    Some(ID_OK) => confirm(),
                    Some(ID_CANCEL) => MODE = Mode::Browse,
                    Some(ID_HEX) => switch_base(true),
                    Some(ID_DEC) => switch_base(false),
                    Some(k @ ID_LOOK..=47) => LOOK[(k - ID_LOOK) as usize] ^= true,
                    Some(ID_SCOPE_SEL) => {
                        EXPORT_ALL = false;
                        default_export_path();
                    }
                    Some(ID_SCOPE_ALL) => {
                        EXPORT_ALL = true;
                        default_export_path();
                    }
                    _ => return false,
                }
                return true;
            }
            ADDR_FOCUS = id == Some(ID_ADDR);
            match id {
                Some(k) if (ID_TOGGLE..ID_VAL).contains(&k) => {
                    let i = TREE_SCROLL + (k - ID_TOGGLE) as usize;
                    if i < NODE_COUNT {
                        if NODES[i].expanded {
                            collapse(i);
                        } else {
                            expand(i);
                        }
                    }
                    true
                }
                Some(k) if (ID_TREE..ID_TOGGLE).contains(&k) => {
                    FOCUS_VALUES = false;
                    select(TREE_SCROLL + (k - ID_TREE) as usize);
                    true
                }
                Some(k) if k >= ID_VAL => {
                    FOCUS_VALUES = true;
                    VSEL = (k - ID_VAL) as usize;
                    true
                }
                Some(ID_ADDR) => true,
                Some(k) => command(k),
                None => {
                    if inside(&LAST_VALUES, x, y) {
                        FOCUS_VALUES = true;
                        VSEL = usize::MAX;
                        return true;
                    }
                    false
                }
            }
        }
        Mouse::DoubleClick if MODE == Mode::Browse && !MENU_OPEN => match HITS.find(x, y) {
            Some(k) if (ID_TREE..ID_TOGGLE).contains(&k) => {
                let i = TREE_SCROLL + (k - ID_TREE) as usize;
                if i < NODE_COUNT {
                    if NODES[i].expanded {
                        collapse(i);
                    } else {
                        expand(i);
                    }
                }
                true
            }
            Some(k) if k >= ID_VAL => {
                start_edit((k - ID_VAL) as usize);
                true
            }
            _ => false,
        },
        Mouse::Wheel(delta) => {
            let (scroll, total) = if inside(&LAST_VALUES, x, y) {
                (&mut VAL_SCROLL, VALUE_COUNT)
            } else {
                (&mut TREE_SCROLL, NODE_COUNT)
            };
            *scroll = if delta > 0 {
                scroll.saturating_sub(3)
            } else {
                (*scroll + 3).min(total.saturating_sub(1))
            };
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if !READY {
        return false;
    }
    let ctrl = key_state(0x11) < 0;
    if MENU_OPEN {
        if ch == 27 {
            MENU_OPEN = false;
            return true;
        }
        return false;
    }
    if MODE != Mode::Browse {
        return match ch {
            27 => {
                MODE = Mode::Browse;
                true
            }
            13 if MODE == Mode::EditMulti && !ctrl => {
                EDIT.push(b"\n");
                true
            }
            13 => {
                confirm();
                true
            }
            0 => false,
            _ if matches!(
                MODE,
                Mode::ConfirmDeleteKey | Mode::ConfirmDeleteValue | Mode::ConfirmImport
            ) =>
            {
                false
            }
            _ => {
                if core::mem::take(&mut FRESH) && ch >= 32 {
                    EDIT.len = 0;
                }
                EDIT.key(ch)
            }
        };
    }
    if ADDR_FOCUS {
        return match ch {
            13 => {
                let mut copy = [0u8; PATH_MAX];
                let n = ADDR.len;
                copy[..n].copy_from_slice(ADDR.get());
                ADDR_FOCUS = false;
                if !navigate(&copy[..n]) {
                    sync_address();
                }
                true
            }
            27 => {
                ADDR_FOCUS = false;
                sync_address();
                true
            }
            0 => false,
            _ => ADDR.key(ch),
        };
    }
    if ctrl && (ch == 6 || vk == 0x46) {
        return command(ID_FIND);
    }
    // Entrée : le caractère (et non l'appui) ouvre l'éditeur, sinon le
    // caractère qui suit validerait aussitôt la boîte de dialogue.
    if ch == 13 && FOCUS_VALUES {
        start_edit(VSEL);
        return true;
    }
    match vk {
        0x72 => find_next(),    // F3
        0x71 => start_rename(), // F2
        0x74 => refresh_all(),  // F5
        0x2E => ask_delete(),   // Suppr
        0x09 => FOCUS_VALUES = !FOCUS_VALUES,
        0x26 | 0x28 => {
            let down = vk == 0x28;
            if FOCUS_VALUES {
                VSEL = match (VSEL, down) {
                    (usize::MAX, _) => 0,
                    (v, true) => (v + 1).min(VALUE_COUNT.saturating_sub(1)),
                    (v, false) => v.saturating_sub(1),
                };
            } else {
                let next = if down {
                    (SEL + 1).min(NODE_COUNT - 1)
                } else {
                    SEL.saturating_sub(1)
                };
                select(next);
                ensure_visible();
            }
        }
        0x27 if !FOCUS_VALUES => {
            if NODES[SEL].expanded
                && SEL + 1 < NODE_COUNT
                && NODES[SEL + 1].depth > NODES[SEL].depth
            {
                select(SEL + 1);
            } else {
                expand(SEL);
            }
            ensure_visible();
        }
        0x25 if !FOCUS_VALUES => {
            if NODES[SEL].expanded && NODES[SEL].hive != 255 {
                collapse(SEL);
            } else {
                select(parent_of(SEL));
            }
            ensure_visible();
        }
        _ => return false,
    }
    true
}

pub unsafe fn tick(_f: &Frame) -> bool {
    !MESSAGE.is_empty() && GetTickCount().wrapping_sub(MESSAGE_TICK) < 9000
}
