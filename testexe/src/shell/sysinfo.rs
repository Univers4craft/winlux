//! Informations système façon msinfo32 : résumé, carte mère et BIOS,
//! processeur, mémoire, stockage, affichage, réseau, son, périphériques,
//! pilotes (modules du noyau) et Winlux.  Tout est lu dans /proc et /sys
//! (et pci.ids s'il est installé) ; une catégorie sans donnée sur cette
//! machine est masquée.  Recherche dans toutes les catégories et export en
//! fichier texte.

use crate::admin_ui::{self as ui, Field, Hits, Path};
use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

const CAT_SUMMARY: u8 = 0;
const CAT_BOARD: u8 = 1;
const CAT_CPU: u8 = 2;
const CAT_MEMORY: u8 = 3;
const CAT_STORAGE: u8 = 4;
const CAT_DISPLAY: u8 = 5;
const CAT_NETWORK: u8 = 6;
const CAT_SOUND: u8 = 7;
const CAT_DEVICES: u8 = 8;
const CAT_DRIVERS: u8 = 9;
const CAT_WINLUX: u8 = 10;
const CAT_COUNT: usize = 11;

const CAT_NAMES: [Tr4; CAT_COUNT] = [
    [
        b"R\xE9sum\xE9 syst\xE8me",
        b"System summary",
        b"Resumen del sistema",
        b"System\xFCbersicht",
    ],
    [
        b"Carte m\xE8re et BIOS",
        b"Motherboard and BIOS",
        b"Placa base y BIOS",
        b"Mainboard und BIOS",
    ],
    [b"Processeur", b"Processor", b"Procesador", b"Prozessor"],
    [b"M\xE9moire", b"Memory", b"Memoria", b"Arbeitsspeicher"],
    [b"Stockage", b"Storage", b"Almacenamiento", b"Speicher"],
    [b"Affichage", b"Display", b"Pantalla", b"Anzeige"],
    [b"R\xE9seau", b"Network", b"Red", b"Netzwerk"],
    [b"Son", b"Sound", b"Sonido", b"Audio"],
    [
        b"P\xE9riph\xE9riques",
        b"Devices",
        b"Dispositivos",
        b"Ger\xE4te",
    ],
    [b"Pilotes", b"Drivers", b"Controladores", b"Treiber"],
    [b"Winlux", b"Winlux", b"Winlux", b"Winlux"],
];

const CAT_GLYPHS: [Glyph; CAT_COUNT] = [
    Glyph::Computer,
    Glyph::Tools,
    Glyph::Grid,
    Glyph::Details,
    Glyph::Drive,
    Glyph::Desktop,
    Glyph::Ethernet,
    Glyph::Audio,
    Glyph::Removable,
    Glyph::Code,
    Glyph::Start,
];

const MAX_ROWS: usize = 1200;
const MAX_KEY: usize = 64;
const MAX_VAL: usize = 200;

#[derive(Clone, Copy)]
struct Row {
    cat: u8,
    /// Titre de groupe (un disque, une carte réseau…), sans valeur.
    head: bool,
    key: [u8; MAX_KEY],
    kn: u8,
    val: [u8; MAX_VAL],
    vn: u8,
}

impl Row {
    fn key(&self) -> &[u8] {
        &self.key[..self.kn as usize]
    }
    fn val(&self) -> &[u8] {
        &self.val[..self.vn as usize]
    }
}

const EMPTY: Row = Row {
    cat: 0,
    head: false,
    key: [0; MAX_KEY],
    kn: 0,
    val: [0; MAX_VAL],
    vn: 0,
};

static mut ROWS: [Row; MAX_ROWS] = [EMPTY; MAX_ROWS];
static mut COUNT: usize = 0;
static mut COUNTS: [usize; CAT_COUNT] = [0; CAT_COUNT];
/// Lignes affichées : index dans ROWS, ou `SYNTH + cat` pour le titre de
/// catégorie inséré dans les résultats d'une recherche.
static mut VISIBLE: [u16; MAX_ROWS + CAT_COUNT] = [0; MAX_ROWS + CAT_COUNT];
static mut VISIBLE_COUNT: usize = 0;
const SYNTH: u16 = 60000;

static mut HITS: Hits = Hits::new();
static mut HOVER: u16 = 0;
static mut CAT: u8 = CAT_SUMMARY;
static mut SEARCH: Field<40> = Field::new();
static mut SEARCH_FOCUS: bool = false;
static mut SCROLL: usize = 0;
static mut ROWS_SHOWN: usize = 10;
static mut SELECTED: usize = usize::MAX;
static mut LOADED: bool = false;
static mut MESSAGE: Field<200> = Field::new();
static mut MESSAGE_AT: u32 = 0;

/// Tampon de lecture des fichiers (pci.ids fait environ 1,5 Mo).
static mut BUF: [u8; 2 * 1024 * 1024] = [0; 2 * 1024 * 1024];
/// Tampon des petites lectures (une valeur de /sys).
static mut SMALL: [u8; 8192] = [0; 8192];

const ID_CAT: u16 = 1;
const ID_SEARCH: u16 = 30;
const ID_REFRESH: u16 = 50;
const ID_EXPORT: u16 = 51;
const ID_ROW: u16 = 1000;

// ---------- Texte ----------

/// UTF-8 vers Windows-1252 (latin-1, guillemets et tirets typographiques).
fn to_1252(dst: &mut [u8], src: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < src.len() && n < dst.len() {
        let c = src[i];
        let (out, len) = if c < 0x80 {
            (if c < 0x20 { b' ' } else { c }, 1)
        } else if c & 0xE0 == 0xC0 && i + 1 < src.len() {
            let cp = ((c as u32 & 0x1F) << 6) | (src[i + 1] as u32 & 0x3F);
            (
                if (0xA0..=0xFF).contains(&cp) {
                    cp as u8
                } else {
                    b'?'
                },
                2,
            )
        } else if c & 0xF0 == 0xE0 && i + 2 < src.len() {
            let cp = ((c as u32 & 0x0F) << 12)
                | ((src[i + 1] as u32 & 0x3F) << 6)
                | (src[i + 2] as u32 & 0x3F);
            let b = match cp {
                0x2018 => 0x91,
                0x2019 => 0x92,
                0x201C => 0x93,
                0x201D => 0x94,
                0x2013 => 0x96,
                0x2014 => 0x97,
                0x2026 => 0x85,
                0x20AC => 0x80,
                0x2122 => 0x99,
                _ => b'?',
            };
            (b, 3)
        } else if c & 0xF8 == 0xF0 {
            (b'?', 4)
        } else {
            (c, 1)
        };
        dst[n] = out;
        n += 1;
        i += len;
    }
    n
}

/// Windows-1252 vers UTF-8 pour le fichier exporté.
fn push_utf8(out: &mut [u8], n: &mut usize, s: &[u8]) {
    for &c in s {
        let cp: u32 = match c {
            0x80 => 0x20AC,
            0x85 => 0x2026,
            0x91 => 0x2018,
            0x92 => 0x2019,
            0x93 => 0x201C,
            0x94 => 0x201D,
            0x96 => 0x2013,
            0x97 => 0x2014,
            0x99 => 0x2122,
            0x9C => 0x0153,
            0x8C => 0x0152,
            c => c as u32,
        };
        let mut buf = [0u8; 3];
        let len = if cp < 0x80 {
            buf[0] = cp as u8;
            1
        } else if cp < 0x800 {
            buf[0] = 0xC0 | (cp >> 6) as u8;
            buf[1] = 0x80 | (cp & 0x3F) as u8;
            2
        } else {
            buf[0] = 0xE0 | (cp >> 12) as u8;
            buf[1] = 0x80 | ((cp >> 6) & 0x3F) as u8;
            buf[2] = 0x80 | (cp & 0x3F) as u8;
            3
        };
        if *n + len <= out.len() {
            out[*n..*n + len].copy_from_slice(&buf[..len]);
            *n += len;
        }
    }
}

fn num(s: &[u8]) -> u64 {
    let mut v = 0u64;
    for &c in s {
        if !c.is_ascii_digit() {
            break;
        }
        v = v.saturating_mul(10).saturating_add((c - b'0') as u64);
    }
    v
}

fn hex(s: &[u8]) -> u64 {
    let mut v = 0u64;
    for &c in s {
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => break,
        };
        v = (v << 4) | d as u64;
    }
    v
}

fn trim(s: &[u8]) -> &[u8] {
    let a = s
        .iter()
        .position(|c| !c.is_ascii_whitespace() && *c != b'"')
        .unwrap_or(s.len());
    let b = s
        .iter()
        .rposition(|c| !c.is_ascii_whitespace() && *c != b'"')
        .map_or(a, |i| i + 1);
    &s[a..b]
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || hay
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle))
}

/// Valeur de `clé=valeur` (os-release, uevent, BUILD-INFO).
fn kv<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        if line.len() > key.len() && line.starts_with(key) && line[key.len()] == b'=' {
            return trim(&line[key.len() + 1..]);
        }
    }
    b""
}

/// Valeur de `clé<espaces/tabulations>: valeur` (cpuinfo, meminfo).
fn colon<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        if line.starts_with(key) {
            let rest = &line[key.len()..];
            let k = rest
                .iter()
                .position(|&c| c != b' ' && c != b'\t')
                .unwrap_or(rest.len());
            if rest.get(k) == Some(&b':') {
                return trim(&rest[k + 1..]);
            }
        }
    }
    b""
}

/// Valeur suspecte laissée par le fabricant (« To be filled by O.E.M. »).
fn placeholder(v: &[u8]) -> bool {
    v.is_empty()
        || [
            b"to be filled".as_slice(),
            b"default string",
            b"not specified",
            b"system product name",
            b"system manufacturer",
            b"o.e.m.",
            b"not applicable",
        ]
        .iter()
        .any(|p| contains_ci(v, p))
}

struct T {
    b: [u8; 240],
    n: usize,
}

impl T {
    const fn new() -> T {
        T { b: [0; 240], n: 0 }
    }
    fn s(&mut self, s: &[u8]) -> &mut T {
        let k = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s[..k]);
        self.n += k;
        self
    }
    /// Texte UTF-8 venant de Linux.
    fn l(&mut self, s: &[u8]) -> &mut T {
        let n = self.n;
        self.n += to_1252(&mut self.b[n..], s);
        self
    }
    fn u(&mut self, v: u64) -> &mut T {
        let mut d = [0u8; 20];
        let mut i = 20;
        let mut v = v;
        loop {
            i -= 1;
            d[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        self.s(&d[i..])
    }
    fn u2(&mut self, v: u64) -> &mut T {
        self.s(&[b'0' + (v / 10 % 10) as u8, b'0' + (v % 10) as u8])
    }
    fn hex2(&mut self, v: u8) -> &mut T {
        const H: &[u8; 16] = b"0123456789abcdef";
        self.s(&[H[(v >> 4) as usize], H[(v & 15) as usize]])
    }
    /// Taille en octets, unités binaires comme Windows (« 15,6 Go »).
    fn size(&mut self, bytes: u64) -> &mut T {
        const UNITS: [Tr4; 5] = [
            [b"octets", b"bytes", b"bytes", b"Byte"],
            [b"Ko", b"KB", b"KB", b"KB"],
            [b"Mo", b"MB", b"MB", b"MB"],
            [b"Go", b"GB", b"GB", b"GB"],
            [b"To", b"TB", b"TB", b"TB"],
        ];
        let mut unit = 0;
        let mut whole = bytes;
        let mut tenth = 0;
        while whole >= 1024 && unit < 4 {
            let next = bytes >> (10 * (unit + 1));
            tenth = (bytes >> (10 * unit)) % 1024 * 10 / 1024;
            whole = next;
            unit += 1;
        }
        self.u(whole);
        if unit > 0 && whole < 100 && tenth > 0 {
            self.s(if ui_lang() == 1 { b"." } else { b"," }).u(tenth);
        }
        self.s(b" ").s(tr4(&UNITS[unit]))
    }
    fn mhz(&mut self, mhz: u64) -> &mut T {
        if mhz >= 1000 {
            self.u(mhz / 1000)
                .s(if ui_lang() == 1 { b"." } else { b"," })
                .u2(mhz % 1000 / 10)
                .s(b" GHz")
        } else {
            self.u(mhz).s(b" MHz")
        }
    }
    fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
}

// ---------- Fichiers Linux ----------

fn zpath(linux: &[u8]) -> Path {
    let mut p = Path::new(b"Z:");
    for &c in linux {
        p.push(if c == b'/' {
            b"\\"
        } else {
            core::slice::from_ref(&c)
        });
    }
    p
}

fn join(parts: &[&[u8]]) -> T {
    let mut t = T::new();
    for p in parts {
        t.s(p);
    }
    t
}

/// Lit un petit fichier (valeur de /sys) dans SMALL.
unsafe fn small(parts: &[&[u8]]) -> &'static [u8] {
    let path = join(parts);
    let buf = &mut *core::ptr::addr_of_mut!(SMALL);
    let n = ui::read_file(zpath(path.get()).ptr(), &mut buf[..], false);
    trim(&buf[..n])
}

/// Copie d'une petite valeur, pour en lire une autre ensuite.
fn keep(s: &[u8]) -> ([u8; 128], usize) {
    let mut out = [0u8; 128];
    let n = s.len().min(128);
    out[..n].copy_from_slice(&s[..n]);
    (out, n)
}

/// Lit un fichier entier dans BUF.
unsafe fn big(path: &[u8]) -> &'static [u8] {
    let buf = &mut *core::ptr::addr_of_mut!(BUF);
    let n = ui::read_file(zpath(path).ptr(), &mut buf[..], false);
    &buf[..n]
}

unsafe fn exists(parts: &[&[u8]]) -> bool {
    ui::exists(zpath(join(parts).get()).ptr())
}

/// Noms des entrées d'un dossier, triés (au plus 64).
unsafe fn list_dir(dir: &[u8], out: &mut [[u8; 40]; 64], lens: &mut [u8; 64]) -> usize {
    let pattern = join(&[dir, b"/*"]);
    let mut fd: Win32FindDataA = core::mem::zeroed();
    let h = FindFirstFileA(zpath(pattern.get()).ptr(), &mut fd);
    if h.is_null() || h == -1isize as Handle {
        return 0;
    }
    let mut n = 0;
    loop {
        let name = &*(&fd.file_name as *const [i8; 260] as *const [u8; 260]);
        let len = name.iter().position(|&c| c == 0).unwrap_or(260);
        if len > 0 && len <= 40 && name[0] != b'.' && n < 64 {
            out[n][..len].copy_from_slice(&name[..len]);
            lens[n] = len as u8;
            n += 1;
        }
        if FindNextFileA(h, &mut fd) == 0 {
            break;
        }
    }
    FindClose(h);
    // Tri naturel simple (sda avant sdb, eth0 avant eth1).
    for i in 1..n {
        let mut j = i;
        while j > 0 && out[j - 1][..lens[j - 1] as usize] > out[j][..lens[j] as usize] {
            out.swap(j - 1, j);
            lens.swap(j - 1, j);
            j -= 1;
        }
    }
    n
}

// ---------- Lignes ----------

unsafe fn push(cat: u8, head: bool, key: &[u8], val1252: &[u8]) {
    if COUNT >= MAX_ROWS {
        return;
    }
    let mut r = EMPTY;
    r.cat = cat;
    r.head = head;
    let k = key.len().min(MAX_KEY);
    r.key[..k].copy_from_slice(&key[..k]);
    r.kn = k as u8;
    let v = val1252.len().min(MAX_VAL);
    r.val[..v].copy_from_slice(&val1252[..v]);
    r.vn = v as u8;
    ROWS[COUNT] = r;
    COUNT += 1;
    COUNTS[cat as usize] += 1;
}

/// Élément (texte traduit) et valeur déjà en Windows-1252.
unsafe fn add(cat: u8, key: &[u8], val: &[u8]) {
    if !val.is_empty() {
        push(cat, false, key, val);
    }
}

/// Élément et valeur venant de Linux (UTF-8).
unsafe fn add_l(cat: u8, key: &[u8], val: &[u8]) {
    let mut t = T::new();
    t.l(val);
    add(cat, key, t.get());
}

unsafe fn heading(cat: u8, title: &[u8]) {
    push(cat, true, title, b"");
}

// ---------- Collecte ----------

unsafe fn load() {
    COUNT = 0;
    COUNTS = [0; CAT_COUNT];
    load_summary();
    load_board();
    load_cpu();
    load_memory();
    load_storage();
    load_display();
    load_network();
    load_sound();
    load_devices();
    load_drivers();
    load_winlux();
    LOADED = true;
    if COUNTS[CAT as usize] == 0 {
        CAT = CAT_SUMMARY;
    }
    filter();
}

fn yes_no(v: bool) -> &'static [u8] {
    if v {
        tr4(&[b"Oui", b"Yes", b"S\xED", b"Ja"])
    } else {
        tr4(&[b"Non", b"No", b"No", b"Nein"])
    }
}

unsafe fn winlux_version() -> ([u8; 128], usize) {
    let v = small(&[b"/usr/share/winlux/version"]);
    if !v.is_empty() {
        return keep(v);
    }
    let info = small(&[b"/usr/share/winlux/BUILD-INFO"]);
    keep(kv(info, b"version"))
}

unsafe fn cpu_model() -> ([u8; 128], usize) {
    let data = big(b"/proc/cpuinfo");
    let m = colon(data, b"model name");
    keep(if m.is_empty() {
        colon(data, b"Hardware")
    } else {
        m
    })
}

unsafe fn load_summary() {
    let c = CAT_SUMMARY;
    let os = small(&[b"/etc/os-release"]);
    let name = kv(os, b"PRETTY_NAME");
    add_l(
        c,
        tr4(&[
            b"Syst\xE8me d'exploitation",
            b"Operating system",
            b"Sistema operativo",
            b"Betriebssystem",
        ]),
        if name.is_empty() {
            kv(os, b"NAME")
        } else {
            name
        },
    );
    let (v, n) = winlux_version();
    add_l(
        c,
        tr4(&[
            b"Version de Winlux",
            b"Winlux version",
            b"Versi\xF3n de Winlux",
            b"Winlux-Version",
        ]),
        &v[..n],
    );
    add_l(
        c,
        tr4(&[
            b"Noyau Linux",
            b"Linux kernel",
            b"Kernel Linux",
            b"Linux-Kernel",
        ]),
        small(&[b"/proc/sys/kernel/osrelease"]),
    );
    add_l(
        c,
        tr4(&[
            b"Nom de l'ordinateur",
            b"Computer name",
            b"Nombre del equipo",
            b"Computername",
        ]),
        small(&[b"/proc/sys/kernel/hostname"]),
    );
    let vendor = keep(small(&[b"/sys/class/dmi/id/sys_vendor"]));
    if !placeholder(&vendor.0[..vendor.1]) {
        add_l(
            c,
            tr4(&[
                b"Fabricant du syst\xE8me",
                b"System manufacturer",
                b"Fabricante del sistema",
                b"Systemhersteller",
            ]),
            &vendor.0[..vendor.1],
        );
    }
    let model = keep(small(&[b"/sys/class/dmi/id/product_name"]));
    if !placeholder(&model.0[..model.1]) {
        add_l(
            c,
            tr4(&[
                b"Mod\xE8le du syst\xE8me",
                b"System model",
                b"Modelo del sistema",
                b"Systemmodell",
            ]),
            &model.0[..model.1],
        );
    }
    add(
        c,
        tr4(&[
            b"Type du syst\xE8me",
            b"System type",
            b"Tipo de sistema",
            b"Systemtyp",
        ]),
        tr4(&[
            b"PC x64 (64 bits)",
            b"x64-based PC (64-bit)",
            b"PC basado en x64 (64 bits)",
            b"x64-basierter PC (64 Bit)",
        ]),
    );
    let (m, n) = cpu_model();
    add_l(
        c,
        tr4(&[b"Processeur", b"Processor", b"Procesador", b"Prozessor"]),
        &m[..n],
    );
    let mem = big(b"/proc/meminfo");
    let total = num(colon(mem, b"MemTotal")) * 1024;
    if total > 0 {
        let mut t = T::new();
        t.size(total);
        add(
            c,
            tr4(&[
                b"M\xE9moire physique (RAM)",
                b"Installed physical memory (RAM)",
                b"Memoria f\xEDsica (RAM)",
                b"Physischer Speicher (RAM)",
            ]),
            t.get(),
        );
    }
    crate::gpuinfo::refresh(true);
    if let Some(gpu) = crate::gpuinfo::get(0, b"name") {
        add_l(
            c,
            tr4(&[
                b"Carte graphique",
                b"Graphics card",
                b"Tarjeta gr\xE1fica",
                b"Grafikkarte",
            ]),
            gpu,
        );
    }
    let uefi = exists(&[b"/sys/firmware/efi"]);
    add(
        c,
        tr4(&[
            b"Mode du BIOS",
            b"BIOS mode",
            b"Modo de BIOS",
            b"BIOS-Modus",
        ]),
        if uefi {
            b"UEFI"
        } else {
            tr4(&[b"H\xE9rit\xE9", b"Legacy", b"Heredado", b"Legacy"])
        },
    );
    if uefi {
        let path = b"/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
        let buf = &mut *core::ptr::addr_of_mut!(SMALL);
        let n = ui::read_file(zpath(path).ptr(), &mut buf[..16], false);
        if n >= 5 {
            add(
                c,
                tr4(&[
                    b"\xC9tat du d\xE9marrage s\xE9curis\xE9",
                    b"Secure Boot state",
                    b"Estado de arranque seguro",
                    b"Sicherer Start",
                ]),
                if buf[4] == 1 {
                    tr4(&[b"Activ\xE9", b"On", b"Activado", b"Ein"])
                } else {
                    tr4(&[b"D\xE9sactiv\xE9", b"Off", b"Desactivado", b"Aus"])
                },
            );
        }
    }
    let flags = colon(big(b"/proc/cpuinfo"), b"flags");
    add(
        c,
        tr4(&[
            b"Machine virtuelle",
            b"Virtual machine",
            b"M\xE1quina virtual",
            b"Virtuelle Maschine",
        ]),
        yes_no(has_flag(flags, b"hypervisor")),
    );
    let mut user = [0u8; 64];
    let un = GetEnvironmentVariableA(c"USER".as_ptr(), user.as_mut_ptr().cast(), 64) as usize;
    if un > 0 && un < 64 {
        add_l(
            c,
            tr4(&[b"Utilisateur", b"User name", b"Usuario", b"Benutzername"]),
            &user[..un],
        );
    }
    add(
        c,
        tr4(&[
            b"Langue de l'interface",
            b"Display language",
            b"Idioma de la interfaz",
            b"Anzeigesprache",
        ]),
        [
            b"Fran\xE7ais".as_slice(),
            b"English",
            b"Espa\xF1ol",
            b"Deutsch",
        ][ui_lang().min(3)],
    );
    let tz = keep(small(&[b"/etc/timezone"]));
    add_l(
        c,
        tr4(&[
            b"Fuseau horaire",
            b"Time zone",
            b"Zona horaria",
            b"Zeitzone",
        ]),
        &tz.0[..tz.1],
    );
    let up = num(small(&[b"/proc/uptime"]));
    if up > 0 {
        let mut t = T::new();
        let (d, h, m) = (up / 86400, up / 3600 % 24, up / 60 % 60);
        if d > 0 {
            t.u(d).s(tr4(&[b" j ", b" d ", b" d ", b" T "]));
        }
        t.u(h).s(b" h ").u2(m).s(b" min");
        add(
            c,
            tr4(&[
                b"Allum\xE9 depuis",
                b"Up time",
                b"Tiempo encendido",
                b"Betriebszeit",
            ]),
            t.get(),
        );
    }
    let (w, h) = (GetSystemMetrics(0), GetSystemMetrics(1));
    if w > 0 && h > 0 {
        let mut t = T::new();
        t.u(w as u64).s(b" \xD7 ").u(h as u64);
        add(
            c,
            tr4(&[
                b"R\xE9solution de l'\xE9cran",
                b"Screen resolution",
                b"Resoluci\xF3n de pantalla",
                b"Bildschirmaufl\xF6sung",
            ]),
            t.get(),
        );
    }
}

fn has_flag(flags: &[u8], f: &[u8]) -> bool {
    flags.split(|&c| c == b' ').any(|w| w == f)
}

unsafe fn load_board() {
    let c = CAT_BOARD;
    let fields: [(&[u8], Tr4); 12] = [
        (
            b"sys_vendor",
            [
                b"Fabricant du syst\xE8me",
                b"System manufacturer",
                b"Fabricante del sistema",
                b"Systemhersteller",
            ],
        ),
        (
            b"product_name",
            [
                b"Mod\xE8le du syst\xE8me",
                b"System model",
                b"Modelo del sistema",
                b"Systemmodell",
            ],
        ),
        (
            b"product_version",
            [
                b"Version du syst\xE8me",
                b"System version",
                b"Versi\xF3n del sistema",
                b"Systemversion",
            ],
        ),
        (
            b"product_family",
            [b"Gamme", b"System family", b"Familia", b"Systemfamilie"],
        ),
        (
            b"board_vendor",
            [
                b"Fabricant de la carte m\xE8re",
                b"BaseBoard manufacturer",
                b"Fabricante de la placa base",
                b"Mainboard-Hersteller",
            ],
        ),
        (
            b"board_name",
            [
                b"Mod\xE8le de la carte m\xE8re",
                b"BaseBoard product",
                b"Modelo de la placa base",
                b"Mainboard-Modell",
            ],
        ),
        (
            b"board_version",
            [
                b"Version de la carte m\xE8re",
                b"BaseBoard version",
                b"Versi\xF3n de la placa base",
                b"Mainboard-Version",
            ],
        ),
        (
            b"bios_vendor",
            [
                b"Fabricant du BIOS",
                b"BIOS manufacturer",
                b"Fabricante del BIOS",
                b"BIOS-Hersteller",
            ],
        ),
        (
            b"bios_version",
            [
                b"Version du BIOS",
                b"BIOS version",
                b"Versi\xF3n del BIOS",
                b"BIOS-Version",
            ],
        ),
        (
            b"bios_date",
            [
                b"Date du BIOS",
                b"BIOS date",
                b"Fecha del BIOS",
                b"BIOS-Datum",
            ],
        ),
        (
            b"bios_release",
            [
                b"R\xE9vision du BIOS",
                b"BIOS release",
                b"Revisi\xF3n del BIOS",
                b"BIOS-Revision",
            ],
        ),
        (
            b"ec_firmware_release",
            [
                b"Micrologiciel du contr\xF4leur",
                b"Embedded controller version",
                b"Firmware del controlador",
                b"Controller-Firmware",
            ],
        ),
    ];
    for (file, label) in fields.iter() {
        let v = small(&[b"/sys/class/dmi/id/", file]);
        if !placeholder(v) {
            add_l(c, tr4(label), v);
        }
    }
    let chassis = num(small(&[b"/sys/class/dmi/id/chassis_type"]));
    let kind: Option<Tr4> = match chassis {
        3..=7 | 13 | 15 | 16 | 24 | 35 => Some([
            b"Ordinateur de bureau",
            b"Desktop",
            b"Equipo de escritorio",
            b"Desktop",
        ]),
        8..=10 | 14 | 31 | 32 => {
            Some([b"Ordinateur portable", b"Laptop", b"Port\xE1til", b"Laptop"])
        }
        30 => Some([b"Tablette", b"Tablet", b"Tableta", b"Tablet"]),
        17 | 23 | 25 | 28 | 29 => Some([b"Serveur", b"Server", b"Servidor", b"Server"]),
        1 | 2 if exists(&[b"/sys/class/dmi/id/sys_vendor"]) => Some([
            b"Machine virtuelle ou inconnu",
            b"Virtual machine or unknown",
            b"M\xE1quina virtual o desconocido",
            b"Virtuelle Maschine oder unbekannt",
        ]),
        _ => None,
    };
    if let Some(k) = kind {
        add(
            c,
            tr4(&[
                b"Type de bo\xEEtier",
                b"Chassis type",
                b"Tipo de chasis",
                b"Geh\xE4usetyp",
            ]),
            tr4(&k),
        );
    }
}

unsafe fn load_cpu() {
    let c = CAT_CPU;
    let data = big(b"/proc/cpuinfo");
    let mut logical = 0u64;
    let mut max_socket = 0u64;
    let mut seen_socket = false;
    for line in data.split(|&ch| ch == b'\n') {
        if line.starts_with(b"processor") {
            logical += 1;
        }
        if line.starts_with(b"physical id") {
            seen_socket = true;
            max_socket = max_socket.max(num(colon(line, b"physical id")));
        }
    }
    let sockets = if seen_socket { max_socket + 1 } else { 1 };
    let cores = num(colon(data, b"cpu cores")) * sockets;
    let model = keep(colon(data, b"model name"));
    let vendor_id = keep(colon(data, b"vendor_id"));
    let family = num(colon(data, b"cpu family"));
    let model_no = num(colon(data, b"model"));
    let stepping = num(colon(data, b"stepping"));
    let microcode = keep(colon(data, b"microcode"));
    let address = keep(colon(data, b"address sizes"));
    let mut flags_buf = [0u8; 2048];
    let fl = colon(data, b"flags");
    let fln = fl.len().min(2048);
    flags_buf[..fln].copy_from_slice(&fl[..fln]);
    let flags = &flags_buf[..fln];

    add_l(
        c,
        tr4(&[b"Nom", b"Name", b"Nombre", b"Name"]),
        &model.0[..model.1],
    );
    let vendor: &[u8] = match &vendor_id.0[..vendor_id.1] {
        b"GenuineIntel" => b"Intel",
        b"AuthenticAMD" => b"AMD",
        v => v,
    };
    add_l(
        c,
        tr4(&[b"Fabricant", b"Manufacturer", b"Fabricante", b"Hersteller"]),
        vendor,
    );
    if family > 0 {
        let mut t = T::new();
        t.s(tr4(&[b"Famille ", b"Family ", b"Familia ", b"Familie "]))
            .u(family)
            .s(tr4(&[
                b", mod\xE8le ",
                b", model ",
                b", modelo ",
                b", Modell ",
            ]))
            .u(model_no)
            .s(tr4(&[
                b", r\xE9vision ",
                b", stepping ",
                b", revisi\xF3n ",
                b", Stepping ",
            ]))
            .u(stepping);
        add(
            c,
            tr4(&[b"Identifiant", b"Identifier", b"Identificador", b"Kennung"]),
            t.get(),
        );
    }
    if sockets > 1 {
        let mut t = T::new();
        t.u(sockets);
        add(
            c,
            tr4(&[b"Sockets", b"Sockets", b"Z\xF3calos", b"Sockel"]),
            t.get(),
        );
    }
    if cores > 0 {
        let mut t = T::new();
        t.u(cores);
        add(
            c,
            tr4(&[b"C\x9Curs physiques", b"Cores", b"N\xFAcleos", b"Kerne"]),
            t.get(),
        );
    }
    if logical > 0 {
        let mut t = T::new();
        t.u(logical);
        add(
            c,
            tr4(&[
                b"Processeurs logiques",
                b"Logical processors",
                b"Procesadores l\xF3gicos",
                b"Logische Prozessoren",
            ]),
            t.get(),
        );
    }
    let base = num(small(&[
        b"/sys/devices/system/cpu/cpu0/cpufreq/base_frequency",
    ]));
    if base > 0 {
        let mut t = T::new();
        t.mhz(base / 1000);
        add(
            c,
            tr4(&[
                b"Fr\xE9quence de base",
                b"Base speed",
                b"Velocidad base",
                b"Basistakt",
            ]),
            t.get(),
        );
    }
    let max = num(small(&[
        b"/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq",
    ]));
    if max > 0 {
        let mut t = T::new();
        t.mhz(max / 1000);
        add(
            c,
            tr4(&[
                b"Fr\xE9quence maximale",
                b"Maximum speed",
                b"Velocidad m\xE1xima",
                b"Maximaler Takt",
            ]),
            t.get(),
        );
    } else {
        let cur = num(colon(big(b"/proc/cpuinfo"), b"cpu MHz"));
        if cur > 0 {
            let mut t = T::new();
            t.mhz(cur);
            add(
                c,
                tr4(&[b"Fr\xE9quence", b"Speed", b"Velocidad", b"Takt"]),
                t.get(),
            );
        }
    }
    // Caches
    for idx in 0..6u8 {
        let dir = join(&[
            b"/sys/devices/system/cpu/cpu0/cache/index",
            &[b'0' + idx],
            b"/",
        ]);
        let level = num(small(&[dir.get(), b"level"]));
        if level == 0 {
            continue;
        }
        let kind = keep(small(&[dir.get(), b"type"]));
        let size = small(&[dir.get(), b"size"]);
        let mut bytes = num(size);
        match size.last() {
            Some(b'K') => bytes *= 1024,
            Some(b'M') => bytes *= 1024 * 1024,
            _ => {}
        }
        let mut key = T::new();
        key.s(b"Cache L").u(level);
        match &kind.0[..kind.1] {
            b"Data" => {
                key.s(tr4(&[
                    b" (donn\xE9es)",
                    b" (data)",
                    b" (datos)",
                    b" (Daten)",
                ]));
            }
            b"Instruction" => {
                key.s(tr4(&[
                    b" (instructions)",
                    b" (instructions)",
                    b" (instrucciones)",
                    b" (Befehle)",
                ]));
            }
            _ => {}
        }
        let mut t = T::new();
        t.size(bytes);
        add(c, key.get(), t.get());
    }
    let virt: &[u8] = if has_flag(flags, b"vmx") {
        b"Intel VT-x"
    } else if has_flag(flags, b"svm") {
        b"AMD-V"
    } else {
        tr4(&[
            b"Non disponible",
            b"Not available",
            b"No disponible",
            b"Nicht verf\xFCgbar",
        ])
    };
    add(
        c,
        tr4(&[
            b"Virtualisation",
            b"Virtualization",
            b"Virtualizaci\xF3n",
            b"Virtualisierung",
        ]),
        virt,
    );
    let sets: [(&[u8], &[u8]); 12] = [
        (b"sse2", b"SSE2"),
        (b"pni", b"SSE3"),
        (b"ssse3", b"SSSE3"),
        (b"sse4_1", b"SSE4.1"),
        (b"sse4_2", b"SSE4.2"),
        (b"avx", b"AVX"),
        (b"avx2", b"AVX2"),
        (b"fma", b"FMA3"),
        (b"avx512f", b"AVX-512"),
        (b"aes", b"AES-NI"),
        (b"sha_ni", b"SHA"),
        (b"bmi2", b"BMI2"),
    ];
    let mut t = T::new();
    for (flag, name) in sets {
        if has_flag(flags, flag) {
            if t.n > 0 {
                t.s(b", ");
            }
            t.s(name);
        }
    }
    add(
        c,
        tr4(&[
            b"Jeux d'instructions",
            b"Instruction sets",
            b"Conjuntos de instrucciones",
            b"Befehlss\xE4tze",
        ]),
        t.get(),
    );
    add_l(
        c,
        tr4(&[b"Microcode", b"Microcode", b"Microc\xF3digo", b"Microcode"]),
        &microcode.0[..microcode.1],
    );
    add_l(
        c,
        tr4(&[
            b"Taille des adresses",
            b"Address sizes",
            b"Tama\xF1o de direcciones",
            b"Adressgr\xF6\xDFen",
        ]),
        &address.0[..address.1],
    );
}

unsafe fn load_memory() {
    let c = CAT_MEMORY;
    let mem = big(b"/proc/meminfo");
    let k = |key: &[u8]| num(colon(mem, key)) * 1024;
    let (total, avail) = (k(b"MemTotal"), k(b"MemAvailable"));
    let cached = k(b"Cached") + k(b"Buffers");
    let (swap, swap_free) = (k(b"SwapTotal"), k(b"SwapFree"));
    let rows: [(Tr4, u64); 5] = [
        (
            [
                b"M\xE9moire physique totale",
                b"Total physical memory",
                b"Memoria f\xEDsica total",
                b"Gesamter physischer Speicher",
            ],
            total,
        ),
        (
            [
                b"M\xE9moire physique disponible",
                b"Available physical memory",
                b"Memoria f\xEDsica disponible",
                b"Verf\xFCgbarer physischer Speicher",
            ],
            avail,
        ),
        (
            [
                b"M\xE9moire utilis\xE9e",
                b"Memory in use",
                b"Memoria en uso",
                b"Verwendeter Speicher",
            ],
            total.saturating_sub(avail),
        ),
        (
            [b"En cache", b"Cached", b"En cach\xE9", b"Im Cache"],
            cached,
        ),
        (
            [
                b"Fichier d'\xE9change (total)",
                b"Swap (total)",
                b"Intercambio (total)",
                b"Auslagerung (gesamt)",
            ],
            swap,
        ),
    ];
    for (label, v) in rows.iter() {
        if total > 0 && (*v > 0 || label[0] != rows[4].0[0]) {
            let mut t = T::new();
            t.size(*v);
            add(c, tr4(label), t.get());
        }
    }
    if swap > 0 {
        let mut t = T::new();
        t.size(swap_free);
        add(
            c,
            tr4(&[
                b"Fichier d'\xE9change (libre)",
                b"Swap (free)",
                b"Intercambio (libre)",
                b"Auslagerung (frei)",
            ]),
            t.get(),
        );
    }
    let zram = num(small(&[b"/sys/block/zram0/disksize"]));
    if zram > 0 {
        let mut t = T::new();
        t.size(zram);
        add(
            c,
            tr4(&[
                b"M\xE9moire compress\xE9e (zram)",
                b"Compressed memory (zram)",
                b"Memoria comprimida (zram)",
                b"Komprimierter Speicher (zram)",
            ]),
            t.get(),
        );
    }
}

unsafe fn load_storage() {
    let c = CAT_STORAGE;
    let mut names = [[0u8; 40]; 64];
    let mut lens = [0u8; 64];
    let n = list_dir(b"/sys/block", &mut names, &mut lens);
    // Points de montage (copie : BUF sert aussi aux autres lectures).
    let mut mounts = [0u8; 16384];
    let m = {
        let data = big(b"/proc/mounts");
        let k = data.len().min(mounts.len());
        mounts[..k].copy_from_slice(&data[..k]);
        k
    };
    let mounts = &mounts[..m];
    for i in 0..n {
        let name = &names[i][..lens[i] as usize];
        if [b"loop".as_slice(), b"ram", b"zram", b"dm-", b"md", b"nbd"]
            .iter()
            .any(|p| name.starts_with(p))
        {
            continue;
        }
        let sectors = num(small(&[b"/sys/block/", name, b"/size"]));
        if sectors == 0 && !name.starts_with(b"sr") {
            continue;
        }
        let model = keep(small(&[b"/sys/block/", name, b"/device/model"]));
        let vendor = keep(small(&[b"/sys/block/", name, b"/device/vendor"]));
        let mut title = T::new();
        if model.1 > 0 {
            if vendor.1 > 0 && !contains_ci(&model.0[..model.1], &vendor.0[..vendor.1]) {
                title.l(&vendor.0[..vendor.1]).s(b" ");
            }
            title.l(&model.0[..model.1]);
        } else {
            title.s(tr4(&[b"Disque", b"Disk", b"Disco", b"Datentr\xE4ger"]));
        }
        title.s(b" (").s(name).s(b")");
        heading(c, title.get());
        let rot = small(&[b"/sys/block/", name, b"/queue/rotational"]) == b"1";
        let removable = small(&[b"/sys/block/", name, b"/removable"]) == b"1";
        let kind: Tr4 = if name.starts_with(b"sr") {
            [
                b"Lecteur optique",
                b"Optical drive",
                b"Unidad \xF3ptica",
                b"Optisches Laufwerk",
            ]
        } else if name.starts_with(b"nvme") {
            [b"SSD NVMe", b"NVMe SSD", b"SSD NVMe", b"NVMe-SSD"]
        } else if name.starts_with(b"mmcblk") {
            [
                b"Carte m\xE9moire",
                b"Memory card",
                b"Tarjeta de memoria",
                b"Speicherkarte",
            ]
        } else if removable {
            [
                b"Amovible",
                b"Removable",
                b"Extra\xEDble",
                b"Wechseldatentr\xE4ger",
            ]
        } else if name.starts_with(b"vd") {
            [
                b"Disque virtuel",
                b"Virtual disk",
                b"Disco virtual",
                b"Virtueller Datentr\xE4ger",
            ]
        } else if rot {
            [
                b"Disque dur (HDD)",
                b"Hard disk (HDD)",
                b"Disco duro (HDD)",
                b"Festplatte (HDD)",
            ]
        } else {
            [b"SSD", b"SSD", b"SSD", b"SSD"]
        };
        add(c, tr4(&[b"Type", b"Type", b"Tipo", b"Typ"]), tr4(&kind));
        if sectors > 0 {
            let mut t = T::new();
            t.size(sectors * 512);
            add(
                c,
                tr4(&[b"Capacit\xE9", b"Capacity", b"Capacidad", b"Kapazit\xE4t"]),
                t.get(),
            );
        }
        let mut fw = keep(small(&[b"/sys/block/", name, b"/device/firmware_rev"]));
        if fw.1 == 0 {
            fw = keep(small(&[b"/sys/block/", name, b"/device/rev"]));
        }
        add_l(
            c,
            tr4(&[b"Micrologiciel", b"Firmware", b"Firmware", b"Firmware"]),
            &fw.0[..fw.1],
        );
        // Partitions
        let mut parts = [[0u8; 40]; 64];
        let mut plens = [0u8; 64];
        let dir = join(&[b"/sys/block/", name]);
        let pn = list_dir(dir.get(), &mut parts, &mut plens);
        for j in 0..pn {
            let part = &parts[j][..plens[j] as usize];
            if !part.starts_with(name) || !exists(&[dir.get(), b"/", part, b"/partition"]) {
                continue;
            }
            let size = num(small(&[dir.get(), b"/", part, b"/size"])) * 512;
            let mut key = T::new();
            key.s(tr4(&[
                b"Partition ",
                b"Partition ",
                b"Partici\xF3n ",
                b"Partition ",
            ]))
            .s(part);
            let mut t = T::new();
            t.size(size);
            let mut dev = T::new();
            dev.s(b"/dev/").s(part);
            for line in mounts.split(|&ch| ch == b'\n') {
                let mut f = line.split(|&ch| ch == b' ');
                if f.next() != Some(dev.get()) {
                    continue;
                }
                let point = f.next().unwrap_or(b"");
                let fs = f.next().unwrap_or(b"");
                t.s(b" \xB7 ").l(fs).s(b" \xB7 ");
                // Les espaces des points de montage sont écrits « \040 ».
                let mut k = 0;
                while k < point.len() {
                    if point[k] == b'\\' && point.len() >= k + 4 {
                        t.s(b" ");
                        k += 4;
                    } else {
                        t.l(&point[k..k + 1]);
                        k += 1;
                    }
                }
                break;
            }
            add(c, key.get(), t.get());
        }
    }
}

unsafe fn load_display() {
    let c = CAT_DISPLAY;
    crate::gpuinfo::refresh(true);
    for i in 0..crate::gpuinfo::count() {
        let name = crate::gpuinfo::get(i, b"name").unwrap_or(b"GPU");
        let mut t = T::new();
        t.l(name);
        heading(c, t.get());
        if let Some(v) = crate::gpuinfo::get(i, b"vendor") {
            let shown: &[u8] = match v {
                b"intel" => b"Intel",
                b"amd" => b"AMD",
                b"nvidia" => b"NVIDIA",
                b"virtio" => b"VirtIO",
                other => other,
            };
            add_l(
                c,
                tr4(&[b"Fabricant", b"Manufacturer", b"Fabricante", b"Hersteller"]),
                shown,
            );
        }
        if let Some(v) = crate::gpuinfo::get(i, b"driver") {
            add_l(
                c,
                tr4(&[b"Pilote", b"Driver", b"Controlador", b"Treiber"]),
                v,
            );
        }
        if let Some(v) = crate::gpuinfo::get(i, b"role") {
            let role: &[u8] = match v {
                b"integrated" => {
                    tr4(&[b"Int\xE9gr\xE9", b"Integrated", b"Integrada", b"Integriert"])
                }
                b"dedicated" => tr4(&[b"D\xE9di\xE9", b"Dedicated", b"Dedicada", b"Dediziert"]),
                _ => b"",
            };
            add(c, tr4(&[b"Type", b"Type", b"Tipo", b"Typ"]), role);
        }
        if let Some(mb) = crate::gpuinfo::num(i, b"vram_total_mb") {
            let mut t = T::new();
            t.size(mb as u64 * 1024 * 1024);
            add(
                c,
                tr4(&[
                    b"M\xE9moire vid\xE9o",
                    b"Video memory",
                    b"Memoria de v\xEDdeo",
                    b"Grafikspeicher",
                ]),
                t.get(),
            );
        }
        if let Some(v) = crate::gpuinfo::get(i, b"slot") {
            add_l(
                c,
                tr4(&[
                    b"Emplacement PCI",
                    b"PCI location",
                    b"Ubicaci\xF3n PCI",
                    b"PCI-Steckplatz",
                ]),
                v,
            );
        }
    }
    // Écrans branchés (connecteurs DRM « card0-HDMI-A-1 »)
    let mut names = [[0u8; 40]; 64];
    let mut lens = [0u8; 64];
    let n = list_dir(b"/sys/class/drm", &mut names, &mut lens);
    let mut first = true;
    for i in 0..n {
        let name = &names[i][..lens[i] as usize];
        let Some(dash) = name.iter().position(|&ch| ch == b'-') else {
            continue;
        };
        if small(&[b"/sys/class/drm/", name, b"/status"]) != b"connected" {
            continue;
        }
        if first {
            heading(
                c,
                tr4(&[b"\xC9crans", b"Monitors", b"Monitores", b"Bildschirme"]),
            );
            first = false;
        }
        let modes = small(&[b"/sys/class/drm/", name, b"/modes"]);
        let best = modes.split(|&ch| ch == b'\n').next().unwrap_or(b"");
        let mut key = T::new();
        key.s(&name[dash + 1..]);
        let mut t = T::new();
        t.l(best);
        if t.n == 0 {
            t.s(tr4(&[
                b"Branch\xE9",
                b"Connected",
                b"Conectado",
                b"Verbunden",
            ]));
        }
        add(c, key.get(), t.get());
    }
    let (w, h) = (GetSystemMetrics(0), GetSystemMetrics(1));
    if w > 0 && h > 0 {
        let mut t = T::new();
        t.u(w as u64).s(b" \xD7 ").u(h as u64);
        add(
            c,
            tr4(&[
                b"R\xE9solution du bureau",
                b"Desktop resolution",
                b"Resoluci\xF3n del escritorio",
                b"Desktopaufl\xF6sung",
            ]),
            t.get(),
        );
    }
}

fn ipv4(v: u32, t: &mut T) {
    // Valeurs de /proc/net/route : octets dans l'ordre de la mémoire.
    t.u((v & 0xFF) as u64)
        .s(b".")
        .u((v >> 8 & 0xFF) as u64)
        .s(b".")
        .u((v >> 16 & 0xFF) as u64)
        .s(b".")
        .u((v >> 24) as u64);
}

fn parse_ipv4(s: &[u8]) -> Option<u32> {
    let mut v = 0u32;
    let mut parts = 0;
    for (i, p) in s.split(|&c| c == b'.').enumerate() {
        if p.is_empty() || p.len() > 3 || !p.iter().all(u8::is_ascii_digit) || i > 3 {
            return None;
        }
        v |= (num(p) as u32 & 0xFF) << (8 * i);
        parts += 1;
    }
    (parts == 4).then_some(v)
}

/// Adresse IPv6 de /proc/net/if_inet6 (32 chiffres hexadécimaux),
/// abrégée comme `ip` (« fe80::1 »).
fn ipv6(h: &[u8], t: &mut T) {
    let mut g = [0u16; 8];
    for (i, w) in g.iter_mut().enumerate() {
        *w = hex(&h[i * 4..i * 4 + 4]) as u16;
    }
    // Plus longue suite de zéros (au moins deux groupes).
    let (mut best, mut best_len, mut k) = (8, 0, 0);
    while k < 8 {
        if g[k] == 0 {
            let s = k;
            while k < 8 && g[k] == 0 {
                k += 1;
            }
            if k - s > best_len && k - s >= 2 {
                best = s;
                best_len = k - s;
            }
        } else {
            k += 1;
        }
    }
    let mut i = 0;
    while i < 8 {
        if i == best {
            t.s(b"::");
            i += best_len;
            continue;
        }
        if i > 0 && i != best + best_len {
            t.s(b":");
        }
        let w = g[i];
        let mut started = false;
        for sh in [12, 8, 4, 0] {
            let d = (w >> sh & 15) as u8;
            if d != 0 || started || sh == 0 {
                started = true;
                t.s(&[b"0123456789abcdef"[d as usize]]);
            }
        }
        i += 1;
    }
}

unsafe fn load_network() {
    let c = CAT_NETWORK;
    // Adresses IPv4 locales (fib_trie) et routes, pour les rattacher.
    let mut locals = [0u32; 32];
    let mut nl = 0;
    {
        let data = big(b"/proc/net/fib_trie");
        let mut last = 0u32;
        for line in data.split(|&ch| ch == b'\n') {
            let l = trim(line);
            if let Some(p) = l.iter().position(|&ch| ch.is_ascii_digit()) {
                if l.starts_with(b"|--") || l.starts_with(b"+--") {
                    let addr = &l[p..];
                    let end = addr.iter().position(|&ch| ch == b' ').unwrap_or(addr.len());
                    last = parse_ipv4(&addr[..end]).unwrap_or(0);
                }
            }
            if contains_ci(l, b"/32 host LOCAL")
                && last & 0xFF != 127
                && last != 0
                && !locals[..nl].contains(&last)
                && nl < 32
            {
                locals[nl] = last;
                nl += 1;
            }
        }
    }
    let mut route_buf = [0u8; 8192];
    let route_len = {
        let data = big(b"/proc/net/route");
        let k = data.len().min(route_buf.len());
        route_buf[..k].copy_from_slice(&data[..k]);
        k
    };
    let route = &route_buf[..route_len];
    let mut inet6 = [0u8; 8192];
    let inet6_len = {
        let data = big(b"/proc/net/if_inet6");
        let k = data.len().min(inet6.len());
        inet6[..k].copy_from_slice(&data[..k]);
        k
    };
    let inet6 = &inet6[..inet6_len];

    let mut names = [[0u8; 40]; 64];
    let mut lens = [0u8; 64];
    let n = list_dir(b"/sys/class/net", &mut names, &mut lens);
    for i in 0..n {
        let name = &names[i][..lens[i] as usize];
        if name == b"lo" {
            continue;
        }
        heading(c, name);
        let wireless = exists(&[b"/sys/class/net/", name, b"/wireless"])
            || exists(&[b"/sys/class/net/", name, b"/phy80211"]);
        let physical = exists(&[b"/sys/class/net/", name, b"/device"]);
        add(
            c,
            tr4(&[b"Type", b"Type", b"Tipo", b"Typ"]),
            if wireless {
                b"Wi-Fi"
            } else if physical {
                b"Ethernet"
            } else {
                tr4(&[b"Virtuelle", b"Virtual", b"Virtual", b"Virtuell"])
            },
        );
        let state = keep(small(&[b"/sys/class/net/", name, b"/operstate"]));
        let st: &[u8] = match &state.0[..state.1] {
            b"up" => tr4(&[b"Connect\xE9", b"Connected", b"Conectado", b"Verbunden"]),
            b"down" | b"dormant" => tr4(&[
                b"D\xE9connect\xE9",
                b"Disconnected",
                b"Desconectado",
                b"Getrennt",
            ]),
            _ => tr4(&[b"Inconnu", b"Unknown", b"Desconocido", b"Unbekannt"]),
        };
        add(c, tr4(&[b"\xC9tat", b"Status", b"Estado", b"Status"]), st);
        if physical {
            let ue = small(&[b"/sys/class/net/", name, b"/device/uevent"]);
            let drv = keep(kv(ue, b"DRIVER"));
            add_l(
                c,
                tr4(&[b"Pilote", b"Driver", b"Controlador", b"Treiber"]),
                &drv.0[..drv.1],
            );
        }
        let mac = keep(small(&[b"/sys/class/net/", name, b"/address"]));
        if mac.1 > 0 && &mac.0[..mac.1] != b"00:00:00:00:00:00" {
            add_l(
                c,
                tr4(&[
                    b"Adresse physique (MAC)",
                    b"Physical address (MAC)",
                    b"Direcci\xF3n f\xEDsica (MAC)",
                    b"Physische Adresse (MAC)",
                ]),
                &mac.0[..mac.1],
            );
        }
        let speed = small(&[b"/sys/class/net/", name, b"/speed"]);
        if !speed.starts_with(b"-") && num(speed) > 0 {
            let mut t = T::new();
            let s = num(speed);
            if s >= 1000 && s % 1000 == 0 {
                t.u(s / 1000).s(b" Gb/s");
            } else {
                t.u(s).s(b" Mb/s");
            }
            add(
                c,
                tr4(&[b"Vitesse", b"Speed", b"Velocidad", b"Geschwindigkeit"]),
                t.get(),
            );
        }
        // IPv4 : adresses locales dans un réseau routé par cette interface.
        let mut v4 = T::new();
        for &ip in &locals[..nl] {
            let mut ours = false;
            for line in route.split(|&ch| ch == b'\n').skip(1) {
                let mut f = line.split(|&ch| ch == b'\t');
                if f.next() != Some(name) {
                    continue;
                }
                let dest = hex(f.next().unwrap_or(b"")) as u32;
                let mask = hex(f.nth(5).unwrap_or(b"")) as u32;
                if mask != 0 && ip & mask == dest {
                    ours = true;
                }
            }
            if ours {
                if v4.n > 0 {
                    v4.s(b", ");
                }
                ipv4(ip, &mut v4);
            }
        }
        add(
            c,
            tr4(&[
                b"Adresse IPv4",
                b"IPv4 address",
                b"Direcci\xF3n IPv4",
                b"IPv4-Adresse",
            ]),
            v4.get(),
        );
        let mut v6 = T::new();
        for line in inet6.split(|&ch| ch == b'\n') {
            let f: [&[u8]; 6] = {
                let mut it = line.split(|&ch| ch == b' ').filter(|w| !w.is_empty());
                [
                    it.next().unwrap_or(b""),
                    it.next().unwrap_or(b""),
                    it.next().unwrap_or(b""),
                    it.next().unwrap_or(b""),
                    it.next().unwrap_or(b""),
                    it.next().unwrap_or(b""),
                ]
            };
            if f[5] == name && f[0].len() == 32 {
                if v6.n > 0 {
                    v6.s(b", ");
                }
                ipv6(f[0], &mut v6);
            }
        }
        add(
            c,
            tr4(&[
                b"Adresse IPv6",
                b"IPv6 address",
                b"Direcci\xF3n IPv6",
                b"IPv6-Adresse",
            ]),
            v6.get(),
        );
        for line in route.split(|&ch| ch == b'\n').skip(1) {
            let mut f = line.split(|&ch| ch == b'\t');
            if f.next() != Some(name) {
                continue;
            }
            let dest = hex(f.next().unwrap_or(b"x"));
            let gw = hex(f.next().unwrap_or(b"")) as u32;
            if dest == 0 && gw != 0 {
                let mut t = T::new();
                ipv4(gw, &mut t);
                add(
                    c,
                    tr4(&[
                        b"Passerelle par d\xE9faut",
                        b"Default gateway",
                        b"Puerta de enlace",
                        b"Standardgateway",
                    ]),
                    t.get(),
                );
                break;
            }
        }
        let mtu = num(small(&[b"/sys/class/net/", name, b"/mtu"]));
        if mtu > 0 {
            let mut t = T::new();
            t.u(mtu);
            add(c, b"MTU", t.get());
        }
    }
    // Serveurs DNS
    let resolv = small(&[b"/etc/resolv.conf"]);
    let mut dns = T::new();
    for line in resolv.split(|&ch| ch == b'\n') {
        if let Some(rest) = line.strip_prefix(b"nameserver") {
            if dns.n > 0 {
                dns.s(b", ");
            }
            dns.l(trim(rest));
        }
    }
    if dns.n > 0 {
        heading(
            c,
            tr4(&[b"G\xE9n\xE9ral", b"General", b"General", b"Allgemein"]),
        );
        add(
            c,
            tr4(&[
                b"Serveurs DNS",
                b"DNS servers",
                b"Servidores DNS",
                b"DNS-Server",
            ]),
            dns.get(),
        );
    }
}

unsafe fn load_sound() {
    let c = CAT_SOUND;
    let data = small(&[b"/proc/asound/cards"]);
    let mut copy = [0u8; 4096];
    let k = data.len().min(4096);
    copy[..k].copy_from_slice(&data[..k]);
    for line in copy[..k].split(|&ch| ch == b'\n') {
        let l = trim(line);
        let Some(p) = l.windows(3).position(|w| w == b"]: ") else {
            continue;
        };
        let index = num(l);
        let rest = &l[p + 3..];
        let (driver, name) = match rest.windows(3).position(|w| w == b" - ") {
            Some(d) => (&rest[..d], &rest[d + 3..]),
            None => (&b""[..], rest),
        };
        let mut key = T::new();
        key.s(tr4(&[
            b"Carte son ",
            b"Sound card ",
            b"Tarjeta de sonido ",
            b"Soundkarte ",
        ]))
        .u(index);
        let mut t = T::new();
        t.l(name);
        if !driver.is_empty() {
            t.s(b" (").l(driver).s(b")");
        }
        add(c, key.get(), t.get());
    }
}

/// Nom d'un périphérique PCI dans pci.ids (« Intel Corporation Alder Lake… »).
fn pci_name(ids: &[u8], vendor: &[u8], device: &[u8], t: &mut T) -> bool {
    let mut in_vendor = false;
    let mut found_vendor = false;
    for line in ids.split(|&c| c == b'\n') {
        if line.is_empty() || line[0] == b'#' {
            continue;
        }
        if line[0] != b'\t' {
            if in_vendor {
                break;
            }
            if line.len() > 6 && line[..4].eq_ignore_ascii_case(vendor) && line[4] == b' ' {
                in_vendor = true;
                found_vendor = true;
                t.l(trim(&line[6..]));
            }
            continue;
        }
        if in_vendor
            && line.len() > 7
            && line[1] != b'\t'
            && line[1..5].eq_ignore_ascii_case(device)
            && line[5] == b' '
        {
            t.s(b" ").l(trim(&line[7..]));
            return true;
        }
    }
    found_vendor
}

unsafe fn load_devices() {
    let c = CAT_DEVICES;
    // pci.ids est facultatif : sans lui, les identifiants restent affichés.
    let mut ids_len = 0;
    for p in [
        b"/usr/share/misc/pci.ids".as_slice(),
        b"/usr/share/hwdata/pci.ids",
        b"/usr/share/pci.ids",
    ] {
        ids_len = big(p).len();
        if ids_len > 0 {
            break;
        }
    }
    let ids = &(&*core::ptr::addr_of!(BUF))[..ids_len];
    let mut names = [[0u8; 40]; 64];
    let mut lens = [0u8; 64];
    let n = list_dir(b"/sys/bus/pci/devices", &mut names, &mut lens);
    if n > 0 {
        heading(c, b"PCI");
    }
    for i in 0..n {
        let name = &names[i][..lens[i] as usize];
        let ue = small(&[b"/sys/bus/pci/devices/", name, b"/uevent"]);
        let class = hex(kv(ue, b"PCI_CLASS")) as u32;
        let id = keep(kv(ue, b"PCI_ID"));
        let drv = keep(kv(ue, b"DRIVER"));
        let id = &id.0[..id.1];
        let kind: Tr4 = match (class >> 16, class >> 8 & 0xFF) {
            (0x01, _) => [
                b"Contr\xF4leur de stockage",
                b"Storage controller",
                b"Controlador de almacenamiento",
                b"Speichercontroller",
            ],
            (0x02, _) => [
                b"Carte r\xE9seau",
                b"Network adapter",
                b"Adaptador de red",
                b"Netzwerkadapter",
            ],
            (0x03, _) => [
                b"Carte graphique",
                b"Display adapter",
                b"Adaptador de pantalla",
                b"Grafikkarte",
            ],
            (0x04, _) => [
                b"Contr\xF4leur multim\xE9dia",
                b"Multimedia controller",
                b"Controlador multimedia",
                b"Multimediacontroller",
            ],
            (0x06, _) => [b"Pont", b"Bridge", b"Puente", b"Bridge"],
            (0x0C, 0x03) => [
                b"Contr\xF4leur USB",
                b"USB controller",
                b"Controlador USB",
                b"USB-Controller",
            ],
            (0x0C, _) => [
                b"Contr\xF4leur de bus",
                b"Bus controller",
                b"Controlador de bus",
                b"Buscontroller",
            ],
            (0x0D, _) => [
                b"Contr\xF4leur sans fil",
                b"Wireless controller",
                b"Controlador inal\xE1mbrico",
                b"Drahtloscontroller",
            ],
            (0x08, _) => [
                b"P\xE9riph\xE9rique syst\xE8me",
                b"System device",
                b"Dispositivo del sistema",
                b"Systemger\xE4t",
            ],
            (0x05, _) => [b"M\xE9moire", b"Memory", b"Memoria", b"Speicher"],
            (0x10, _) => [
                b"Chiffrement",
                b"Encryption",
                b"Cifrado",
                b"Verschl\xFCsselung",
            ],
            (0x11, _) => [
                b"Acquisition de donn\xE9es",
                b"Signal processing",
                b"Procesamiento de se\xF1al",
                b"Signalverarbeitung",
            ],
            _ => [
                b"Autre p\xE9riph\xE9rique",
                b"Other device",
                b"Otro dispositivo",
                b"Anderes Ger\xE4t",
            ],
        };
        let mut t = T::new();
        let named = id.len() == 9 && !ids.is_empty() && pci_name(ids, &id[..4], &id[5..9], &mut t);
        if !named {
            t.s(id);
        }
        if drv.1 > 0 {
            t.s(tr4(&[
                b" \x97 pilote ",
                b" \x97 driver ",
                b" \x97 controlador ",
                b" \x97 Treiber ",
            ]))
            .l(&drv.0[..drv.1]);
        }
        add(c, tr4(&kind), t.get());
    }
    // USB
    let n = list_dir(b"/sys/bus/usb/devices", &mut names, &mut lens);
    let mut first = true;
    for i in 0..n {
        let name = &names[i][..lens[i] as usize];
        if name.contains(&b':') || name.starts_with(b"usb") {
            continue;
        }
        let vid = keep(small(&[b"/sys/bus/usb/devices/", name, b"/idVendor"]));
        if vid.1 == 0 {
            continue;
        }
        let pid = keep(small(&[b"/sys/bus/usb/devices/", name, b"/idProduct"]));
        let maker = keep(small(&[b"/sys/bus/usb/devices/", name, b"/manufacturer"]));
        let product = keep(small(&[b"/sys/bus/usb/devices/", name, b"/product"]));
        if first {
            heading(c, b"USB");
            first = false;
        }
        let mut key = T::new();
        if product.1 > 0 {
            key.l(&product.0[..product.1]);
        } else {
            key.s(tr4(&[
                b"P\xE9riph\xE9rique USB",
                b"USB device",
                b"Dispositivo USB",
                b"USB-Ger\xE4t",
            ]));
        }
        let mut t = T::new();
        if maker.1 > 0 {
            t.l(&maker.0[..maker.1]).s(b" ");
        }
        t.s(b"(")
            .l(&vid.0[..vid.1])
            .s(b":")
            .l(&pid.0[..pid.1])
            .s(b")");
        let speed = num(small(&[b"/sys/bus/usb/devices/", name, b"/speed"]));
        if speed > 0 {
            t.s(b" \xB7 ");
            if speed >= 1000 {
                t.u(speed / 1000).s(b" Gb/s");
            } else {
                t.u(speed).s(b" Mb/s");
            }
        }
        add(c, key.get(), t.get());
    }
}

unsafe fn load_drivers() {
    let c = CAT_DRIVERS;
    let data = big(b"/proc/modules");
    let start = COUNT;
    let mut loaded = 0u64;
    for line in data.split(|&ch| ch == b'\n') {
        let mut f = line.split(|&ch| ch == b' ');
        let name = f.next().unwrap_or(b"");
        if name.is_empty() {
            continue;
        }
        let size = num(f.next().unwrap_or(b""));
        let users = num(f.next().unwrap_or(b""));
        let mut t = T::new();
        t.size(size).s(b" \xB7 ");
        t.s(tr4(&[
            b"utilis\xE9 par ",
            b"used by ",
            b"usado por ",
            b"verwendet von ",
        ]))
        .u(users);
        let mut key = T::new();
        key.l(name);
        add(c, key.get(), t.get());
        loaded += 1;
    }
    // Ordre alphabétique, comme la liste des pilotes de Windows.
    let rows = &mut *core::ptr::addr_of_mut!(ROWS);
    rows[start..COUNT].sort_unstable_by(|a, b| a.key().cmp(b.key()));
    if loaded == 0 {
        return;
    }
    // Total en tête de la catégorie.
    let mut t = T::new();
    t.u(loaded);
    push(
        c,
        false,
        tr4(&[
            b"Modules du noyau charg\xE9s",
            b"Loaded kernel modules",
            b"M\xF3dulos del kernel cargados",
            b"Geladene Kernelmodule",
        ]),
        t.get(),
    );
    rows[start..COUNT].rotate_right(1);
}

unsafe fn load_winlux() {
    let c = CAT_WINLUX;
    let (v, n) = winlux_version();
    add_l(
        c,
        tr4(&[b"Version", b"Version", b"Versi\xF3n", b"Version"]),
        &v[..n],
    );
    let info = small(&[b"/usr/share/winlux/BUILD-INFO"]);
    let mut copy = [0u8; 1024];
    let k = info.len().min(1024);
    copy[..k].copy_from_slice(&info[..k]);
    let info = &copy[..k];
    let commit = kv(info, b"commit");
    if commit.len() >= 12 {
        add_l(
            c,
            tr4(&[b"R\xE9vision", b"Revision", b"Revisi\xF3n", b"Revision"]),
            &commit[..12],
        );
    }
    let built = kv(info, b"built_at_utc");
    if built.len() >= 10 {
        let mut t = T::new();
        t.l(&built[..10]);
        add(
            c,
            tr4(&[
                b"Date de construction",
                b"Build date",
                b"Fecha de compilaci\xF3n",
                b"Erstellungsdatum",
            ]),
            t.get(),
        );
    }
    add(
        c,
        tr4(&[
            b"Couche de compatibilit\xE9 .exe",
            b".exe compatibility layer",
            b"Capa de compatibilidad .exe",
            b".exe-Kompatibilit\xE4tsschicht",
        ]),
        tr4(&[
            b"Winlux (maison, sans Wine)",
            b"Winlux (in-house, no Wine)",
            b"Winlux (propia, sin Wine)",
            b"Winlux (eigen, ohne Wine)",
        ]),
    );
    let native =
        GetEnvironmentVariableA(c"WINLUX_NATIVE_SHELL".as_ptr(), core::ptr::null_mut(), 0) > 0;
    add(
        c,
        tr4(&[b"Session", b"Session", b"Sesi\xF3n", b"Sitzung"]),
        if native {
            tr4(&[
                b"Bureau Linux natif",
                b"Native Linux desktop",
                b"Escritorio Linux nativo",
                b"Nativer Linux-Desktop",
            ])
        } else {
            tr4(&[
                b"Bureau Winlux int\xE9gr\xE9",
                b"Built-in Winlux desktop",
                b"Escritorio Winlux integrado",
                b"Integrierter Winlux-Desktop",
            ])
        },
    );
    let crashes = small(&[b"/run/winlux/journal/crash.log"]);
    let count = crashes
        .split(|&ch| ch == b'\n')
        .filter(|l| l.starts_with(b"[CRASH]"))
        .count();
    let mut t = T::new();
    t.u(count as u64);
    add(
        c,
        tr4(&[
            b"Plantages depuis le d\xE9marrage",
            b"Crashes since startup",
            b"Bloqueos desde el inicio",
            b"Abst\xFCrze seit dem Start",
        ]),
        t.get(),
    );
    add(
        c,
        tr4(&[b"Journaux", b"Logs", b"Registros", b"Protokolle"]),
        b"/run/winlux/journal",
    );
}

// ---------- Filtre ----------

unsafe fn filter() {
    VISIBLE_COUNT = 0;
    let rows = &*core::ptr::addr_of!(ROWS);
    let q = (*core::ptr::addr_of!(SEARCH)).get();
    if q.is_empty() {
        for (i, r) in rows[..COUNT].iter().enumerate() {
            if r.cat == CAT {
                VISIBLE[VISIBLE_COUNT] = i as u16;
                VISIBLE_COUNT += 1;
            }
        }
    } else {
        // Toutes les catégories, chacune sous son titre.
        for cat in 0..CAT_COUNT as u8 {
            let mut titled = false;
            let mut group = usize::MAX;
            for (i, r) in rows[..COUNT].iter().enumerate() {
                if r.cat != cat {
                    continue;
                }
                if r.head {
                    group = i;
                }
                let hit = contains_ci(r.key(), q) || contains_ci(r.val(), q);
                if !hit {
                    continue;
                }
                if !titled {
                    VISIBLE[VISIBLE_COUNT] = SYNTH + cat as u16;
                    VISIBLE_COUNT += 1;
                    titled = true;
                }
                // Le titre du groupe (le disque, la carte) reste visible.
                if group != usize::MAX && group != i && !r.head {
                    VISIBLE[VISIBLE_COUNT] = group as u16;
                    VISIBLE_COUNT += 1;
                    group = usize::MAX;
                } else if r.head {
                    group = usize::MAX;
                }
                VISIBLE[VISIBLE_COUNT] = i as u16;
                VISIBLE_COUNT += 1;
            }
        }
    }
    SCROLL = SCROLL.min(VISIBLE_COUNT.saturating_sub(1));
    if SELECTED >= VISIBLE_COUNT {
        SELECTED = usize::MAX;
    }
}

unsafe fn say(s: &[u8]) {
    MESSAGE.set(s);
    MESSAGE_AT = GetTickCount();
}

// ---------- Dessin ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    if !LOADED {
        load();
    }
    let h = &mut *core::ptr::addr_of_mut!(HITS);
    h.clear();
    let r = f.client;
    let p = pal();
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.mica);

    // Catégories à gauche (celles sans donnée sont masquées)
    let nav_w = 240.min((r.right - r.left) / 4);
    let searching = !(*core::ptr::addr_of!(SEARCH)).get().is_empty();
    let mut y = r.top + 14;
    for i in 0..CAT_COUNT {
        if COUNTS[i] == 0 {
            continue;
        }
        ui::nav_item(
            g,
            h,
            r.left,
            y,
            nav_w,
            ID_CAT + i as u16,
            tr4(&CAT_NAMES[i]),
            CAT_GLYPHS[i],
            !searching && CAT as usize == i,
        );
        y += 40;
    }

    // Contenu
    let cx = r.left + nav_w;
    fill(g.hdc, g.b, cx, r.top, r.right, r.bottom, p.window);
    fill(g.hdc, g.b, cx, r.top, cx + 1, r.bottom, p.border);
    let x = cx + 20;
    let right = r.right - 16;
    let title: &[u8] = if searching {
        tr4(&[
            b"R\xE9sultats de la recherche",
            b"Search results",
            b"Resultados de la b\xFAsqueda",
            b"Suchergebnisse",
        ])
    } else {
        tr4(&CAT_NAMES[CAT as usize])
    };
    text(g.hdc, g.f.large, p.text, x, r.top + 14, title);

    let mut bx = right;
    for (id, label) in [
        (
            ID_EXPORT,
            tr4(&[b"Exporter", b"Export", b"Exportar", b"Exportieren"]),
        ),
        (
            ID_REFRESH,
            tr4(&[b"Actualiser", b"Refresh", b"Actualizar", b"Aktualisieren"]),
        ),
    ] {
        let w = ui::button_w(label);
        ui::button(
            g,
            h,
            rect(bx - w, r.top + 12, bx, r.top + 44),
            id,
            label,
            false,
            true,
        );
        bx -= w + 8;
    }

    let fy = r.top + 58;
    let sw = 300.min(right - x);
    ui::input(
        g,
        h,
        rect(x, fy, x + sw, fy + 30),
        ID_SEARCH,
        (*core::ptr::addr_of!(SEARCH)).get(),
        SEARCH_FOCUS,
        tr4(&[
            b"Rechercher dans toutes les cat\xE9gories",
            b"Search all categories",
            b"Buscar en todas las categor\xEDas",
            b"Alle Kategorien durchsuchen",
        ]),
    );

    let ty = fy + 44;
    let w = right - x;
    let cols: [(&[u8], i32); 2] = [
        (
            tr4(&[b"\xC9l\xE9ment", b"Item", b"Elemento", b"Element"]),
            w * 36 / 100,
        ),
        (tr4(&[b"Valeur", b"Value", b"Valor", b"Wert"]), w * 64 / 100),
    ];
    let top = ui::header(g, x, ty, right, &cols);
    let row_h = 28;
    let bottom = r.bottom - 12;
    ROWS_SHOWN = ((bottom - top) / row_h).max(1) as usize;
    let rows = &*core::ptr::addr_of!(ROWS);
    let vis = &*core::ptr::addr_of!(VISIBLE);
    let mut ry = top;
    for (slot, k) in (SCROLL..VISIBLE_COUNT).take(ROWS_SHOWN).enumerate() {
        let idx = vis[k];
        let rr = rect(x - 8, ry, right, ry + row_h);
        if idx >= SYNTH {
            text(
                g.hdc,
                g.f.semibold,
                p.accent,
                x,
                ry + 8,
                tr4(&CAT_NAMES[(idx - SYNTH) as usize]),
            );
            ry += row_h;
            continue;
        }
        let row = &rows[idx as usize];
        let hover = h.add(rr, ID_ROW + slot as u16);
        if row.head {
            fill(
                g.hdc,
                g.b,
                rr.left,
                rr.bottom - 1,
                rr.right,
                rr.bottom,
                p.border,
            );
            text_fit(
                g.hdc,
                g.f.semibold,
                13,
                p.text,
                x,
                ry + 7,
                row.key(),
                right - x,
            );
            ry += row_h;
            continue;
        }
        if SELECTED == k {
            fill(
                g.hdc,
                g.b,
                rr.left,
                rr.top,
                rr.right,
                rr.bottom,
                mix(p.selected, p.window, 3, 10),
            );
            fill(
                g.hdc,
                g.b,
                rr.left,
                rr.top,
                rr.left + 3,
                rr.bottom,
                p.accent,
            );
        } else if hover {
            fill(g.hdc, g.b, rr.left, rr.top, rr.right, rr.bottom, p.hover);
        }
        ui::cell(g, x, ry + 6, cols[0].1, row.key(), p.text2);
        ui::cell(
            g,
            x + cols[0].1,
            ry + 6,
            right - x - cols[0].1,
            row.val(),
            p.text,
        );
        ry += row_h;
    }
    if VISIBLE_COUNT == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text2,
            x,
            top + 12,
            tr4(&[
                b"Aucun r\xE9sultat.",
                b"No results.",
                b"Sin resultados.",
                b"Keine Ergebnisse.",
            ]),
        );
    }
    if VISIBLE_COUNT > ROWS_SHOWN {
        let hgt = bottom - top;
        let thumb = ((hgt as usize * ROWS_SHOWN / VISIBLE_COUNT) as i32).max(24);
        let ty2 =
            top + ((hgt - thumb) as usize * SCROLL / (VISIBLE_COUNT - ROWS_SHOWN).max(1)) as i32;
        round_rect(
            g.hdc,
            g.b,
            right + 6,
            ty2,
            right + 10,
            ty2 + thumb,
            2,
            p.text3,
        );
    }

    // Valeur complète de la ligne choisie, si elle est coupée à l'écran.
    if SELECTED < VISIBLE_COUNT && vis[SELECTED] < SYNTH {
        let row = &rows[vis[SELECTED] as usize];
        if text_width(row.val(), 13) > right - x - cols[0].1 - 8 {
            let tw = (text_width(row.val(), 13) + 32).min(right - x);
            let ty3 = bottom - 46;
            panel(
                g.hdc,
                g.b,
                right - tw,
                ty3,
                right,
                ty3 + 36,
                6,
                p.surface2,
                p.border,
            );
            text_fit(
                g.hdc,
                g.f.body,
                13,
                p.text,
                right - tw + 16,
                ty3 + 10,
                row.val(),
                tw - 32,
            );
        }
    }

    let m = (*core::ptr::addr_of!(MESSAGE)).get();
    if !m.is_empty() && GetTickCount().wrapping_sub(MESSAGE_AT) < 7000 {
        let mw = (text_width(m, 13) + 32).min(right - x);
        let mx = x + (right - x - mw) / 2;
        let my = bottom - 46;
        panel(
            g.hdc,
            g.b,
            mx,
            my,
            mx + mw,
            my + 36,
            6,
            p.surface2,
            p.border,
        );
        text_fit(g.hdc, g.f.body, 13, p.text, mx + 16, my + 10, m, mw - 32);
    }
}

// ---------- Export ----------

/// Écrit toutes les catégories dans un fichier texte (UTF-8) de Documents,
/// sinon du dossier personnel.
unsafe fn export() {
    let mut home = [0u8; 200];
    let n = GetEnvironmentVariableA(c"HOME".as_ptr(), home.as_mut_ptr().cast(), 200) as usize;
    let home = if n == 0 || n >= 200 {
        &b"/root"[..]
    } else {
        &home[..n]
    };
    let mut dir = zpath(home);
    let mut docs = zpath(home);
    docs.push(b"\\Documents");
    if ui::exists(docs.ptr()) {
        dir = docs;
    }
    let mut lt: SystemTime = core::mem::zeroed();
    GetLocalTime(&mut lt);
    let mut name = T::new();
    name.s(b"\\")
        .s(tr4(&[
            b"informations-systeme-",
            b"system-information-",
            b"informacion-del-sistema-",
            b"systeminformationen-",
        ]))
        .u(lt.year as u64)
        .u2(lt.month as u64)
        .u2(lt.day as u64)
        .s(b"-")
        .u2(lt.hour as u64)
        .u2(lt.minute as u64)
        .u2(lt.second as u64)
        .s(b".txt");
    dir.push(name.get());
    let out = &mut *core::ptr::addr_of_mut!(BUF);
    let mut k = 0usize;
    let mut head = T::new();
    head.s(tr4(&[
        b"Informations syst\xE8me Winlux \x97 ",
        b"Winlux system information \x97 ",
        b"Informaci\xF3n del sistema Winlux \x97 ",
        b"Winlux-Systeminformationen \x97 ",
    ]))
    .u2(lt.day as u64)
    .s(b"/")
    .u2(lt.month as u64)
    .s(b"/")
    .u(lt.year as u64)
    .s(b" ")
    .u2(lt.hour as u64)
    .s(b":")
    .u2(lt.minute as u64)
    .s(b"\r\n");
    push_utf8(out, &mut k, head.get());
    let rows = &*core::ptr::addr_of!(ROWS);
    for cat in 0..CAT_COUNT {
        if COUNTS[cat] == 0 {
            continue;
        }
        push_utf8(out, &mut k, b"\r\n[");
        push_utf8(out, &mut k, tr4(&CAT_NAMES[cat]));
        push_utf8(out, &mut k, b"]\r\n");
        for row in rows[..COUNT].iter().filter(|r| r.cat as usize == cat) {
            if k + 600 > out.len() {
                break;
            }
            if row.head {
                push_utf8(out, &mut k, b"\r\n  ");
                push_utf8(out, &mut k, row.key());
                push_utf8(out, &mut k, b"\r\n");
                continue;
            }
            push_utf8(out, &mut k, b"    ");
            push_utf8(out, &mut k, row.key());
            // Alignement des valeurs (36 colonnes).
            let pad = 36usize.saturating_sub(row.key().len()).max(1);
            for _ in 0..pad {
                push_utf8(out, &mut k, b" ");
            }
            push_utf8(out, &mut k, row.val());
            push_utf8(out, &mut k, b"\r\n");
        }
    }
    let ok = ui::write_file(&dir, &out[..k]);
    let mut m = T::new();
    if ok {
        m.s(tr4(&[
            b"Export\xE9 dans ",
            b"Exported to ",
            b"Exportado a ",
            b"Exportiert nach ",
        ]));
        let shown = &dir_bytes(&dir)[2..];
        for &ch in shown {
            m.s(if ch == b'\\' {
                b"/"
            } else {
                core::slice::from_ref(&ch)
            });
        }
    } else {
        m.s(tr4(&[
            b"Export impossible.",
            b"Export failed.",
            b"No se pudo exportar.",
            b"Export fehlgeschlagen.",
        ]));
    }
    let msg = m.get();
    let mut copy = [0u8; 200];
    let len = msg.len().min(200);
    copy[..len].copy_from_slice(&msg[..len]);
    say(&copy[..len]);
}

fn dir_bytes(p: &Path) -> &[u8] {
    unsafe {
        let ptr = p.ptr() as *const u8;
        let mut n = 0;
        while *ptr.add(n) != 0 {
            n += 1;
        }
        core::slice::from_raw_parts(ptr, n)
    }
}

// ---------- Souris et clavier ----------

pub unsafe fn mouse(ev: Mouse, mx: i32, my: i32, _f: &Frame) -> bool {
    let h = &mut *core::ptr::addr_of_mut!(HITS);
    match ev {
        Mouse::Move => {
            h.mx = mx;
            h.my = my;
            let id = h.find(mx, my).unwrap_or(0);
            let changed = id != HOVER;
            HOVER = id;
            changed
        }
        Mouse::Wheel(delta) => {
            let max = VISIBLE_COUNT.saturating_sub(ROWS_SHOWN);
            let next = (SCROLL as i32 - delta * 3).clamp(0, max as i32) as usize;
            let changed = next != SCROLL;
            SCROLL = next;
            changed
        }
        Mouse::Down | Mouse::DoubleClick => {
            let id = h.find(mx, my).unwrap_or(0);
            SEARCH_FOCUS = id == ID_SEARCH;
            match id {
                i if (ID_CAT..ID_CAT + CAT_COUNT as u16).contains(&i) => {
                    CAT = (i - ID_CAT) as u8;
                    (*core::ptr::addr_of_mut!(SEARCH)).set(b"");
                    SCROLL = 0;
                    SELECTED = usize::MAX;
                    filter();
                }
                ID_REFRESH => {
                    load();
                    say(tr4(&[
                        b"Informations actualis\xE9es.",
                        b"Information refreshed.",
                        b"Informaci\xF3n actualizada.",
                        b"Informationen aktualisiert.",
                    ]));
                }
                ID_EXPORT => export(),
                i if i >= ID_ROW => {
                    let k = SCROLL + (i - ID_ROW) as usize;
                    if k < VISIBLE_COUNT {
                        SELECTED = k;
                    }
                }
                _ => {}
            }
            true
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, ch: u16) -> bool {
    if SEARCH_FOCUS && ch != 0 {
        if ch == 27 {
            SEARCH_FOCUS = false;
            return true;
        }
        let search = &mut *core::ptr::addr_of_mut!(SEARCH);
        if search.key(ch) {
            SCROLL = 0;
            SELECTED = usize::MAX;
            filter();
            return true;
        }
        return false;
    }
    match vk {
        0x26 | 0x28 if VISIBLE_COUNT > 0 => {
            SELECTED = match (SELECTED, vk) {
                (usize::MAX, _) => 0,
                (s, 0x26) => s.saturating_sub(1),
                (s, _) => (s + 1).min(VISIBLE_COUNT - 1),
            };
            if SELECTED < SCROLL {
                SCROLL = SELECTED;
            } else if SELECTED >= SCROLL + ROWS_SHOWN {
                SCROLL = SELECTED + 1 - ROWS_SHOWN;
            }
            true
        }
        0x74 => {
            load();
            true
        }
        _ => false,
    }
}

pub unsafe fn tick(_f: &Frame) -> bool {
    // Le message d'information disparaît tout seul.
    let m = (*core::ptr::addr_of!(MESSAGE)).len;
    if m > 0 && GetTickCount().wrapping_sub(MESSAGE_AT) >= 7000 {
        MESSAGE.set(b"");
        return true;
    }
    false
}
