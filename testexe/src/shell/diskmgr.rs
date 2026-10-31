//! Gestionnaire de disques façon Windows Disk Management : visualisation des
//! partitions, usage, montage/démontage et informations SMART.
//! Lecture depuis /proc/partitions, /proc/mounts et /sys/block via Z:\.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

// Navigation
const NAV_W: i32 = 200;
static mut TAB: usize = 0;
const TAB_NAMES: [Tr4; 3] = [
    [b"Volumes", b"Volumes", b"Vol\xFAmenes", b"Volumes"],
    [b"Disques", b"Disks", b"Discos", b"Datentr\xE4ger"],
    [b"SMART", b"SMART", b"SMART", b"SMART"],
];
static mut NAV_HOVER: i32 = -1;

// Scroll et sélection
static mut SCROLL_Y: i32 = 0;
static mut HOVER_ROW: i32 = -1;
static mut SELECTED_ROW: i32 = 0;

// Données partitions
const MAX_PARTS: usize = 32;

#[derive(Clone, Copy)]
struct Partition {
    name: [u8; 16],
    name_len: usize,
    mount: [u8; 64],
    mount_len: usize,
    fstype: [u8; 16],
    fstype_len: usize,
    size_mb: u32,
    used_mb: u32,
    major: u32,
    minor: u32,
    is_disk: bool,
}

const EMPTY_PART: Partition = Partition {
    name: [0; 16],
    name_len: 0,
    mount: [0; 64],
    mount_len: 0,
    fstype: [0; 16],
    fstype_len: 0,
    size_mb: 0,
    used_mb: 0,
    major: 0,
    minor: 0,
    is_disk: false,
};

static mut PARTS: [Partition; MAX_PARTS] = [EMPTY_PART; MAX_PARTS];
static mut PART_COUNT: usize = 0;
static mut LAST_REFRESH: u32 = 0;

// SMART
const MAX_SMART: usize = 16;
#[derive(Clone, Copy)]
struct SmartAttr {
    id: u32,
    name: [u8; 24],
    name_len: usize,
    value: u32,
    worst: u32,
    threshold: u32,
    raw: u64,
}
const EMPTY_SMART: SmartAttr = SmartAttr {
    id: 0,
    name: [0; 24],
    name_len: 0,
    value: 0,
    worst: 0,
    threshold: 0,
    raw: 0,
};
static mut SMART_ATTRS: [SmartAttr; MAX_SMART] = [EMPTY_SMART; MAX_SMART];
static mut SMART_COUNT: usize = 0;
static mut SMART_DISK: [u8; 16] = [0; 16];
static mut SMART_DISK_LEN: usize = 0;
static mut SMART_HEALTH: [u8; 16] = [0; 16];
static mut SMART_HEALTH_LEN: usize = 0;

// Boutons
static mut MOUNT_HOVER: bool = false;
static mut UNMOUNT_HOVER: bool = false;

// ---------- Lecture des données ----------

const BUF_CAP: usize = 4096;
static mut BUF: [u8; BUF_CAP] = [0; BUF_CAP];

unsafe fn read_file(path: *const i8, buf: &mut [u8]) -> usize {
    let f = CreateFileA(
        path,
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return 0;
    }
    let mut read = 0u32;
    ReadFile(
        f,
        buf.as_mut_ptr(),
        buf.len() as u32,
        &mut read,
        core::ptr::null_mut(),
    );
    CloseHandle(f);
    read as usize
}

unsafe fn refresh() {
    let t = GetTickCount();
    if t.wrapping_sub(LAST_REFRESH) < 3000 && PART_COUNT > 0 {
        return;
    }
    LAST_REFRESH = t;
    PART_COUNT = 0;

    // Lecture /proc/partitions
    let n = read_file(c"Z:\\proc\\partitions".as_ptr(), &mut BUF);
    if n > 0 {
        parse_partitions(&BUF[..n]);
    } else {
        add_demo_partitions();
    }

    // Lecture /proc/mounts pour les points de montage et types de FS
    let n2 = read_file(c"Z:\\proc\\mounts".as_ptr(), &mut BUF);
    if n2 > 0 {
        parse_mounts(&BUF[..n2]);
    }

    // Lecture de la taille/usage via statfs (on lit df via un fichier de contrôle)
    let n3 = read_file(c"Z:\\run\\winlux\\disk-usage.txt".as_ptr(), &mut BUF);
    if n3 > 0 {
        parse_disk_usage(&BUF[..n3]);
    }
}

unsafe fn parse_partitions(data: &[u8]) {
    let mut i = 0;
    let mut line_no = 0;
    while i < data.len() {
        let line_start = i;
        while i < data.len() && data[i] != b'\n' {
            i += 1;
        }
        let line = &data[line_start..i];
        if i < data.len() {
            i += 1;
        }
        line_no += 1;
        if line_no <= 2 {
            continue; // Skip header
        }
        if PART_COUNT >= MAX_PARTS {
            break;
        }
        // Format: major minor #blocks name
        let fields = split_whitespace(line);
        if fields.len() >= 4 {
            let mut p = EMPTY_PART;
            p.major = parse_u32(fields[0]);
            p.minor = parse_u32(fields[1]);
            let blocks = parse_u64(fields[2]);
            p.size_mb = (blocks / 1024) as u32;
            let n = fields[3].len().min(p.name.len());
            p.name[..n].copy_from_slice(&fields[3][..n]);
            p.name_len = n;

            // Un disque entier (pas de numéro à la fin, ou minor == 0)
            p.is_disk = p.minor == 0
                || (p.name_len > 0
                    && !(p.name[p.name_len - 1] >= b'0' && p.name[p.name_len - 1] <= b'9'));
            // Filtre : ignorer les périphériques loop et ram
            if !starts_with(&p.name[..p.name_len], b"loop")
                && !starts_with(&p.name[..p.name_len], b"ram")
            {
                PARTS[PART_COUNT] = p;
                PART_COUNT += 1;
            }
        }
    }
}

unsafe fn parse_mounts(data: &[u8]) {
    let mut i = 0;
    while i < data.len() {
        let line_start = i;
        while i < data.len() && data[i] != b'\n' {
            i += 1;
        }
        let line = &data[line_start..i];
        if i < data.len() {
            i += 1;
        }
        // Format: device mountpoint fstype options ...
        let fields = split_whitespace(line);
        if fields.len() >= 3 {
            let dev = fields[0];
            let mount = fields[1];
            let fstype = fields[2];
            // Chercher le device correspondant dans PARTS
            for j in 0..PART_COUNT {
                let pname = &PARTS[j].name[..PARTS[j].name_len];
                // dev est /dev/sda1, pname est sda1
                if dev.len() > 5 && &dev[5..] == pname {
                    let ml = mount.len().min(PARTS[j].mount.len());
                    PARTS[j].mount[..ml].copy_from_slice(&mount[..ml]);
                    PARTS[j].mount_len = ml;
                    let fl = fstype.len().min(PARTS[j].fstype.len());
                    PARTS[j].fstype[..fl].copy_from_slice(&fstype[..fl]);
                    PARTS[j].fstype_len = fl;
                }
            }
        }
    }
}

unsafe fn parse_disk_usage(data: &[u8]) {
    // Format: device used_mb total_mb
    let mut i = 0;
    while i < data.len() {
        let line_start = i;
        while i < data.len() && data[i] != b'\n' {
            i += 1;
        }
        let line = &data[line_start..i];
        if i < data.len() {
            i += 1;
        }
        let fields = split_whitespace(line);
        if fields.len() >= 3 {
            let dev = fields[0];
            let used = parse_u32(fields[1]);
            let total = parse_u32(fields[2]);
            for j in 0..PART_COUNT {
                let pname = &PARTS[j].name[..PARTS[j].name_len];
                if dev == pname || (dev.len() > 5 && &dev[5..] == pname) {
                    PARTS[j].used_mb = used;
                    if total > 0 {
                        PARTS[j].size_mb = total;
                    }
                }
            }
        }
    }
}

unsafe fn load_smart(disk_name: &[u8]) {
    SMART_COUNT = 0;
    let n = disk_name.len().min(SMART_DISK.len());
    SMART_DISK[..n].copy_from_slice(&disk_name[..n]);
    SMART_DISK_LEN = n;
    SMART_HEALTH_LEN = 0;

    // Essayer Z:\run\winlux\smart-<disk>.txt
    let mut path = [0u8; 64];
    let mut pos = 0;
    for &c in b"Z:\\run\\winlux\\smart-" {
        path[pos] = c;
        pos += 1;
    }
    for &c in &disk_name[..n] {
        path[pos] = c;
        pos += 1;
    }
    for &c in b".txt\0" {
        path[pos] = c;
        pos += 1;
    }
    let nr = read_file(path.as_ptr() as *const i8, &mut BUF);
    if nr > 0 {
        parse_smart(&BUF[..nr]);
    } else {
        add_demo_smart();
    }
}

unsafe fn parse_smart(data: &[u8]) {
    let mut i = 0;
    while i < data.len() {
        let line_start = i;
        while i < data.len() && data[i] != b'\n' {
            i += 1;
        }
        let line = &data[line_start..i];
        if i < data.len() {
            i += 1;
        }
        // health=PASSED
        if starts_with(line, b"health=") && line.len() > 7 {
            let hl = (line.len() - 7).min(SMART_HEALTH.len());
            SMART_HEALTH[..hl].copy_from_slice(&line[7..7 + hl]);
            SMART_HEALTH_LEN = hl;
            continue;
        }
        // id value worst threshold raw name
        let fields = split_whitespace(line);
        if fields.len() >= 6 && SMART_COUNT < MAX_SMART {
            let mut a = EMPTY_SMART;
            a.id = parse_u32(fields[0]);
            a.value = parse_u32(fields[1]);
            a.worst = parse_u32(fields[2]);
            a.threshold = parse_u32(fields[3]);
            a.raw = parse_u64(fields[4]);
            let nl = fields[5].len().min(a.name.len());
            a.name[..nl].copy_from_slice(&fields[5][..nl]);
            a.name_len = nl;
            SMART_ATTRS[SMART_COUNT] = a;
            SMART_COUNT += 1;
        }
    }
}

// ---------- Données de démonstration ----------

unsafe fn add_demo_partitions() {
    let demos: [(&[u8], bool, u32, u32, &[u8], &[u8]); 5] = [
        (b"sda", true, 512000, 0, b"", b""),
        (b"sda1", false, 512, 180, b"/boot/efi", b"vfat"),
        (b"sda2", false, 480000, 128000, b"/", b"ext4"),
        (b"sda3", false, 31000, 0, b"[SWAP]", b"swap"),
        (b"nvme0n1", true, 1024000, 0, b"", b"nvme"),
    ];
    for (name, is_disk, size, used, mount, fs) in demos {
        if PART_COUNT >= MAX_PARTS {
            break;
        }
        let mut p = EMPTY_PART;
        let n = name.len().min(p.name.len());
        p.name[..n].copy_from_slice(&name[..n]);
        p.name_len = n;
        p.is_disk = is_disk;
        p.size_mb = size;
        p.used_mb = used;
        let ml = mount.len().min(p.mount.len());
        p.mount[..ml].copy_from_slice(&mount[..ml]);
        p.mount_len = ml;
        let fl = fs.len().min(p.fstype.len());
        p.fstype[..fl].copy_from_slice(&fs[..fl]);
        p.fstype_len = fl;
        PARTS[PART_COUNT] = p;
        PART_COUNT += 1;
    }
}

unsafe fn add_demo_smart() {
    SMART_HEALTH[..6].copy_from_slice(b"PASSED");
    SMART_HEALTH_LEN = 6;
    let demos: [(u32, &[u8], u32, u32, u32, u64); 5] = [
        (1, b"Read_Error_Rate", 100, 100, 6, 0),
        (5, b"Reallocated_Sectors", 100, 100, 10, 0),
        (9, b"Power_On_Hours", 98, 98, 0, 4825),
        (194, b"Temperature_Cel", 68, 55, 0, 32),
        (197, b"Pending_Sectors", 100, 100, 0, 0),
    ];
    SMART_COUNT = 0;
    for (id, name, val, worst, thresh, raw) in demos {
        let mut a = EMPTY_SMART;
        a.id = id;
        let nl = name.len().min(a.name.len());
        a.name[..nl].copy_from_slice(&name[..nl]);
        a.name_len = nl;
        a.value = val;
        a.worst = worst;
        a.threshold = thresh;
        a.raw = raw;
        SMART_ATTRS[SMART_COUNT] = a;
        SMART_COUNT += 1;
    }
}

// ---------- Utilitaires ----------

fn split_whitespace(s: &[u8]) -> [&[u8]; 8] {
    let mut result: [&[u8]; 8] = [b""; 8];
    let mut count = 0;
    let mut i = 0;
    while i < s.len() && count < 8 {
        while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
            i += 1;
        }
        let start = i;
        while i < s.len() && s[i] != b' ' && s[i] != b'\t' && s[i] != b'\n' && s[i] != b'\r' {
            i += 1;
        }
        if i > start {
            result[count] = &s[start..i];
            count += 1;
        }
    }
    result
}

fn parse_u32(s: &[u8]) -> u32 {
    let mut val: u32 = 0;
    for &c in s {
        if c >= b'0' && c <= b'9' {
            val = val.saturating_mul(10).saturating_add((c - b'0') as u32);
        }
    }
    val
}

fn parse_u64(s: &[u8]) -> u64 {
    let mut val: u64 = 0;
    for &c in s {
        if c >= b'0' && c <= b'9' {
            val = val.saturating_mul(10).saturating_add((c - b'0') as u64);
        }
    }
    val
}

fn starts_with(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && &s[..prefix.len()] == prefix
}

fn format_size(buf: &mut [u8], mb: u32) -> usize {
    let mut pos = 0;
    if mb >= 1024 {
        let gb = mb / 1024;
        let frac = (mb % 1024) * 10 / 1024;
        if gb >= 100 {
            buf[pos] = b'0' + (gb / 100).min(9) as u8;
            pos += 1;
        }
        if gb >= 10 {
            buf[pos] = b'0' + (gb / 10 % 10) as u8;
            pos += 1;
        }
        buf[pos] = b'0' + (gb % 10) as u8;
        pos += 1;
        buf[pos] = b'.';
        pos += 1;
        buf[pos] = b'0' + frac.min(9) as u8;
        pos += 1;
        for &c in tr4(&[b" Go", b" GB", b" GB", b" GB"]) {
            buf[pos] = c;
            pos += 1;
        }
    } else {
        if mb >= 100 {
            buf[pos] = b'0' + (mb / 100).min(9) as u8;
            pos += 1;
        }
        if mb >= 10 {
            buf[pos] = b'0' + (mb / 10 % 10) as u8;
            pos += 1;
        }
        buf[pos] = b'0' + (mb % 10) as u8;
        pos += 1;
        for &c in tr4(&[b" Mo", b" MB", b" MB", b" MB"]) {
            buf[pos] = c;
            pos += 1;
        }
    }
    pos
}

fn format_u32(buf: &mut [u8], v: u32) -> usize {
    if v == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut digits = [0u8; 10];
    let mut n = 0;
    let mut val = v;
    while val > 0 {
        digits[n] = b'0' + (val % 10) as u8;
        val /= 10;
        n += 1;
    }
    for i in 0..n {
        buf[i] = digits[n - 1 - i];
    }
    n
}

fn format_u64(buf: &mut [u8], v: u64) -> usize {
    if v == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut digits = [0u8; 20];
    let mut n = 0;
    let mut val = v;
    while val > 0 {
        digits[n] = b'0' + (val % 10) as u8;
        val /= 10;
        n += 1;
    }
    for i in 0..n {
        buf[i] = digits[n - 1 - i];
    }
    n
}

// ---------- Dessin ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let c = &f.client;
    let p = pal();

    // Fond
    fill(g.hdc, g.b, c.left, c.top, c.right, c.bottom, p.window);

    // Barre de navigation latérale
    fill(g.hdc, g.b, c.left, c.top, c.left + NAV_W, c.bottom, p.mica);
    fill(
        g.hdc,
        g.b,
        c.left + NAV_W,
        c.top,
        c.left + NAV_W + 1,
        c.bottom,
        p.border,
    );

    // Icône + titre en haut du nav
    glyph(g.hdc, g.b, g.f, Glyph::Drive, c.left + 16, c.top + 16, 24);
    text(
        g.hdc,
        g.f.semibold,
        p.text,
        c.left + 48,
        c.top + 20,
        tr4(&[
            b"Gestion des disques",
            b"Disk Management",
            b"Administraci\xF3n de discos",
            b"Datentr\xE4gerverwaltung",
        ]),
    );

    // Onglets
    for i in 0..TAB_NAMES.len() {
        let ty = c.top + 60 + i as i32 * 36;
        let active = TAB == i;
        if active {
            fill(
                g.hdc,
                g.b,
                c.left + 4,
                ty,
                c.left + NAV_W - 4,
                ty + 32,
                p.selected,
            );
            fill(
                g.hdc,
                g.b,
                c.left + 1,
                ty + 6,
                c.left + 4,
                ty + 26,
                p.accent,
            );
        } else if NAV_HOVER == i as i32 {
            fill(
                g.hdc,
                g.b,
                c.left + 4,
                ty,
                c.left + NAV_W - 4,
                ty + 32,
                p.hover,
            );
        }
        text(
            g.hdc,
            if active { g.f.semibold } else { g.f.body },
            if active { p.text } else { p.text2 },
            c.left + 20,
            ty + 8,
            tr4(&TAB_NAMES[i]),
        );
    }

    // Contenu principal
    let content_x = c.left + NAV_W + 1;
    let content_w = c.right - content_x;
    match TAB {
        0 => draw_volumes(g, content_x, c.top, content_w, c.bottom - c.top),
        1 => draw_disks(g, content_x, c.top, content_w, c.bottom - c.top),
        2 => draw_smart(g, content_x, c.top, content_w, c.bottom - c.top),
        _ => {}
    }
}

unsafe fn draw_volumes(g: &mut Gfx, x: i32, top: i32, w: i32, h: i32) {
    let p = pal();
    text(
        g.hdc,
        g.f.large,
        p.text,
        x + 24,
        top + 16,
        tr4(&[b"Volumes", b"Volumes", b"Vol\xFAmenes", b"Volumes"]),
    );

    // En-tête colonnes
    let col_y = top + 52;
    let cols: [(i32, &[u8]); 5] = [
        (x + 24, tr4(&[b"Nom", b"Name", b"Nombre", b"Name"])),
        (
            x + 120,
            tr4(&[
                b"Point de montage",
                b"Mount point",
                b"Punto de montaje",
                b"Einh\xE4ngepunkt",
            ]),
        ),
        (
            x + 300,
            tr4(&[
                b"Syst\xE8me",
                b"File system",
                b"Sistema de archivos",
                b"Dateisystem",
            ]),
        ),
        (
            x + 400,
            tr4(&[b"Taille", b"Size", b"Tama\xF1o", b"Gr\xF6\xDFe"]),
        ),
        (x + 500, tr4(&[b"Utilis\xE9", b"Used", b"Usado", b"Belegt"])),
    ];
    for (cx, label) in cols {
        text(g.hdc, g.f.small, p.text3, cx, col_y, label);
    }
    fill(
        g.hdc,
        g.b,
        x + 16,
        col_y + 18,
        x + w - 16,
        col_y + 19,
        p.border,
    );

    // Liste des partitions (pas les disques entiers)
    let mut row = 0;
    let row_h = 56;
    for i in 0..PART_COUNT {
        let part = &PARTS[i];
        if part.is_disk {
            continue;
        }
        let ry = col_y + 24 + row * row_h - SCROLL_Y;
        if ry + row_h < col_y + 24 || ry > top + h {
            row += 1;
            continue;
        }

        // Sélection / survol
        if SELECTED_ROW == row {
            round_rect(
                g.hdc,
                g.b,
                x + 12,
                ry,
                x + w - 12,
                ry + row_h - 4,
                4,
                p.selected,
            );
        } else if HOVER_ROW == row {
            round_rect(
                g.hdc,
                g.b,
                x + 12,
                ry,
                x + w - 12,
                ry + row_h - 4,
                4,
                p.hover,
            );
        }

        // Icône disque
        glyph(g.hdc, g.b, g.f, Glyph::Drive, x + 24, ry + 8, 20);

        // Nom
        text(
            g.hdc,
            g.f.body,
            p.text,
            x + 52,
            ry + 6,
            &part.name[..part.name_len],
        );

        // Point de montage
        if part.mount_len > 0 {
            text(
                g.hdc,
                g.f.body,
                p.text2,
                x + 120,
                ry + 6,
                &part.mount[..part.mount_len],
            );
        } else {
            text(
                g.hdc,
                g.f.body,
                p.text3,
                x + 120,
                ry + 6,
                tr4(&[
                    b"(non mont\xE9)",
                    b"(not mounted)",
                    b"(no montado)",
                    b"(nicht eingeh\xE4ngt)",
                ]),
            );
        }

        // Système de fichiers
        if part.fstype_len > 0 {
            text(
                g.hdc,
                g.f.body,
                p.text2,
                x + 300,
                ry + 6,
                &part.fstype[..part.fstype_len],
            );
        }

        // Taille
        let mut sz = [0u8; 16];
        let sl = format_size(&mut sz, part.size_mb);
        text(g.hdc, g.f.body, p.text2, x + 400, ry + 6, &sz[..sl]);

        // Barre d'usage
        if part.size_mb > 0 && part.used_mb > 0 {
            let bar_x = x + 52;
            let bar_w = w - 80;
            let bar_y = ry + 28;
            let bar_h = 12;
            round_rect(
                g.hdc,
                g.b,
                bar_x,
                bar_y,
                bar_x + bar_w,
                bar_y + bar_h,
                3,
                p.surface2,
            );
            let pct = (part.used_mb as u64 * bar_w as u64 / part.size_mb as u64) as i32;
            let color = if part.used_mb * 100 / part.size_mb > 90 {
                rgb(0xFF, 0x60, 0x50) // Rouge si >90%
            } else if part.used_mb * 100 / part.size_mb > 70 {
                rgb(0xFF, 0xA0, 0x40) // Orange si >70%
            } else {
                p.accent
            };
            if pct > 0 {
                round_rect(
                    g.hdc,
                    g.b,
                    bar_x,
                    bar_y,
                    bar_x + pct.min(bar_w),
                    bar_y + bar_h,
                    3,
                    color,
                );
            }
            // Pourcentage
            let used_pct = (part.used_mb as u64 * 100 / part.size_mb as u64) as u32;
            let mut pct_str = [0u8; 8];
            let pl = format_u32(&mut pct_str, used_pct);
            pct_str[pl] = b'%';
            text(
                g.hdc,
                g.f.small,
                p.text3,
                bar_x + bar_w + 4,
                bar_y,
                &pct_str[..pl + 1],
            );
        }

        row += 1;
    }

    // Boutons Monter/Démonter
    let btn_y = top + h - 52;
    let mount_r = rect(x + w - 280, btn_y, x + w - 156, btn_y + 32);
    let unmount_r = rect(x + w - 148, btn_y, x + w - 24, btn_y + 32);

    round_rect(
        g.hdc,
        g.b,
        mount_r.left,
        mount_r.top,
        mount_r.right,
        mount_r.bottom,
        4,
        if MOUNT_HOVER { p.hover } else { p.surface2 },
    );
    text(
        g.hdc,
        g.f.body,
        p.text,
        mount_r.left + 16,
        mount_r.top + 8,
        tr4(&[b"Monter", b"Mount", b"Montar", b"Einh\xE4ngen"]),
    );

    round_rect(
        g.hdc,
        g.b,
        unmount_r.left,
        unmount_r.top,
        unmount_r.right,
        unmount_r.bottom,
        4,
        if UNMOUNT_HOVER { p.hover } else { p.surface2 },
    );
    text(
        g.hdc,
        g.f.body,
        p.text,
        unmount_r.left + 16,
        unmount_r.top + 8,
        tr4(&[b"D\xE9monter", b"Unmount", b"Desmontar", b"Aush\xE4ngen"]),
    );
}

unsafe fn draw_disks(g: &mut Gfx, x: i32, top: i32, w: i32, _h: i32) {
    let p = pal();
    text(
        g.hdc,
        g.f.large,
        p.text,
        x + 24,
        top + 16,
        tr4(&[
            b"Disques physiques",
            b"Physical disks",
            b"Discos f\xEDsicos",
            b"Physische Datentr\xE4ger",
        ]),
    );

    let mut row = 0;
    let card_h = 120;
    for i in 0..PART_COUNT {
        let disk = &PARTS[i];
        if !disk.is_disk {
            continue;
        }
        let cy = top + 56 + row * (card_h + 12);

        // Carte du disque
        panel(
            g.hdc,
            g.b,
            x + 16,
            cy,
            x + w - 16,
            cy + card_h,
            6,
            p.surface,
            p.border,
        );

        // Icône + nom
        glyph(g.hdc, g.b, g.f, Glyph::Drive, x + 28, cy + 12, 28);
        text(
            g.hdc,
            g.f.semibold,
            p.text,
            x + 64,
            cy + 16,
            &disk.name[..disk.name_len],
        );

        // Taille totale
        let mut sz = [0u8; 16];
        let sl = format_size(&mut sz, disk.size_mb);
        text(g.hdc, g.f.body, p.text2, x + 64, cy + 36, &sz[..sl]);

        // Barre de partitions (représentation graphique)
        let bar_x = x + 28;
        let bar_w = w - 56;
        let bar_y = cy + 64;
        let bar_h = 32;
        fill(
            g.hdc,
            g.b,
            bar_x,
            bar_y,
            bar_x + bar_w,
            bar_y + bar_h,
            p.surface2,
        );

        if disk.size_mb > 0 {
            let colors = [
                p.accent,
                rgb(0x80, 0xC0, 0x80),
                rgb(0xE0, 0x90, 0x50),
                rgb(0xA0, 0x80, 0xD0),
                rgb(0x60, 0xB0, 0xD0),
            ];
            let mut px = bar_x;
            let mut ci = 0;
            for j in 0..PART_COUNT {
                let part = &PARTS[j];
                if part.is_disk {
                    continue;
                }
                // Vérifier que cette partition appartient à ce disque
                let dname = &disk.name[..disk.name_len];
                let pname = &part.name[..part.name_len];
                if !starts_with(pname, dname) {
                    continue;
                }
                let pw = (part.size_mb as u64 * bar_w as u64 / disk.size_mb as u64) as i32;
                if pw > 1 {
                    fill(
                        g.hdc,
                        g.b,
                        px,
                        bar_y + 1,
                        (px + pw).min(bar_x + bar_w),
                        bar_y + bar_h - 1,
                        colors[ci % colors.len()],
                    );
                    // Nom partition (petit)
                    if pw > 40 {
                        text(
                            g.hdc,
                            g.f.small,
                            p.on_accent,
                            px + 4,
                            bar_y + 4,
                            &part.name[..part.name_len],
                        );
                        let mut psz = [0u8; 16];
                        let psl = format_size(&mut psz, part.size_mb);
                        text(
                            g.hdc,
                            g.f.small,
                            p.on_accent,
                            px + 4,
                            bar_y + 18,
                            &psz[..psl],
                        );
                    }
                }
                px += pw + 1;
                ci += 1;
            }
        }

        row += 1;
    }

    if row == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text3,
            x + 24,
            top + 56,
            tr4(&[
                b"Aucun disque d\xE9tect\xE9",
                b"No disk detected",
                b"No se detect\xF3 ning\xFAn disco",
                b"Kein Datentr\xE4ger erkannt",
            ]),
        );
    }
}

unsafe fn draw_smart(g: &mut Gfx, x: i32, top: i32, w: i32, _h: i32) {
    let p = pal();
    text(
        g.hdc,
        g.f.large,
        p.text,
        x + 24,
        top + 16,
        tr4(&[
            b"Informations SMART",
            b"SMART information",
            b"Informaci\xF3n SMART",
            b"SMART-Informationen",
        ]),
    );

    // Disque sélectionné
    if SMART_DISK_LEN == 0 {
        // Charger le premier disque
        for i in 0..PART_COUNT {
            if PARTS[i].is_disk {
                load_smart(
                    &{
                        let mut tmp = [0u8; 16];
                        let n = PARTS[i].name_len;
                        tmp[..n].copy_from_slice(&PARTS[i].name[..n]);
                        tmp
                    }[..PARTS[i].name_len],
                );
                break;
            }
        }
    }

    if SMART_DISK_LEN > 0 {
        text(
            g.hdc,
            g.f.semibold,
            p.text,
            x + 24,
            top + 48,
            &SMART_DISK[..SMART_DISK_LEN],
        );
    }

    // Santé
    if SMART_HEALTH_LEN > 0 {
        let health_color = if starts_with(&SMART_HEALTH[..SMART_HEALTH_LEN], b"PASSED")
            || starts_with(&SMART_HEALTH[..SMART_HEALTH_LEN], b"OK")
        {
            rgb(0x60, 0xC0, 0x60)
        } else {
            rgb(0xFF, 0x60, 0x50)
        };
        text(
            g.hdc,
            g.f.body,
            p.text3,
            x + 24,
            top + 68,
            tr4(&[b"Sant\xE9 : ", b"Health: ", b"Estado: ", b"Zustand: "]),
        );
        text(
            g.hdc,
            g.f.semibold,
            health_color,
            x + 80,
            top + 68,
            &SMART_HEALTH[..SMART_HEALTH_LEN],
        );
    }

    // En-tête colonnes SMART
    let col_y = top + 96;
    text(g.hdc, g.f.small, p.text3, x + 24, col_y, b"ID");
    text(
        g.hdc,
        g.f.small,
        p.text3,
        x + 60,
        col_y,
        tr4(&[b"Attribut", b"Attribute", b"Atributo", b"Attribut"]),
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        x + 240,
        col_y,
        tr4(&[b"Valeur", b"Value", b"Valor", b"Wert"]),
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        x + 310,
        col_y,
        tr4(&[b"Pire", b"Worst", b"Peor", b"Schlechtester"]),
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        x + 370,
        col_y,
        tr4(&[b"Seuil", b"Threshold", b"Umbral", b"Schwellenwert"]),
    );
    text(
        g.hdc,
        g.f.small,
        p.text3,
        x + 430,
        col_y,
        tr4(&[b"Brut", b"Raw", b"Sin procesar", b"Rohwert"]),
    );
    fill(
        g.hdc,
        g.b,
        x + 16,
        col_y + 18,
        x + w - 16,
        col_y + 19,
        p.border,
    );

    for i in 0..SMART_COUNT {
        let a = &SMART_ATTRS[i];
        let ry = col_y + 24 + i as i32 * 26;

        // Indicateur de santé
        let healthy = a.threshold == 0 || a.value >= a.threshold;
        let dot_color = if healthy {
            rgb(0x60, 0xC0, 0x60)
        } else {
            rgb(0xFF, 0x60, 0x50)
        };
        ellipse(g.hdc, g.b, x + 16, ry + 4, x + 22, ry + 10, dot_color);

        // ID
        let mut id_buf = [0u8; 8];
        let il = format_u32(&mut id_buf, a.id);
        text(g.hdc, g.f.body, p.text2, x + 30, ry, &id_buf[..il]);

        // Nom
        text(g.hdc, g.f.body, p.text, x + 60, ry, &a.name[..a.name_len]);

        // Valeur
        let mut vbuf = [0u8; 8];
        let vl = format_u32(&mut vbuf, a.value);
        text(g.hdc, g.f.body, p.text2, x + 250, ry, &vbuf[..vl]);

        // Pire
        let wl = format_u32(&mut vbuf, a.worst);
        text(g.hdc, g.f.body, p.text2, x + 320, ry, &vbuf[..wl]);

        // Seuil
        let tl = format_u32(&mut vbuf, a.threshold);
        text(g.hdc, g.f.body, p.text2, x + 380, ry, &vbuf[..tl]);

        // Brut
        let mut rbuf = [0u8; 20];
        let rl = format_u64(&mut rbuf, a.raw);
        text(g.hdc, g.f.body, p.text2, x + 430, ry, &rbuf[..rl]);
    }

    if SMART_COUNT == 0 {
        text(
            g.hdc,
            g.f.body,
            p.text3,
            x + 24,
            col_y + 24,
            tr4(&[
                b"Aucune donn\xE9e SMART disponible",
                b"No SMART data available",
                b"No hay datos SMART disponibles",
                b"Keine SMART-Daten verf\xFCgbar",
            ]),
        );
    }
}

// ---------- Interaction ----------

pub unsafe fn mouse(ev: Mouse, x: i32, y: i32, f: &Frame) -> bool {
    let c = &f.client;
    refresh();

    match ev {
        Mouse::Down => {
            // Navigation
            if x >= c.left && x < c.left + NAV_W {
                for i in 0..TAB_NAMES.len() {
                    let ty = c.top + 60 + i as i32 * 36;
                    if y >= ty && y < ty + 32 {
                        TAB = i;
                        SCROLL_Y = 0;
                        SELECTED_ROW = 0;
                        if i == 2 {
                            SMART_DISK_LEN = 0;
                        }
                        return true;
                    }
                }
                return false;
            }

            let content_x = c.left + NAV_W + 1;
            let content_w = c.right - content_x;

            if TAB == 0 {
                // Sélection de ligne
                let col_y = c.top + 52;
                let row_h = 56;
                let mut row = 0i32;
                for i in 0..PART_COUNT {
                    if PARTS[i].is_disk {
                        continue;
                    }
                    let ry = col_y + 24 + row * row_h - SCROLL_Y;
                    if y >= ry
                        && y < ry + row_h - 4
                        && x >= content_x + 12
                        && x < content_x + content_w - 12
                    {
                        SELECTED_ROW = row;
                        return true;
                    }
                    row += 1;
                }

                // Boutons
                let btn_y = c.top + (c.bottom - c.top) - 52;
                let mount_r = rect(
                    content_x + content_w - 280,
                    btn_y,
                    content_x + content_w - 156,
                    btn_y + 32,
                );
                let unmount_r = rect(
                    content_x + content_w - 148,
                    btn_y,
                    content_x + content_w - 24,
                    btn_y + 32,
                );

                if inside(&mount_r, x, y) {
                    do_mount();
                    return true;
                }
                if inside(&unmount_r, x, y) {
                    do_unmount();
                    return true;
                }
            }

            if TAB == 1 {
                // Clic sur un disque -> afficher SMART
                let mut row = 0;
                let card_h = 120;
                for i in 0..PART_COUNT {
                    if !PARTS[i].is_disk {
                        continue;
                    }
                    let cy = c.top + 56 + row * (card_h + 12);
                    if y >= cy && y < cy + card_h {
                        TAB = 2;
                        load_smart(&PARTS[i].name[..PARTS[i].name_len]);
                        return true;
                    }
                    row += 1;
                }
            }

            false
        }
        Mouse::Move => {
            let mut changed = false;
            let old_nav = NAV_HOVER;
            NAV_HOVER = -1;
            if x >= c.left && x < c.left + NAV_W {
                for i in 0..TAB_NAMES.len() as i32 {
                    let ty = c.top + 60 + i * 36;
                    if y >= ty && y < ty + 32 {
                        NAV_HOVER = i;
                    }
                }
            }
            changed |= NAV_HOVER != old_nav;

            if TAB == 0 {
                let content_x = c.left + NAV_W + 1;
                let content_w = c.right - content_x;
                let col_y = c.top + 52;
                let row_h = 56;
                let old_hover = HOVER_ROW;
                HOVER_ROW = -1;
                let mut row = 0i32;
                for i in 0..PART_COUNT {
                    if PARTS[i].is_disk {
                        continue;
                    }
                    let ry = col_y + 24 + row * row_h - SCROLL_Y;
                    if y >= ry
                        && y < ry + row_h - 4
                        && x >= content_x + 12
                        && x < content_x + content_w - 12
                    {
                        HOVER_ROW = row;
                    }
                    row += 1;
                }
                changed |= HOVER_ROW != old_hover;

                // Boutons
                let btn_y = c.top + (c.bottom - c.top) - 52;
                let mount_r = rect(
                    content_x + content_w - 280,
                    btn_y,
                    content_x + content_w - 156,
                    btn_y + 32,
                );
                let unmount_r = rect(
                    content_x + content_w - 148,
                    btn_y,
                    content_x + content_w - 24,
                    btn_y + 32,
                );
                let old_m = MOUNT_HOVER;
                let old_u = UNMOUNT_HOVER;
                MOUNT_HOVER = inside(&mount_r, x, y);
                UNMOUNT_HOVER = inside(&unmount_r, x, y);
                changed |= MOUNT_HOVER != old_m || UNMOUNT_HOVER != old_u;
            }

            changed
        }
        Mouse::Wheel(delta) => {
            if TAB == 0 {
                SCROLL_Y = (SCROLL_Y - delta * 40).max(0);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

pub unsafe fn key(vk: u32, _ch: u16) -> bool {
    match vk {
        0x26 => {
            // Flèche haut
            if SELECTED_ROW > 0 {
                SELECTED_ROW -= 1;
                true
            } else {
                false
            }
        }
        0x28 => {
            // Flèche bas
            SELECTED_ROW += 1;
            true
        }
        0x09 => {
            // Tab
            TAB = (TAB + 1) % TAB_NAMES.len();
            SCROLL_Y = 0;
            true
        }
        _ => false,
    }
}

pub unsafe fn tick(_f: &Frame) -> bool {
    let old_count = PART_COUNT;
    refresh();
    PART_COUNT != old_count
}

// ---------- Actions montage/démontage ----------

unsafe fn selected_partition() -> Option<usize> {
    let mut row = 0i32;
    for i in 0..PART_COUNT {
        if PARTS[i].is_disk {
            continue;
        }
        if row == SELECTED_ROW {
            return Some(i);
        }
        row += 1;
    }
    None
}

unsafe fn do_mount() {
    if let Some(idx) = selected_partition() {
        let part = &PARTS[idx];
        if part.mount_len > 0 {
            return; // Déjà montée
        }
        // Écrire la commande dans Z:\run\winlux\disk-action.txt
        let mut cmd = [0u8; 128];
        let mut pos = 0;
        for &c in b"mount /dev/" {
            cmd[pos] = c;
            pos += 1;
        }
        for &c in &part.name[..part.name_len] {
            cmd[pos] = c;
            pos += 1;
        }
        cmd[pos] = b'\n';
        pos += 1;
        write_action(&cmd[..pos]);
    }
}

unsafe fn do_unmount() {
    if let Some(idx) = selected_partition() {
        let part = &PARTS[idx];
        if part.mount_len == 0 {
            return; // Pas montée
        }
        let mut cmd = [0u8; 128];
        let mut pos = 0;
        for &c in b"umount /dev/" {
            cmd[pos] = c;
            pos += 1;
        }
        for &c in &part.name[..part.name_len] {
            cmd[pos] = c;
            pos += 1;
        }
        cmd[pos] = b'\n';
        pos += 1;
        write_action(&cmd[..pos]);
    }
}

unsafe fn write_action(data: &[u8]) {
    let f = CreateFileA(
        c"Z:\\run\\winlux\\disk-action.txt".as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if f as isize != -1 && !f.is_null() {
        let mut written = 0u32;
        WriteFile(
            f,
            data.as_ptr(),
            data.len() as u32,
            &mut written,
            core::ptr::null_mut(),
        );
        CloseHandle(f);
    }
    LAST_REFRESH = 0;
}
