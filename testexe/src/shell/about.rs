//! Paramètres > À propos : logo, nom de version et caractéristiques réelles
//! de l'appareil (processeur, mémoire, carte graphique, écran, modèle),
//! puis les métadonnées de construction du livrable installé.
//! Chaque valeur vient du système ; une ligne sans source est masquée.

use crate::gfx::*;
use crate::wm::Gfx;
use winlux_testexe::*;

/// Nom de version par branche (affiché après le numéro).
fn codename(version: &[u8]) -> &'static [u8] {
    let v = version.strip_prefix(b"v").unwrap_or(version);
    if v.starts_with(b"0.2") || v.starts_with(b"0.3") {
        b"Prisme"
    } else {
        b""
    }
}

const CAP: usize = 4096;
static mut CPUINFO: [u8; 16384] = [0; 16384];
static mut CPU_LEN: usize = 0;
static mut MEMINFO: [u8; CAP] = [0; CAP];
static mut MEM_LEN: usize = 0;
static mut VENDOR: [u8; 128] = [0; 128];
static mut VENDOR_LEN: usize = 0;
static mut PRODUCT: [u8; 128] = [0; 128];
static mut PRODUCT_LEN: usize = 0;
static mut HOST: [u8; 128] = [0; 128];
static mut HOST_LEN: usize = 0;
static mut KERNEL: [u8; 128] = [0; 128];
static mut KERNEL_LEN: usize = 0;
static mut LOADED: bool = false;

unsafe fn read_into(path: &core::ffi::CStr, buf: &mut [u8]) -> usize {
    let f = CreateFileA(
        path.as_ptr(),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return 0;
    }
    let mut total = 0usize;
    while total < buf.len() {
        let mut got = 0u32;
        let want = (buf.len() - total).min(65536) as u32;
        if ReadFile(
            f,
            buf[total..].as_mut_ptr(),
            want,
            &mut got,
            core::ptr::null_mut(),
        ) == 0
            || got == 0
        {
            break;
        }
        total += got as usize;
    }
    CloseHandle(f);
    // Une seule ligne (fichiers /sys et /proc/sys) : sans saut de ligne final.
    while total > 0 && (buf[total - 1] == b'\n' || buf[total - 1] == b' ') {
        total -= 1;
    }
    total
}

/// Lit les sources une fois par ouverture de la page (elles ne changent pas
/// pendant une session).
pub unsafe fn refresh() -> bool {
    if LOADED {
        return false;
    }
    LOADED = true;
    CPU_LEN = read_into(c"Z:\\proc\\cpuinfo", &mut *core::ptr::addr_of_mut!(CPUINFO));
    MEM_LEN = read_into(c"Z:\\proc\\meminfo", &mut *core::ptr::addr_of_mut!(MEMINFO));
    VENDOR_LEN = read_into(
        c"Z:\\sys\\class\\dmi\\id\\sys_vendor",
        &mut *core::ptr::addr_of_mut!(VENDOR),
    );
    PRODUCT_LEN = read_into(
        c"Z:\\sys\\class\\dmi\\id\\product_name",
        &mut *core::ptr::addr_of_mut!(PRODUCT),
    );
    HOST_LEN = read_into(
        c"Z:\\proc\\sys\\kernel\\hostname",
        &mut *core::ptr::addr_of_mut!(HOST),
    );
    KERNEL_LEN = read_into(
        c"Z:\\proc\\sys\\kernel\\osrelease",
        &mut *core::ptr::addr_of_mut!(KERNEL),
    );
    crate::gpuinfo::refresh(false);
    true
}

fn trim(mut s: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = s {
        s = rest;
    }
    while let [rest @ .., b' ' | b'\t' | b'\r'] = s {
        s = rest;
    }
    s
}

/// Valeur de `key : valeur` (format /proc/cpuinfo et /proc/meminfo).
fn colon_field<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    data.split(|&c| c == b'\n').find_map(|l| {
        let colon = l.iter().position(|&c| c == b':')?;
        (trim(&l[..colon]) == key).then(|| trim(&l[colon + 1..]))
    })
}

/// Champ `cle=valeur` du fichier BUILD-INFO.
pub fn build_field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    data.split(|&c| c == b'\n')
        .find_map(|l| {
            (l.len() > key.len() && l.starts_with(key) && l[key.len()] == b'=')
                .then(|| &l[key.len() + 1..])
        })
        .unwrap_or(b"")
}

/// Petit tampon de texte pour composer une valeur.
pub struct Line {
    buf: [u8; 192],
    len: usize,
}
impl Line {
    pub fn new() -> Self {
        Line {
            buf: [0; 192],
            len: 0,
        }
    }
    pub fn push(&mut self, s: &[u8]) -> &mut Self {
        for &c in s {
            if self.len < self.buf.len() {
                self.buf[self.len] = c;
                self.len += 1;
            }
        }
        self
    }
    pub fn num(&mut self, mut v: u64) -> &mut Self {
        let mut digits = [0u8; 20];
        let mut n = 0;
        loop {
            digits[n] = b'0' + (v % 10) as u8;
            n += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        while n > 0 {
            n -= 1;
            self.push(&[digits[n]]);
        }
        self
    }
    pub fn get(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

fn parse_u64(s: &[u8]) -> Option<u64> {
    let mut v: u64 = 0;
    let mut any = false;
    for &c in s {
        if c.is_ascii_digit() {
            v = v.saturating_mul(10).saturating_add((c - b'0') as u64);
            any = true;
        } else if any {
            break;
        }
    }
    any.then_some(v)
}

/// « Intel(R) Core(TM) i5-8250U CPU @ 1.60GHz » et le nombre de cœurs logiques.
pub unsafe fn processor(out: &mut Line) -> bool {
    let data = &(&*core::ptr::addr_of!(CPUINFO))[..CPU_LEN];
    let Some(model) = colon_field(data, b"model name") else {
        return false;
    };
    let threads = data
        .split(|&c| c == b'\n')
        .filter(|l| l.starts_with(b"processor"))
        .count() as u64;
    // Les marques déposées (R)/(TM) alourdissent la ligne sans rien apprendre.
    let mut i = 0;
    while i < model.len() {
        if model[i..].starts_with(b"(R)") || model[i..].starts_with(b"(TM)") {
            i += if model[i + 1] == b'R' { 3 } else { 4 };
            continue;
        }
        if model[i] == b' ' && out.get().last() == Some(&b' ') {
            i += 1;
            continue;
        }
        out.push(&model[i..i + 1]);
        i += 1;
    }
    if threads > 0 {
        out.push(b"   \xB7   ").num(threads).push(tr4(&[
            b" c\x9Curs logiques",
            b" logical cores",
            b" n\xFAcleos l\xF3gicos",
            b" logische Kerne",
        ]));
    }
    true
}

/// Mémoire vive installée, arrondie au Go (« 16 Go » comme Windows).
pub unsafe fn memory(out: &mut Line) -> bool {
    let data = &(&*core::ptr::addr_of!(MEMINFO))[..MEM_LEN];
    let Some(kb) = colon_field(data, b"MemTotal").and_then(parse_u64) else {
        return false;
    };
    let mb = kb / 1024;
    if mb >= 1536 {
        // Le noyau réserve une partie de la RAM : arrondir au Go supérieur
        // quand il manque moins de 6 %.
        let gb = (mb + 1023) / 1024;
        let shown = if mb * 100 >= gb * 1024 * 94 {
            gb
        } else {
            mb / 1024
        };
        out.num(shown).push(tr4(&[b" Go", b" GB", b" GB", b" GB"]));
    } else {
        out.num(mb).push(tr4(&[b" Mo", b" MB", b" MB", b" MB"]));
    }
    true
}

/// Fabricant et modèle (DMI), sans les valeurs génériques des cartes mères.
pub unsafe fn model(out: &mut Line) -> bool {
    let vendor = trim(&(&*core::ptr::addr_of!(VENDOR))[..VENDOR_LEN]);
    let product = trim(&(&*core::ptr::addr_of!(PRODUCT))[..PRODUCT_LEN]);
    let useless = |s: &[u8]| {
        s.is_empty()
            || s.eq_ignore_ascii_case(b"To be filled by O.E.M.")
            || s.eq_ignore_ascii_case(b"System Product Name")
            || s.eq_ignore_ascii_case(b"System manufacturer")
            || s.eq_ignore_ascii_case(b"Default string")
    };
    if !useless(vendor) {
        out.push(vendor);
    }
    if !useless(product) {
        if !out.get().is_empty() {
            out.push(b" ");
        }
        out.push(product);
    }
    !out.get().is_empty()
}

unsafe fn gpu(out: &mut Line) -> bool {
    let n = crate::gpuinfo::count();
    for i in 0..n.min(2) {
        if let Some(name) = crate::gpuinfo::get(i, b"name") {
            if !out.get().is_empty() {
                out.push(b" + ");
            }
            out.push(name);
        }
    }
    !out.get().is_empty()
}

fn ascii_or_dash(s: &[u8]) -> &[u8] {
    if s.is_empty() {
        b"\x97"
    } else {
        s
    }
}

/// Une ligne « libellé  valeur » d'une carte de caractéristiques.
unsafe fn spec(g: &mut Gfx, x: i32, w: i32, y: i32, label: &[u8], value: &[u8]) {
    let p = pal();
    let col = (w * 34 / 100).clamp(130, 220);
    text_fit(g.hdc, g.f.body, 13, p.text2, x, y, label, col - 12);
    text_fit(g.hdc, g.f.body, 13, p.text, x + col, y, value, w - col);
}

/// Carte arrondie titrée ; renvoie le haut de la première ligne.
unsafe fn card(g: &mut Gfx, c: &Rect, top: i32, rows: i32, title: &[u8]) -> i32 {
    let p = pal();
    let bottom = top + 44 + rows * 28 + 10;
    round_rect(
        g.hdc,
        g.b,
        c.left - 8,
        top - 1,
        c.right + 8,
        bottom + 1,
        8,
        p.border,
    );
    round_rect(
        g.hdc,
        g.b,
        c.left - 7,
        top,
        c.right + 7,
        bottom,
        7,
        p.surface,
    );
    text(g.hdc, g.f.semibold, p.text, c.left + 8, top + 14, title);
    top + 44
}

/// Dessine la page dans la zone de contenu `c` (sous le titre de la page).
pub unsafe fn draw(g: &mut Gfx, c: &Rect, build: &[u8], fallback_version: &[u8]) {
    refresh();
    let p = pal();
    let version = {
        let v = build_field(build, b"version");
        if v.is_empty() {
            fallback_version
        } else {
            v
        }
    };
    // En-tête : logo, nom et version.
    let top = c.top + 52;
    let s = 64;
    crate::gfx::brand::logo(g.hdc, g.b, c.left, top, s, 1000, Some(p.window));
    text(
        g.hdc,
        g.f.large,
        p.text,
        c.left + s + 20,
        top + 6,
        b"Winlux",
    );
    let mut line = Line::new();
    line.push(b"Version ").push(ascii_or_dash(version));
    let name = codename(version);
    if !name.is_empty() {
        line.push(b"  \xAB ").push(name).push(b" \xBB");
    }
    text(
        g.hdc,
        g.f.body,
        p.text2,
        c.left + s + 20,
        top + 38,
        line.get(),
    );
    let host = trim(&(&*core::ptr::addr_of!(HOST))[..HOST_LEN]);
    if !host.is_empty() {
        let mut l = Line::new();
        l.push(tr4(&[
            b"Nom de l'appareil : ",
            b"Device name: ",
            b"Nombre del dispositivo: ",
            b"Ger\xE4tename: ",
        ]))
        .push(host);
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            p.text3,
            c.left + s + 20,
            top + 58,
            l.get(),
            c.right - c.left - s - 20,
        );
    }

    // Caractéristiques de l'appareil : uniquement les lignes connues.
    let mut rows: [(&[u8], Line); 7] = [
        (b"", Line::new()),
        (b"", Line::new()),
        (b"", Line::new()),
        (b"", Line::new()),
        (b"", Line::new()),
        (b"", Line::new()),
        (b"", Line::new()),
    ];
    let mut n = 0;
    let mut add = |label: &'static [u8], fill: &mut dyn FnMut(&mut Line) -> bool| {
        let mut l = Line::new();
        if fill(&mut l) {
            rows[n] = (label, l);
            n += 1;
        }
    };
    add(
        tr4(&[b"Mod\xE8le", b"Model", b"Modelo", b"Modell"]),
        &mut |l| model(l),
    );
    add(
        tr4(&[b"Processeur", b"Processor", b"Procesador", b"Prozessor"]),
        &mut |l| processor(l),
    );
    add(
        tr4(&[
            b"M\xE9moire vive",
            b"Installed RAM",
            b"RAM instalada",
            b"Installierter RAM",
        ]),
        &mut |l| memory(l),
    );
    add(
        tr4(&[
            b"Carte graphique",
            b"Graphics card",
            b"Tarjeta gr\xE1fica",
            b"Grafikkarte",
        ]),
        &mut |l| gpu(l),
    );
    add(
        tr4(&[b"\xC9cran", b"Display", b"Pantalla", b"Bildschirm"]),
        &mut |l| {
            let (w, h) = (GetSystemMetrics(0), GetSystemMetrics(1));
            if w <= 0 || h <= 0 {
                return false;
            }
            l.num(w as u64).push(b" \xD7 ").num(h as u64);
            true
        },
    );
    add(
        tr4(&[
            b"Type du syst\xE8me",
            b"System type",
            b"Tipo de sistema",
            b"Systemtyp",
        ]),
        &mut |l| {
            l.push(tr4(&[
                b"Syst\xE8me 64 bits, processeur x64",
                b"64-bit operating system, x64 processor",
                b"Sistema operativo de 64 bits, procesador x64",
                b"64-Bit-Betriebssystem, x64-Prozessor",
            ]));
            true
        },
    );
    add(
        tr4(&[
            b"Noyau Linux",
            b"Linux kernel",
            b"N\xFAcleo Linux",
            b"Linux-Kernel",
        ]),
        &mut |l| {
            let k = trim(&(&*core::ptr::addr_of!(KERNEL))[..KERNEL_LEN]);
            l.push(k);
            !k.is_empty()
        },
    );
    let w = c.right - c.left - 16;
    let mut y = card(
        g,
        c,
        top + s + 30,
        n as i32,
        tr4(&[
            b"Caract\xE9ristiques de l'appareil",
            b"Device specifications",
            b"Especificaciones del dispositivo",
            b"Ger\xE4tespezifikationen",
        ]),
    );
    for (label, value) in rows.iter().take(n) {
        spec(g, c.left + 8, w, y, label, value.get());
        y += 28;
    }

    // Construction du livrable installé.
    let commit = build_field(build, b"commit");
    let short = &commit[..commit.len().min(12)];
    let built = build_field(build, b"built_at_utc");
    let scope: &[u8] = match build_field(build, b"scope") {
        b"system" => tr4(&[
            b"Syst\xE8me complet",
            b"Full system",
            b"Sistema completo",
            b"Vollst\xE4ndiges System",
        ]),
        b"runtime" => tr4(&[
            b"Runtime uniquement",
            b"Runtime only",
            b"Solo runtime",
            b"Nur Laufzeit",
        ]),
        _ => b"",
    };
    let build_rows: [(&[u8], &[u8]); 3] = [
        (tr4(&[b"Commit", b"Commit", b"Commit", b"Commit"]), short),
        (
            tr4(&[
                b"Construit le (UTC)",
                b"Built on (UTC)",
                b"Compilado el (UTC)",
                b"Erstellt am (UTC)",
            ]),
            built,
        ),
        (tr4(&[b"Livrable", b"Package", b"Paquete", b"Paket"]), scope),
    ];
    let shown = build_rows.iter().filter(|(_, v)| !v.is_empty()).count() as i32;
    let card_top = y + 22;
    let rows_shown = shown.max(1);
    if card_top + 44 + rows_shown * 28 + 10 >= c.bottom - 40 {
        return;
    }
    y = card(
        g,
        c,
        card_top,
        rows_shown,
        tr4(&[b"Construction", b"Build", b"Compilaci\xF3n", b"Build"]),
    );
    if shown == 0 {
        spec(
            g,
            c.left + 8,
            w,
            y,
            tr4(&[b"Provenance", b"Origin", b"Origen", b"Herkunft"]),
            tr4(&[
                b"Image sans m\xE9tadonn\xE9es de construction.",
                b"Image without build metadata.",
                b"Imagen sin metadatos de compilaci\xF3n.",
                b"Abbild ohne Build-Metadaten.",
            ]),
        );
        y += 28;
    }
    for (label, value) in build_rows.iter().filter(|(_, v)| !v.is_empty()) {
        spec(g, c.left + 8, w, y, label, value);
        y += 28;
    }
    let notice = tr4(&[
        b"Winlux est un logiciel libre (GNU GPL v3). Logo et fonds : cr\xE9ations Winlux.",
        b"Winlux is free software (GNU GPL v3). Logo and wallpapers: Winlux originals.",
        b"Winlux es software libre (GNU GPL v3). Logotipo y fondos: creaciones de Winlux.",
        b"Winlux ist freie Software (GNU GPL v3). Logo und Hintergr\xFCnde: Winlux-Eigenkreationen.",
    ]);
    if y + 60 < c.bottom - 30 {
        text_fit(
            g.hdc,
            g.f.caption,
            12,
            p.text3,
            c.left,
            y + 30,
            notice,
            c.right - c.left,
        );
    }
}
