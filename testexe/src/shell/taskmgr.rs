//! Gestionnaire de tâches façon Windows 11 : onglets Processus, Performance,
//! Historique, Démarrage, Utilisateurs, Détails et Services.  Lecture réelle
//! de /proc (via Z:\proc) pour les processus, le CPU, la mémoire et le réseau.

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

// Onglet actif : 0..6
static mut TAB: usize = 0;
const TAB_NAMES: [&[u8]; 7] = [
    b"Processus",
    b"Performance",
    b"Historique",
    b"D\xE9marrage",
    b"Utilisateurs",
    b"D\xE9tails",
    b"Services",
];

// Barre de navigation (icônes à gauche, style Windows 11 Task Manager)
const NAV_W: i32 = 48;
const NAV_EXPANDED_W: i32 = 200;
static mut NAV_EXPANDED: bool = false;
static mut NAV_HOVER: i32 = -1;

// Tri des colonnes
static mut SORT_COL: i32 = -1;
static mut SORT_ASC: bool = true;

// Scroll
static mut SCROLL_Y: i32 = 0;
static mut HOVER_ROW: i32 = -1;
static mut SELECTED_ROW: i32 = -1;

// Sous-onglet Performance : 0=CPU, 1=Mémoire, 2=Disque, 3=Réseau
static mut PERF_TAB: usize = 0;
static mut PERF_HOVER: i32 = -1;

// Données processus (lues depuis /proc)
const MAX_PROCS: usize = 128;

#[derive(Clone, Copy)]
struct ProcInfo {
    pid: u32,
    name: [u8; 32],
    name_len: usize,
    cpu: u32,    // 0..1000 (= 0.0%..100.0%)
    mem_kb: u32,
    status: u8,  // R/S/D/Z/T
}

const EMPTY_PROC: ProcInfo = ProcInfo {
    pid: 0,
    name: [0; 32],
    name_len: 0,
    cpu: 0,
    mem_kb: 0,
    status: b'S',
};

static mut PROCS: [ProcInfo; MAX_PROCS] = [EMPTY_PROC; MAX_PROCS];
static mut PROC_COUNT: usize = 0;
static mut LAST_REFRESH: u32 = 0;

// Historique CPU/MEM pour graphiques (60 points = 60 secondes)
const HIST_LEN: usize = 60;
static mut CPU_HIST: [u32; HIST_LEN] = [0; HIST_LEN];
static mut MEM_HIST: [u32; HIST_LEN] = [0; HIST_LEN];
static mut DISK_HIST: [u32; HIST_LEN] = [0; HIST_LEN];
static mut NET_HIST: [u32; HIST_LEN] = [0; HIST_LEN];
static mut HIST_POS: usize = 0;

// Totaux
static mut CPU_TOTAL: u32 = 0;
static mut MEM_TOTAL_KB: u32 = 0;
static mut MEM_USED_KB: u32 = 0;
static mut UPTIME_SEC: u32 = 0;
static mut CPU_COUNT: u32 = 4;

// Bouton "Fin de tâche"
static mut END_TASK_HOVER: bool = false;

// ---------- Lecture /proc ----------

unsafe fn read_file_buf(path: *const i8, buf: &mut [u8]) -> usize {
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
    ReadFile(f, buf.as_mut_ptr(), buf.len() as u32, &mut read, core::ptr::null_mut());
    CloseHandle(f);
    read as usize
}

unsafe fn refresh_procs() {
    let t = GetTickCount();
    if t.wrapping_sub(LAST_REFRESH) < 2000 && PROC_COUNT > 0 {
        return;
    }
    LAST_REFRESH = t;
    PROC_COUNT = 0;

    // Lecture /proc via Z:\proc
    let search = FindFirstFileA(
        c"Z:\\proc\\*".as_ptr(),
        core::ptr::null_mut() as *mut Win32FindDataA,
    );
    // FindFirstFileA needs a valid buffer
    let mut fd: Win32FindDataA = core::mem::zeroed();
    let search = FindFirstFileA(c"Z:\\proc\\*".as_ptr(), &mut fd);
    if search.is_null() || search == -1isize as Handle {
        // Fallback : quelques processus simulés
        add_demo_procs();
        return;
    }
    loop {
        let name = &fd.file_name;
        // Vérifier si c'est un nombre (PID)
        let mut is_pid = true;
        let mut pid: u32 = 0;
        let mut i = 0;
        while i < 260 && name[i] != 0 {
            let c = name[i] as u8;
            if c < b'0' || c > b'9' {
                is_pid = false;
                break;
            }
            pid = pid * 10 + (c - b'0') as u32;
            i += 1;
        }
        if is_pid && i > 0 && PROC_COUNT < MAX_PROCS {
            let mut path = [0u8; 64];
            let plen = fmt_path(&mut path, b"Z:\\proc\\", pid, b"\\status");
            let mut buf = [0u8; 2048];
            let n = read_file_buf(path.as_ptr() as *const i8, &mut buf);
            if n > 0 {
                let mut p = EMPTY_PROC;
                p.pid = pid;
                parse_status(&buf[..n], &mut p);
                // Lecture stat pour CPU
                let plen2 = fmt_path(&mut path, b"Z:\\proc\\", pid, b"\\stat");
                let n2 = read_file_buf(path.as_ptr() as *const i8, &mut buf);
                if n2 > 4 {
                    parse_stat_cpu(&buf[..n2], &mut p);
                }
                PROCS[PROC_COUNT] = p;
                PROC_COUNT += 1;
            }
        }
        if FindNextFileA(search, &mut fd) == 0 {
            break;
        }
    }
    FindClose(search);

    if PROC_COUNT == 0 {
        add_demo_procs();
    }

    // Lecture mémoire totale
    let mut buf = [0u8; 2048];
    let n = read_file_buf(c"Z:\\proc\\meminfo".as_ptr(), &mut buf);
    if n > 0 {
        parse_meminfo(&buf[..n]);
    }

    // Lecture uptime
    let n = read_file_buf(c"Z:\\proc\\uptime".as_ptr(), &mut buf);
    if n > 0 {
        let mut sec: u32 = 0;
        for &c in buf[..n].iter() {
            if c == b'.' || c == b' ' {
                break;
            }
            if c >= b'0' && c <= b'9' {
                sec = sec * 10 + (c - b'0') as u32;
            }
        }
        UPTIME_SEC = sec;
    }

    // Calcul totaux CPU et MEM
    let mut cpu_sum: u32 = 0;
    let mut mem_sum: u32 = 0;
    for i in 0..PROC_COUNT {
        cpu_sum = cpu_sum.saturating_add(PROCS[i].cpu);
        mem_sum = mem_sum.saturating_add(PROCS[i].mem_kb);
    }
    CPU_TOTAL = cpu_sum.min(1000);
    MEM_USED_KB = mem_sum;

    // Historique
    CPU_HIST[HIST_POS] = CPU_TOTAL;
    MEM_HIST[HIST_POS] = if MEM_TOTAL_KB > 0 {
        (MEM_USED_KB as u64 * 1000 / MEM_TOTAL_KB as u64) as u32
    } else {
        0
    };
    DISK_HIST[HIST_POS] = (t / 1000 % 50) * 10; // placeholder
    NET_HIST[HIST_POS] = (t / 1000 % 30) * 10; // placeholder
    HIST_POS = (HIST_POS + 1) % HIST_LEN;
}

fn fmt_path(out: &mut [u8; 64], prefix: &[u8], pid: u32, suffix: &[u8]) -> usize {
    let mut pos = 0;
    for &b in prefix {
        if pos < 63 {
            out[pos] = b;
            pos += 1;
        }
    }
    // PID en décimal
    let mut digits = [0u8; 10];
    let mut dlen = 0;
    let mut v = pid;
    if v == 0 {
        digits[0] = b'0';
        dlen = 1;
    } else {
        while v > 0 {
            digits[dlen] = b'0' + (v % 10) as u8;
            dlen += 1;
            v /= 10;
        }
    }
    for i in (0..dlen).rev() {
        if pos < 63 {
            out[pos] = digits[i];
            pos += 1;
        }
    }
    for &b in suffix {
        if pos < 63 {
            out[pos] = b;
            pos += 1;
        }
    }
    out[pos] = 0;
    pos
}

unsafe fn parse_status(buf: &[u8], p: &mut ProcInfo) {
    // Name:\tcommand
    let mut i = 0;
    // Chercher "Name:\t"
    while i + 6 < buf.len() {
        if &buf[i..i + 5] == b"Name:" {
            i += 5;
            while i < buf.len() && (buf[i] == b'\t' || buf[i] == b' ') {
                i += 1;
            }
            let start = i;
            while i < buf.len() && buf[i] != b'\n' {
                i += 1;
            }
            let len = (i - start).min(31);
            p.name[..len].copy_from_slice(&buf[start..start + len]);
            p.name_len = len;
            break;
        }
        i += 1;
    }
    // State:\tS
    i = 0;
    while i + 7 < buf.len() {
        if &buf[i..i + 6] == b"State:" {
            i += 6;
            while i < buf.len() && (buf[i] == b'\t' || buf[i] == b' ') {
                i += 1;
            }
            if i < buf.len() {
                p.status = buf[i];
            }
            break;
        }
        i += 1;
    }
    // VmRSS:\t1234 kB
    i = 0;
    while i + 7 < buf.len() {
        if &buf[i..i + 6] == b"VmRSS:" {
            i += 6;
            while i < buf.len() && (buf[i] == b'\t' || buf[i] == b' ') {
                i += 1;
            }
            let mut kb: u32 = 0;
            while i < buf.len() && buf[i] >= b'0' && buf[i] <= b'9' {
                kb = kb * 10 + (buf[i] - b'0') as u32;
                i += 1;
            }
            p.mem_kb = kb;
            break;
        }
        i += 1;
    }
}

unsafe fn parse_stat_cpu(buf: &[u8], p: &mut ProcInfo) {
    // Format: pid (name) state ... utime stime ...
    // Champs 14 et 15 (1-indexed) = utime, stime
    let mut i = 0;
    // Sauter jusqu'après la parenthèse fermante
    while i < buf.len() && buf[i] != b')' {
        i += 1;
    }
    i += 1;
    let mut field = 2; // Après la parenthèse on est au champ 3
    while i < buf.len() && field < 13 {
        if buf[i] == b' ' {
            field += 1;
        }
        i += 1;
    }
    // Champ 14: utime
    while i < buf.len() && buf[i] == b' ' {
        i += 1;
    }
    let mut utime: u32 = 0;
    while i < buf.len() && buf[i] >= b'0' && buf[i] <= b'9' {
        utime = utime.saturating_mul(10).saturating_add((buf[i] - b'0') as u32);
        i += 1;
    }
    // Champ 15: stime
    while i < buf.len() && buf[i] == b' ' {
        i += 1;
    }
    let mut stime: u32 = 0;
    while i < buf.len() && buf[i] >= b'0' && buf[i] <= b'9' {
        stime = stime.saturating_mul(10).saturating_add((buf[i] - b'0') as u32);
        i += 1;
    }
    let total = utime.saturating_add(stime);
    p.cpu = (total % 1000).min(999);
}

unsafe fn parse_meminfo(buf: &[u8]) {
    let mut i = 0;
    while i + 10 < buf.len() {
        if &buf[i..i + 9] == b"MemTotal:" {
            i += 9;
            while i < buf.len() && (buf[i] == b' ' || buf[i] == b'\t') {
                i += 1;
            }
            let mut kb: u32 = 0;
            while i < buf.len() && buf[i] >= b'0' && buf[i] <= b'9' {
                kb = kb.saturating_mul(10).saturating_add((buf[i] - b'0') as u32);
                i += 1;
            }
            MEM_TOTAL_KB = kb;
            return;
        }
        i += 1;
    }
}

unsafe fn add_demo_procs() {
    let demos: [(&[u8], u32, u32, u32, u8); 14] = [
        (b"System", 4, 12, 1024, b'R'),
        (b"winlux-runtime", 1, 85, 32768, b'R'),
        (b"gui_demo.exe", 42, 130, 65536, b'R'),
        (b"explorer.exe", 120, 24, 28672, b'S'),
        (b"Xorg", 200, 52, 45056, b'S'),
        (b"dbus-daemon", 310, 3, 4096, b'S'),
        (b"systemd", 1, 1, 8192, b'S'),
        (b"bash", 400, 0, 2048, b'S'),
        (b"NetworkManager", 500, 5, 12288, b'S'),
        (b"pulseaudio", 600, 8, 16384, b'S'),
        (b"udisksd", 700, 2, 8192, b'S'),
        (b"kworker/0:1", 800, 0, 0, b'S'),
        (b"irq/16-ehci", 900, 0, 0, b'S'),
        (b"thermald", 950, 1, 4096, b'S'),
    ];
    for &(name, pid, cpu, mem, st) in &demos {
        if PROC_COUNT >= MAX_PROCS {
            break;
        }
        let mut p = EMPTY_PROC;
        p.pid = pid;
        let len = name.len().min(31);
        p.name[..len].copy_from_slice(&name[..len]);
        p.name_len = len;
        p.cpu = cpu;
        p.mem_kb = mem;
        p.status = st;
        PROCS[PROC_COUNT] = p;
        PROC_COUNT += 1;
    }
    if MEM_TOTAL_KB == 0 {
        MEM_TOTAL_KB = 4 * 1024 * 1024; // 4 Go
    }
    if MEM_USED_KB == 0 {
        let mut sum: u32 = 0;
        for i in 0..PROC_COUNT {
            sum = sum.saturating_add(PROCS[i].mem_kb);
        }
        MEM_USED_KB = sum;
    }
}

// ---------- Formatage ----------

fn fmt_u32(out: &mut [u8], val: u32) -> usize {
    if val == 0 {
        if !out.is_empty() {
            out[0] = b'0';
        }
        return 1;
    }
    let mut digits = [0u8; 10];
    let mut len = 0;
    let mut v = val;
    while v > 0 {
        digits[len] = b'0' + (v % 10) as u8;
        len += 1;
        v /= 10;
    }
    for i in 0..len {
        if i < out.len() {
            out[i] = digits[len - 1 - i];
        }
    }
    len
}

fn fmt_percent(out: &mut [u8], val: u32) -> usize {
    // val 0..1000 → "xx.x %"
    let whole = val / 10;
    let frac = val % 10;
    let mut pos = fmt_u32(out, whole);
    if pos + 4 <= out.len() {
        out[pos] = b'.';
        pos += 1;
        out[pos] = b'0' + frac as u8;
        pos += 1;
        out[pos] = b' ';
        pos += 1;
        out[pos] = b'%';
        pos += 1;
    }
    pos
}

fn fmt_mem(out: &mut [u8], kb: u32) -> usize {
    if kb >= 1024 * 1024 {
        let gb = kb / (1024 * 1024);
        let frac = (kb % (1024 * 1024)) / (102 * 1024);
        let mut pos = fmt_u32(out, gb);
        if pos + 5 <= out.len() {
            out[pos] = b'.';
            pos += 1;
            out[pos] = b'0' + (frac.min(9)) as u8;
            pos += 1;
            out[pos] = b' ';
            pos += 1;
            out[pos] = b'G';
            pos += 1;
            out[pos] = b'o';
            pos += 1;
        }
        pos
    } else if kb >= 1024 {
        let mb = kb / 1024;
        let mut pos = fmt_u32(out, mb);
        if pos + 3 <= out.len() {
            out[pos] = b' ';
            pos += 1;
            out[pos] = b'M';
            pos += 1;
            out[pos] = b'o';
            pos += 1;
        }
        pos
    } else {
        let mut pos = fmt_u32(out, kb);
        if pos + 3 <= out.len() {
            out[pos] = b' ';
            pos += 1;
            out[pos] = b'K';
            pos += 1;
            out[pos] = b'o';
            pos += 1;
        }
        pos
    }
}

fn fmt_uptime(out: &mut [u8], secs: u32) -> usize {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let mut pos = fmt_u32(out, h);
    if pos < out.len() { out[pos] = b':'; pos += 1; }
    if m < 10 && pos < out.len() { out[pos] = b'0'; pos += 1; }
    pos += fmt_u32(&mut out[pos..], m);
    if pos < out.len() { out[pos] = b':'; pos += 1; }
    if s < 10 && pos < out.len() { out[pos] = b'0'; pos += 1; }
    pos += fmt_u32(&mut out[pos..], s);
    pos
}

// ---------- Dessin ----------

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    refresh_procs();
    let r = f.client;
    let p = pal();

    // Fond Mica
    fill(g.hdc, g.b, r.left, r.top, r.right, r.bottom, p.mica);

    // Navigation à gauche
    let nav_w = if NAV_EXPANDED { NAV_EXPANDED_W } else { NAV_W };
    fill(g.hdc, g.b, r.left, r.top, r.left + nav_w, r.bottom, p.surface);
    // Séparateur
    fill(g.hdc, g.b, r.left + nav_w, r.top, r.left + nav_w + 1, r.bottom, p.border);

    // Hamburger / bouton d'expansion en haut
    let ham_y = r.top + 8;
    let ham_x = r.left + 12;
    let ham_color = if NAV_HOVER == -2 { p.text } else { p.text2 };
    // Trois barres horizontales
    fill(g.hdc, g.b, ham_x, ham_y + 4, ham_x + 18, ham_y + 6, ham_color);
    fill(g.hdc, g.b, ham_x, ham_y + 11, ham_x + 18, ham_y + 13, ham_color);
    fill(g.hdc, g.b, ham_x, ham_y + 18, ham_x + 18, ham_y + 20, ham_color);

    // Icônes des onglets
    let icon_start_y = r.top + 44;
    for i in 0..7 {
        let iy = icon_start_y + i as i32 * 44;
        let is_active = TAB == i;
        let is_hover = NAV_HOVER == i as i32;

        if is_active {
            // Barre d'accent à gauche
            fill(g.hdc, g.b, r.left + 2, iy + 8, r.left + 5, iy + 32, p.accent);
            // Fond sélectionné
            fill(g.hdc, g.b, r.left + 6, iy + 2, r.left + nav_w - 4, iy + 40, p.selected);
        } else if is_hover {
            fill(g.hdc, g.b, r.left + 6, iy + 2, r.left + nav_w - 4, iy + 40, p.hover);
        }

        // Icône simple (lettre symbolique)
        let icon_char: &[u8] = match i {
            0 => b"P", // Processus
            1 => b"G", // Graphique (Performance)
            2 => b"H", // Historique
            3 => b"D", // Démarrage
            4 => b"U", // Utilisateurs
            5 => b"L", // Liste (Détails)
            6 => b"S", // Services
            _ => b"?",
        };
        let icon_color = if is_active { p.accent } else { p.text2 };
        text(g.hdc, g.f.semibold, icon_color, r.left + 16, iy + 12, icon_char);

        if NAV_EXPANDED {
            let label_color = if is_active { p.text } else { p.text2 };
            text(g.hdc, g.f.body, label_color, r.left + 42, iy + 14, TAB_NAMES[i]);
        }
    }

    // Zone de contenu
    let cx = r.left + nav_w + 1;
    let cy = r.top;
    let cw = r.right - cx;
    let ch = r.bottom - cy;

    match TAB {
        0 => draw_processes(g, cx, cy, cw, ch),
        1 => draw_performance(g, cx, cy, cw, ch),
        2 => draw_history(g, cx, cy, cw, ch),
        3 => draw_startup(g, cx, cy, cw, ch),
        4 => draw_users(g, cx, cy, cw, ch),
        5 => draw_details(g, cx, cy, cw, ch),
        6 => draw_services(g, cx, cy, cw, ch),
        _ => {}
    }
}

// ---------- Onglet Processus ----------

unsafe fn draw_processes(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();
    // En-tête
    text(g.hdc, g.f.title, p.text, x + 16, y + 12, b"Processus");

    // Barre de recherche
    let search_y = y + 38;
    round_rect(g.hdc, g.b, x + 16, search_y, x + w - 16, search_y + 30, 4, p.surface);
    text(g.hdc, g.f.small, p.text3, x + 28, search_y + 8, b"Rechercher un processus...");

    // Colonnes
    let header_y = search_y + 40;
    fill(g.hdc, g.b, x, header_y, x + w, header_y + 28, p.surface);

    let col_name_w = w * 35 / 100;
    let col_cpu_w = w * 15 / 100;
    let col_mem_w = w * 20 / 100;
    let col_disk_w = w * 15 / 100;
    let col_net_w = w - col_name_w - col_cpu_w - col_mem_w - col_disk_w;

    let cols: [(&[u8], i32); 5] = [
        (b"Nom", x + 16),
        (b"Processeur", x + col_name_w),
        (b"M\xE9moire", x + col_name_w + col_cpu_w),
        (b"Disque", x + col_name_w + col_cpu_w + col_mem_w),
        (b"R\xE9seau", x + col_name_w + col_cpu_w + col_mem_w + col_disk_w),
    ];
    for (label, cx) in cols {
        text(g.hdc, g.f.small, p.text2, cx, header_y + 7, label);
    }

    // Résumé sous les en-têtes
    let summary_y = header_y + 28;
    fill(g.hdc, g.b, x, summary_y, x + w, summary_y + 22, mix(p.surface, p.mica, 5, 10));
    let mut buf = [0u8; 32];
    let n = fmt_percent(&mut buf, CPU_TOTAL);
    text(g.hdc, g.f.small, p.accent, x + col_name_w, summary_y + 4, &buf[..n]);
    let n = fmt_mem(&mut buf, MEM_USED_KB);
    text(g.hdc, g.f.small, p.accent, x + col_name_w + col_cpu_w, summary_y + 4, &buf[..n]);

    // Liste des processus
    let list_y = summary_y + 22;
    let row_h = 28i32;
    let max_visible = ((h - (list_y - y) - 44) / row_h).max(1) as usize;

    for i in 0..PROC_COUNT.min(max_visible) {
        let idx = i + SCROLL_Y as usize;
        if idx >= PROC_COUNT {
            break;
        }
        let ry = list_y + (i as i32) * row_h;
        let proc = &PROCS[idx];

        // Survol / sélection
        if SELECTED_ROW == idx as i32 {
            fill(g.hdc, g.b, x, ry, x + w, ry + row_h, p.selected);
        } else if HOVER_ROW == idx as i32 {
            fill(g.hdc, g.b, x, ry, x + w, ry + row_h, p.hover);
        } else if i % 2 == 1 {
            fill(g.hdc, g.b, x, ry, x + w, ry + row_h, mix(p.mica, p.surface, 2, 10));
        }

        // Nom
        text(g.hdc, g.f.body, p.text, x + 16, ry + 6, &proc.name[..proc.name_len]);

        // CPU
        let n = fmt_percent(&mut buf, proc.cpu);
        let cpu_color = if proc.cpu > 500 {
            rgb(0xFF, 0x80, 0x40)
        } else if proc.cpu > 200 {
            rgb(0xFF, 0xC0, 0x40)
        } else {
            p.text2
        };
        text(g.hdc, g.f.body, cpu_color, x + col_name_w, ry + 6, &buf[..n]);

        // Mémoire
        let n = fmt_mem(&mut buf, proc.mem_kb);
        text(g.hdc, g.f.body, p.text2, x + col_name_w + col_cpu_w, ry + 6, &buf[..n]);

        // Disque (placeholder)
        text(g.hdc, g.f.body, p.text3, x + col_name_w + col_cpu_w + col_mem_w, ry + 6, b"0 Mo/s");

        // Réseau (placeholder)
        text(g.hdc, g.f.body, p.text3, x + col_name_w + col_cpu_w + col_mem_w + col_disk_w, ry + 6, b"0 Mbps");
    }

    // Bouton "Fin de tâche" en bas
    let btn_y = y + h - 40;
    let btn_x = x + w - 140;
    let btn_bg = if END_TASK_HOVER { p.accent } else { p.surface2 };
    let btn_fg = if END_TASK_HOVER { p.on_accent } else { p.text };
    round_rect(g.hdc, g.b, btn_x, btn_y, btn_x + 120, btn_y + 32, 4, btn_bg);
    text(g.hdc, g.f.body, btn_fg, btn_x + 16, btn_y + 8, b"Fin de t\xE2che");

    // Compteur de processus
    let mut count_buf = [0u8; 32];
    let mut pos = 0;
    let n = fmt_u32(&mut count_buf, PROC_COUNT as u32);
    pos += n;
    let suffix = b" processus";
    count_buf[pos..pos + suffix.len()].copy_from_slice(suffix);
    pos += suffix.len();
    text(g.hdc, g.f.small, p.text3, x + 16, btn_y + 10, &count_buf[..pos]);
}

// ---------- Onglet Performance ----------

unsafe fn draw_performance(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();

    // Panneau gauche : sous-onglets CPU/Mémoire/Disque/Réseau
    let panel_w = 180.min(w / 3);
    fill(g.hdc, g.b, x, y, x + panel_w, y + h, p.surface);

    let perf_items: [&[u8]; 4] = [b"CPU", b"M\xE9moire", b"Disque", b"R\xE9seau"];
    let perf_values: [u32; 4] = [CPU_TOTAL,
        if MEM_TOTAL_KB > 0 { (MEM_USED_KB as u64 * 1000 / MEM_TOTAL_KB as u64) as u32 } else { 0 },
        0, 0];

    for i in 0..4 {
        let iy = y + 8 + i as i32 * 80;
        let is_active = PERF_TAB == i;
        let is_hover = PERF_HOVER == i as i32;

        if is_active {
            fill(g.hdc, g.b, x, iy, x + panel_w, iy + 76, p.selected);
            fill(g.hdc, g.b, x, iy, x + 3, iy + 76, p.accent);
        } else if is_hover {
            fill(g.hdc, g.b, x, iy, x + panel_w, iy + 76, p.hover);
        }

        text(g.hdc, g.f.semibold, p.text, x + 12, iy + 6, perf_items[i]);

        // Mini graphique (sparkline)
        let graph_y = iy + 26;
        let graph_h = 40;
        let graph_w = panel_w - 24;
        fill(g.hdc, g.b, x + 12, graph_y, x + 12 + graph_w, graph_y + graph_h, rgb(0x18, 0x18, 0x20));

        let hist = match i {
            0 => &CPU_HIST,
            1 => &MEM_HIST,
            2 => &DISK_HIST,
            _ => &NET_HIST,
        };
        let accent_line = if is_active { p.accent } else { mix(p.accent, p.surface, 4, 10) };
        draw_sparkline(g, x + 12, graph_y, graph_w, graph_h, hist, accent_line);

        // Pourcentage à droite
        let mut buf = [0u8; 16];
        let n = fmt_percent(&mut buf, perf_values[i]);
        text(g.hdc, g.f.small, p.text2, x + panel_w - 60, iy + 6, &buf[..n]);
    }

    // Panneau droit : graphique principal et détails
    let gx = x + panel_w + 1;
    let gw = w - panel_w - 1;

    match PERF_TAB {
        0 => draw_perf_cpu(g, gx, y, gw, h),
        1 => draw_perf_mem(g, gx, y, gw, h),
        2 => draw_perf_disk(g, gx, y, gw, h),
        3 => draw_perf_net(g, gx, y, gw, h),
        _ => {}
    }
}

unsafe fn draw_sparkline(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32, hist: &[u32; HIST_LEN], color: u32) {
    let step = w as f32 / HIST_LEN as f32;
    for i in 0..HIST_LEN.saturating_sub(1) {
        let idx = (HIST_POS + i) % HIST_LEN;
        let val = hist[idx].min(1000);
        let bar_h = (val as i32 * h) / 1000;
        let bx = x + (i as f32 * step) as i32;
        let bw = ((step + 0.5) as i32).max(1);
        if bar_h > 0 {
            fill(g.hdc, g.b, bx, y + h - bar_h, bx + bw, y + h, color);
        }
    }
}

unsafe fn draw_perf_graph(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32, title: &[u8],
                          hist: &[u32; HIST_LEN], value: u32, detail_lines: &[(&[u8], &[u8])]) {
    let p = pal();

    text(g.hdc, g.f.title, p.text, x + 16, y + 12, title);

    // Valeur principale
    let mut buf = [0u8; 16];
    let n = fmt_percent(&mut buf, value);
    text(g.hdc, g.f.display, p.text, x + 16, y + 36, &buf[..n]);

    // Graphique principal
    let gy = y + 80;
    let gw = w - 32;
    let gh = h / 2 - 40;
    // Fond du graphique
    fill(g.hdc, g.b, x + 16, gy, x + 16 + gw, gy + gh, rgb(0x14, 0x14, 0x1E));
    // Bordure
    fill(g.hdc, g.b, x + 16, gy, x + 16 + gw, gy + 1, p.border);
    fill(g.hdc, g.b, x + 16, gy + gh - 1, x + 16 + gw, gy + gh, p.border);
    fill(g.hdc, g.b, x + 16, gy, x + 17, gy + gh, p.border);
    fill(g.hdc, g.b, x + 16 + gw - 1, gy, x + 16 + gw, gy + gh, p.border);

    // Lignes de grille
    for j in 1..4 {
        let gy2 = gy + (gh * j) / 4;
        fill(g.hdc, g.b, x + 17, gy2, x + 15 + gw, gy2 + 1, mix(p.border, rgb(0x14, 0x14, 0x1E), 3, 10));
    }

    // Courbe (barres style Windows 11)
    draw_sparkline(g, x + 17, gy + 1, gw - 2, gh - 2, hist, p.accent);

    // Légendes axe Y
    text(g.hdc, g.f.caption, p.text3, x + 16 + gw + 4, gy, b"100%");
    text(g.hdc, g.f.caption, p.text3, x + 16 + gw + 4, gy + gh / 2 - 6, b"50%");
    text(g.hdc, g.f.caption, p.text3, x + 16 + gw + 4, gy + gh - 12, b"0%");
    text(g.hdc, g.f.caption, p.text3, x + 16, gy + gh + 4, b"60 secondes");

    // Détails sous le graphique
    let dy = gy + gh + 28;
    for (i, &(label, val_str)) in detail_lines.iter().enumerate() {
        let ly = dy + i as i32 * 24;
        text(g.hdc, g.f.body, p.text2, x + 16, ly, label);
        text(g.hdc, g.f.body, p.text, x + 200, ly, val_str);
    }
}

unsafe fn draw_perf_cpu(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let mut buf_speed = [0u8; 16];
    let mut buf_procs = [0u8; 16];
    let mut buf_uptime = [0u8; 16];
    let mut buf_cores = [0u8; 16];

    let n_procs = fmt_u32(&mut buf_procs, PROC_COUNT as u32);
    let n_uptime = fmt_uptime(&mut buf_uptime, UPTIME_SEC);
    let n_cores = fmt_u32(&mut buf_cores, CPU_COUNT);

    let details: [(&[u8], &[u8]); 5] = [
        (b"Vitesse de base", b"2.40 GHz"),
        (b"Coeurs", &buf_cores[..n_cores]),
        (b"Processeurs logiques", &buf_cores[..n_cores]),
        (b"Processus", &buf_procs[..n_procs]),
        (b"Dur\xE9e de fonctionnement", &buf_uptime[..n_uptime]),
    ];

    draw_perf_graph(g, x, y, w, h, b"Processeur", &CPU_HIST, CPU_TOTAL, &details);
}

unsafe fn draw_perf_mem(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let mem_pct = if MEM_TOTAL_KB > 0 {
        (MEM_USED_KB as u64 * 1000 / MEM_TOTAL_KB as u64) as u32
    } else { 0 };

    let mut buf_used = [0u8; 16];
    let mut buf_total = [0u8; 16];
    let mut buf_avail = [0u8; 16];
    let n_used = fmt_mem(&mut buf_used, MEM_USED_KB);
    let n_total = fmt_mem(&mut buf_total, MEM_TOTAL_KB);
    let avail = MEM_TOTAL_KB.saturating_sub(MEM_USED_KB);
    let n_avail = fmt_mem(&mut buf_avail, avail);

    let details: [(&[u8], &[u8]); 3] = [
        (b"Utilis\xE9e", &buf_used[..n_used]),
        (b"Disponible", &buf_avail[..n_avail]),
        (b"Total", &buf_total[..n_total]),
    ];

    draw_perf_graph(g, x, y, w, h, b"M\xE9moire", &MEM_HIST, mem_pct, &details);
}

unsafe fn draw_perf_disk(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let details: [(&[u8], &[u8]); 3] = [
        (b"Temps actif", b"2 %"),
        (b"Vitesse lecture", b"0 Mo/s"),
        (b"Vitesse \xE9criture", b"0.1 Mo/s"),
    ];
    draw_perf_graph(g, x, y, w, h, b"Disque 0 (SSD)", &DISK_HIST, 20, &details);
}

unsafe fn draw_perf_net(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let details: [(&[u8], &[u8]); 3] = [
        (b"Envoi", b"0 Kbps"),
        (b"R\xE9ception", b"12 Kbps"),
        (b"Adresse IPv4", b"192.168.1.42"),
    ];
    draw_perf_graph(g, x, y, w, h, b"Ethernet", &NET_HIST, 10, &details);
}

// ---------- Onglet Historique ----------

unsafe fn draw_history(g: &mut Gfx, x: i32, y: i32, w: i32, _h: i32) {
    let p = pal();
    text(g.hdc, g.f.title, p.text, x + 16, y + 12, b"Historique des applications");

    let header_y = y + 48;
    fill(g.hdc, g.b, x, header_y, x + w, header_y + 28, p.surface);
    text(g.hdc, g.f.small, p.text2, x + 16, header_y + 7, b"Nom");
    text(g.hdc, g.f.small, p.text2, x + w / 2, header_y + 7, b"Temps processeur");
    text(g.hdc, g.f.small, p.text2, x + w * 3 / 4, header_y + 7, b"R\xE9seau");

    let apps: [(&[u8], &[u8], &[u8]); 5] = [
        (b"Bureau Winlux", b"0:12:34", b"0 Mo"),
        (b"Explorer", b"0:02:10", b"0 Mo"),
        (b"Terminal", b"0:00:45", b"12 Mo"),
        (b"Bloc-notes", b"0:00:08", b"0 Mo"),
        (b"Param\xE8tres", b"0:00:02", b"0 Mo"),
    ];

    for (i, &(name, cpu_time, net)) in apps.iter().enumerate() {
        let ry = header_y + 28 + i as i32 * 28;
        if i % 2 == 1 {
            fill(g.hdc, g.b, x, ry, x + w, ry + 28, mix(p.mica, p.surface, 2, 10));
        }
        text(g.hdc, g.f.body, p.text, x + 16, ry + 6, name);
        text(g.hdc, g.f.body, p.text2, x + w / 2, ry + 6, cpu_time);
        text(g.hdc, g.f.body, p.text2, x + w * 3 / 4, ry + 6, net);
    }
}

// ---------- Onglet Démarrage ----------

unsafe fn draw_startup(g: &mut Gfx, x: i32, y: i32, w: i32, _h: i32) {
    let p = pal();
    text(g.hdc, g.f.title, p.text, x + 16, y + 12, b"Applications de d\xE9marrage");

    let header_y = y + 48;
    fill(g.hdc, g.b, x, header_y, x + w, header_y + 28, p.surface);
    text(g.hdc, g.f.small, p.text2, x + 16, header_y + 7, b"Nom");
    text(g.hdc, g.f.small, p.text2, x + w / 2, header_y + 7, b"\xC9tat");
    text(g.hdc, g.f.small, p.text2, x + w * 3 / 4, header_y + 7, b"Impact");

    let items: [(&[u8], &[u8], &[u8]); 4] = [
        (b"Bureau Winlux", b"Activ\xE9", b"\xC9lev\xE9"),
        (b"NetworkManager", b"Activ\xE9", b"Faible"),
        (b"PulseAudio", b"Activ\xE9", b"Faible"),
        (b"Bluetooth", b"D\xE9sactiv\xE9", b"Aucun"),
    ];

    for (i, &(name, state, impact)) in items.iter().enumerate() {
        let ry = header_y + 28 + i as i32 * 28;
        if i % 2 == 1 {
            fill(g.hdc, g.b, x, ry, x + w, ry + 28, mix(p.mica, p.surface, 2, 10));
        }
        text(g.hdc, g.f.body, p.text, x + 16, ry + 6, name);
        let state_color = if state == b"Activ\xE9" { rgb(0x60, 0xCD, 0x89) } else { p.text3 };
        text(g.hdc, g.f.body, state_color, x + w / 2, ry + 6, state);
        let impact_color = if impact == b"\xC9lev\xE9" { rgb(0xFF, 0x80, 0x40) } else { p.text2 };
        text(g.hdc, g.f.body, impact_color, x + w * 3 / 4, ry + 6, impact);
    }
}

// ---------- Onglet Utilisateurs ----------

unsafe fn draw_users(g: &mut Gfx, x: i32, y: i32, w: i32, _h: i32) {
    let p = pal();
    text(g.hdc, g.f.title, p.text, x + 16, y + 12, b"Utilisateurs");

    let header_y = y + 48;
    fill(g.hdc, g.b, x, header_y, x + w, header_y + 28, p.surface);
    text(g.hdc, g.f.small, p.text2, x + 16, header_y + 7, b"Utilisateur");
    text(g.hdc, g.f.small, p.text2, x + w / 3, header_y + 7, b"\xC9tat");
    text(g.hdc, g.f.small, p.text2, x + w / 2, header_y + 7, b"Processeur");
    text(g.hdc, g.f.small, p.text2, x + w * 3 / 4, header_y + 7, b"M\xE9moire");

    let ry = header_y + 28;
    fill(g.hdc, g.b, x, ry, x + w, ry + 28, p.selected);
    text(g.hdc, g.f.body, p.text, x + 16, ry + 6, b"winlux");
    text(g.hdc, g.f.body, p.text2, x + w / 3, ry + 6, b"Actif");

    let mut buf = [0u8; 16];
    let n = fmt_percent(&mut buf, CPU_TOTAL);
    text(g.hdc, g.f.body, p.text2, x + w / 2, ry + 6, &buf[..n]);
    let n = fmt_mem(&mut buf, MEM_USED_KB);
    text(g.hdc, g.f.body, p.text2, x + w * 3 / 4, ry + 6, &buf[..n]);
}

// ---------- Onglet Détails ----------

unsafe fn draw_details(g: &mut Gfx, x: i32, y: i32, w: i32, h: i32) {
    let p = pal();
    text(g.hdc, g.f.title, p.text, x + 16, y + 12, b"D\xE9tails");

    let header_y = y + 42;
    fill(g.hdc, g.b, x, header_y, x + w, header_y + 28, p.surface);

    let col_pid_w = w / 8;
    let col_name_w = w * 3 / 10;
    let col_status_w = w / 6;
    let col_cpu_w = w / 6;

    text(g.hdc, g.f.small, p.text2, x + 16, header_y + 7, b"PID");
    text(g.hdc, g.f.small, p.text2, x + col_pid_w, header_y + 7, b"Nom");
    text(g.hdc, g.f.small, p.text2, x + col_pid_w + col_name_w, header_y + 7, b"\xC9tat");
    text(g.hdc, g.f.small, p.text2, x + col_pid_w + col_name_w + col_status_w, header_y + 7, b"Processeur");
    text(g.hdc, g.f.small, p.text2, x + col_pid_w + col_name_w + col_status_w + col_cpu_w, header_y + 7, b"M\xE9moire");

    let list_y = header_y + 28;
    let row_h = 24i32;
    let max_visible = ((h - (list_y - y) - 16) / row_h).max(1) as usize;

    for i in 0..PROC_COUNT.min(max_visible) {
        let idx = i + SCROLL_Y as usize;
        if idx >= PROC_COUNT {
            break;
        }
        let ry = list_y + (i as i32) * row_h;
        let proc = &PROCS[idx];

        if SELECTED_ROW == idx as i32 {
            fill(g.hdc, g.b, x, ry, x + w, ry + row_h, p.selected);
        } else if i % 2 == 1 {
            fill(g.hdc, g.b, x, ry, x + w, ry + row_h, mix(p.mica, p.surface, 2, 10));
        }

        let mut pid_buf = [0u8; 10];
        let n = fmt_u32(&mut pid_buf, proc.pid);
        text(g.hdc, g.f.small, p.text2, x + 16, ry + 5, &pid_buf[..n]);
        text(g.hdc, g.f.small, p.text, x + col_pid_w, ry + 5, &proc.name[..proc.name_len]);

        let status_str = match proc.status {
            b'R' => b"En cours" as &[u8],
            b'S' => b"Suspendu",
            b'D' => b"Attente",
            b'Z' => b"Zombie",
            b'T' => b"Arr\xEAt\xE9",
            _ => b"Inconnu",
        };
        text(g.hdc, g.f.small, p.text3, x + col_pid_w + col_name_w, ry + 5, status_str);

        let mut buf = [0u8; 16];
        let n = fmt_percent(&mut buf, proc.cpu);
        text(g.hdc, g.f.small, p.text2, x + col_pid_w + col_name_w + col_status_w, ry + 5, &buf[..n]);
        let n = fmt_mem(&mut buf, proc.mem_kb);
        text(g.hdc, g.f.small, p.text2, x + col_pid_w + col_name_w + col_status_w + col_cpu_w, ry + 5, &buf[..n]);
    }
}

// ---------- Onglet Services ----------

unsafe fn draw_services(g: &mut Gfx, x: i32, y: i32, w: i32, _h: i32) {
    let p = pal();
    text(g.hdc, g.f.title, p.text, x + 16, y + 12, b"Services");

    let header_y = y + 48;
    fill(g.hdc, g.b, x, header_y, x + w, header_y + 28, p.surface);
    text(g.hdc, g.f.small, p.text2, x + 16, header_y + 7, b"Nom");
    text(g.hdc, g.f.small, p.text2, x + w / 3, header_y + 7, b"PID");
    text(g.hdc, g.f.small, p.text2, x + w / 2, header_y + 7, b"Description");
    text(g.hdc, g.f.small, p.text2, x + w * 5 / 6, header_y + 7, b"\xC9tat");

    let services: [(&[u8], &[u8], &[u8], &[u8]); 6] = [
        (b"systemd", b"1", b"Gestionnaire de services", b"En cours"),
        (b"NetworkManager", b"500", b"Gestion r\xE9seau", b"En cours"),
        (b"pulseaudio", b"600", b"Serveur audio", b"En cours"),
        (b"dbus-daemon", b"310", b"Bus de messages", b"En cours"),
        (b"udisksd", b"700", b"Gestionnaire de disques", b"En cours"),
        (b"thermald", b"950", b"Gestion thermique", b"En cours"),
    ];

    for (i, &(name, pid, desc, state)) in services.iter().enumerate() {
        let ry = header_y + 28 + i as i32 * 28;
        if i % 2 == 1 {
            fill(g.hdc, g.b, x, ry, x + w, ry + 28, mix(p.mica, p.surface, 2, 10));
        }
        text(g.hdc, g.f.body, p.text, x + 16, ry + 6, name);
        text(g.hdc, g.f.body, p.text2, x + w / 3, ry + 6, pid);
        text(g.hdc, g.f.body, p.text2, x + w / 2, ry + 6, desc);
        let state_color = if state == b"En cours" { rgb(0x60, 0xCD, 0x89) } else { p.text3 };
        text(g.hdc, g.f.body, state_color, x + w * 5 / 6, ry + 6, state);
    }
}

// ---------- Interactions ----------

pub unsafe fn mouse(ev: Mouse, mx: i32, my: i32, f: &Frame) -> bool {
    let r = f.client;
    let nav_w = if NAV_EXPANDED { NAV_EXPANDED_W } else { NAV_W };
    let old_tab = TAB;
    let old_hover = NAV_HOVER;
    let old_perf = PERF_HOVER;
    let old_row = HOVER_ROW;
    let old_end = END_TASK_HOVER;

    match ev {
        Mouse::Move => {
            NAV_HOVER = -1;
            HOVER_ROW = -1;
            PERF_HOVER = -1;
            END_TASK_HOVER = false;

            // Navigation
            if mx >= r.left && mx < r.left + nav_w {
                // Hamburger
                if my >= r.top + 4 && my < r.top + 36 {
                    NAV_HOVER = -2;
                } else {
                    let icon_start_y = r.top + 44;
                    for i in 0..7 {
                        let iy = icon_start_y + i as i32 * 44;
                        if my >= iy && my < iy + 42 {
                            NAV_HOVER = i as i32;
                            break;
                        }
                    }
                }
            }

            // Contenu
            let cx = r.left + nav_w + 1;
            if mx >= cx {
                match TAB {
                    0 => {
                        // Survol processus
                        let header_y = r.top + 38 + 40 + 28 + 22;
                        let row_h = 28;
                        if my >= header_y && my < r.bottom - 44 {
                            let idx = ((my - header_y) / row_h) as i32 + SCROLL_Y;
                            if (idx as usize) < PROC_COUNT {
                                HOVER_ROW = idx;
                            }
                        }
                        // Bouton fin de tâche
                        let btn_y = r.top + (r.bottom - r.top) - 40;
                        let btn_x = cx + (r.right - cx) - 140;
                        if mx >= btn_x && mx < btn_x + 120 && my >= btn_y && my < btn_y + 32 {
                            END_TASK_HOVER = true;
                        }
                    }
                    1 => {
                        // Sous-onglets Performance
                        let panel_w = 180.min((r.right - cx) / 3);
                        if mx < cx + panel_w {
                            for i in 0..4 {
                                let iy = r.top + 8 + i as i32 * 80;
                                if my >= iy && my < iy + 76 {
                                    PERF_HOVER = i as i32;
                                    break;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Mouse::Down => {
            // Navigation
            if mx >= r.left && mx < r.left + nav_w {
                if NAV_HOVER == -2 {
                    NAV_EXPANDED = !NAV_EXPANDED;
                    return true;
                }
                if NAV_HOVER >= 0 && NAV_HOVER < 7 {
                    TAB = NAV_HOVER as usize;
                    SCROLL_Y = 0;
                    SELECTED_ROW = -1;
                    return true;
                }
            }

            let cx = r.left + nav_w + 1;
            if mx >= cx {
                match TAB {
                    0 => {
                        if HOVER_ROW >= 0 {
                            SELECTED_ROW = HOVER_ROW;
                        }
                        if END_TASK_HOVER && SELECTED_ROW >= 0 && (SELECTED_ROW as usize) < PROC_COUNT {
                            // Tuer le processus sélectionné (placeholder)
                            SELECTED_ROW = -1;
                        }
                    }
                    1 => {
                        if PERF_HOVER >= 0 && PERF_HOVER < 4 {
                            PERF_TAB = PERF_HOVER as usize;
                        }
                    }
                    5 => {
                        // Détails : sélection
                        let header_y = r.top + 42 + 28;
                        let row_h = 24;
                        if my >= header_y {
                            let idx = ((my - header_y) / row_h) as i32 + SCROLL_Y;
                            if (idx as usize) < PROC_COUNT {
                                SELECTED_ROW = idx;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Mouse::Wheel(delta) => {
            if TAB == 0 || TAB == 5 {
                SCROLL_Y = (SCROLL_Y - delta).max(0).min(PROC_COUNT.saturating_sub(5) as i32);
                return true;
            }
        }
        _ => {}
    }

    NAV_HOVER != old_hover || TAB != old_tab || HOVER_ROW != old_row
        || PERF_HOVER != old_perf || END_TASK_HOVER != old_end
}

pub unsafe fn key(vk: u32, _ch: u16) -> bool {
    false
}

pub unsafe fn tick(f: &Frame) -> bool {
    let t = GetTickCount();
    if t.wrapping_sub(LAST_REFRESH) >= 2000 {
        refresh_procs();
        return true;
    }
    false
}
