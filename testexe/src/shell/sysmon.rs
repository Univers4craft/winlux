//! Mesures système réelles pour le Gestionnaire des tâches : processus,
//! processeur, mémoire, disques, cartes réseau, utilisateurs, applications
//! de démarrage et services.  Tout vient de /proc, /sys et /etc (via Z:\),
//! sans valeur inventée : une mesure indisponible reste absente et
//! l'interface masque la ligne correspondante.
#![allow(dead_code)]

use crate::admin_ui::{read_file, Path};
use winlux_testexe::*;

pub const HIST: usize = 60;
pub const MAX_PROCS: usize = 512;
pub const MAX_DISKS: usize = 4;
pub const MAX_NETS: usize = 3;

/// Nature d'un processus pour les groupes de l'onglet Processus.
pub const KIND_KERNEL: u8 = 0;
/// Programme Windows (.exe) lancé par Winlux.
pub const KIND_EXE: u8 = 1;
/// Application intégrée de Winlux (Explorateur, Paramètres…).
pub const KIND_SHELL: u8 = 2;
/// Programme Linux ordinaire.
pub const KIND_LINUX: u8 = 3;

/// Débit d'E/S inconnu (fichier /proc/PID/io illisible).
pub const UNKNOWN: u32 = u32::MAX;

#[derive(Clone, Copy)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub uid: u32,
    /// Date de lancement en tops d'horloge : avec le PID, identifie le processus.
    pub start: u64,
    pub ticks: u64,
    /// Tops de processeur consommés depuis la mesure précédente.
    pub dticks: u64,
    /// Vu pour la première fois à cette mesure.
    pub fresh: bool,
    pub io: u64,
    pub name: [u8; 48],
    pub name_len: u8,
    pub cmd: [u8; 200],
    pub cmd_len: u8,
    pub kind: u8,
    pub state: u8,
    pub nice: i8,
    pub threads: u32,
    /// Part du processeur en pour mille de toute la machine.
    pub cpu: u32,
    pub mem_kb: u32,
    pub io_bps: u32,
}

impl Proc {
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len as usize]
    }
    pub fn cmd(&self) -> &[u8] {
        &self.cmd[..self.cmd_len as usize]
    }
}

const EMPTY: Proc = Proc {
    pid: 0,
    ppid: 0,
    uid: 0,
    start: 0,
    ticks: 0,
    dticks: 0,
    fresh: false,
    io: 0,
    name: [0; 48],
    name_len: 0,
    cmd: [0; 200],
    cmd_len: 0,
    kind: KIND_LINUX,
    state: b'S',
    nice: 0,
    threads: 1,
    cpu: 0,
    mem_kb: 0,
    io_bps: UNKNOWN,
};

pub static mut PROCS: [Proc; MAX_PROCS] = [EMPTY; MAX_PROCS];
pub static mut COUNT: usize = 0;
static mut PREV: [Proc; MAX_PROCS] = [EMPTY; MAX_PROCS];
static mut PREV_COUNT: usize = 0;

// ---------- Processeur ----------

pub struct CpuInfo {
    pub model: [u8; 64],
    pub model_len: usize,
    pub logical: u32,
    pub cores: u32,
    pub sockets: u32,
    pub base_mhz: u32,
    pub max_mhz: u32,
    pub l1_kb: u32,
    pub l2_kb: u32,
    pub l3_kb: u32,
    pub virtual_machine: bool,
}

pub static mut CPU: CpuInfo = CpuInfo {
    model: [0; 64],
    model_len: 0,
    logical: 1,
    cores: 0,
    sockets: 0,
    base_mhz: 0,
    max_mhz: 0,
    l1_kb: 0,
    l2_kb: 0,
    l3_kb: 0,
    virtual_machine: false,
};
/// Utilisation totale (pour mille) et fréquence actuelle moyenne.
pub static mut CPU_LOAD: u32 = 0;
pub static mut CPU_MHZ: u32 = 0;
pub static mut CPU_HIST: [u32; HIST] = [0; HIST];
static mut CPU_PREV_TOTAL: u64 = 0;
static mut CPU_PREV_IDLE: u64 = 0;
static mut CPU_DELTA: u64 = 0;

// ---------- Mémoire ----------

pub static mut MEM_TOTAL_KB: u64 = 0;
pub static mut MEM_AVAIL_KB: u64 = 0;
pub static mut MEM_CACHED_KB: u64 = 0;
pub static mut SWAP_TOTAL_KB: u64 = 0;
pub static mut SWAP_FREE_KB: u64 = 0;
pub static mut COMMIT_KB: u64 = 0;
pub static mut COMMIT_LIMIT_KB: u64 = 0;
pub static mut MEM_HIST: [u32; HIST] = [0; HIST];

// ---------- Disques ----------

#[derive(Clone, Copy)]
pub struct Disk {
    pub name: [u8; 16],
    pub name_len: usize,
    pub model: [u8; 48],
    pub model_len: usize,
    pub ssd: bool,
    pub removable: bool,
    pub size_bytes: u64,
    prev_read: u64,
    prev_write: u64,
    prev_busy: u64,
    pub read_bps: u64,
    pub write_bps: u64,
    /// Temps actif en pour mille.
    pub active: u32,
    pub hist: [u32; HIST],
}

const NO_DISK: Disk = Disk {
    name: [0; 16],
    name_len: 0,
    model: [0; 48],
    model_len: 0,
    ssd: false,
    removable: false,
    size_bytes: 0,
    prev_read: 0,
    prev_write: 0,
    prev_busy: 0,
    read_bps: 0,
    write_bps: 0,
    active: 0,
    hist: [0; HIST],
};

impl Disk {
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
    pub fn model(&self) -> &[u8] {
        &self.model[..self.model_len]
    }
}

pub static mut DISKS: [Disk; MAX_DISKS] = [NO_DISK; MAX_DISKS];
pub static mut DISK_COUNT: usize = 0;

// ---------- Réseau ----------

#[derive(Clone, Copy)]
pub struct Net {
    pub name: [u8; 16],
    pub name_len: usize,
    pub wifi: bool,
    pub speed_mbps: u32,
    prev_rx: u64,
    prev_tx: u64,
    pub rx_bps: u64,
    pub tx_bps: u64,
    pub mac: [u8; 17],
    /// Débit total (bits/s) ; l'échelle du graphique suit le maximum.
    pub hist: [u32; HIST],
}

const NO_NET: Net = Net {
    name: [0; 16],
    name_len: 0,
    wifi: false,
    speed_mbps: 0,
    prev_rx: 0,
    prev_tx: 0,
    rx_bps: 0,
    tx_bps: 0,
    mac: [0; 17],
    hist: [0; HIST],
};

impl Net {
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
}

pub static mut NETS: [Net; MAX_NETS] = [NO_NET; MAX_NETS];
pub static mut NET_COUNT: usize = 0;

// ---------- État général ----------

pub static mut HIST_POS: usize = 0;
pub static mut UPTIME_SEC: u64 = 0;
pub static mut THREADS: u32 = 0;
pub static mut MY_UID: u32 = u32::MAX;
static mut LAST: u64 = 0;
static mut SAMPLES: u32 = 0;
static mut STATIC_DONE: bool = false;

/// Intervalle d'actualisation (ms) : « Normale » dans Windows.
pub static mut INTERVAL_MS: u32 = 1000;
/// Actualisation suspendue (menu Vitesse d'actualisation > En pause).
pub static mut PAUSED: bool = false;

// ---------- Lecture de fichiers ----------

/// Chemin Linux (« /proc/1/stat ») vers le chemin Winlux (« Z:\proc\1\stat »).
pub fn zpath(parts: &[&[u8]]) -> Path {
    let mut p = Path::new(b"Z:");
    for part in parts {
        for &c in part.iter() {
            p.push(if c == b'/' {
                b"\\"
            } else {
                core::slice::from_ref(&c)
            });
        }
    }
    p
}

pub unsafe fn read(parts: &[&[u8]], buf: &mut [u8]) -> usize {
    read_file(zpath(parts).ptr(), buf, false)
}

pub unsafe fn exists(parts: &[&[u8]]) -> bool {
    GetFileAttributesA(zpath(parts).ptr()) != u32::MAX
}

/// Liste les entrées d'un dossier Linux ; `f` reçoit chaque nom.
pub unsafe fn list_dir(dir: &[u8], f: &mut dyn FnMut(&[u8])) {
    let pattern = zpath(&[dir, b"/*"]);
    let mut fd: Win32FindDataA = core::mem::zeroed();
    let h = FindFirstFileA(pattern.ptr(), &mut fd);
    if h.is_null() || h == -1isize as Handle {
        return;
    }
    loop {
        let name = &*(&fd.file_name as *const [i8; 260] as *const [u8; 260]);
        let n = name.iter().position(|&c| c == 0).unwrap_or(260);
        if n > 0 && name[0] != b'.' {
            f(&name[..n]);
        }
        if FindNextFileA(h, &mut fd) == 0 {
            break;
        }
    }
    FindClose(h);
}

pub fn num(s: &[u8]) -> u64 {
    let mut v: u64 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            break;
        }
        v = v.saturating_mul(10).saturating_add((c - b'0') as u64);
    }
    v
}

fn signed(s: &[u8]) -> i64 {
    match s.first() {
        Some(b'-') => -(num(&s[1..]) as i64),
        _ => num(s) as i64,
    }
}

pub fn trim(s: &[u8]) -> &[u8] {
    let a = s
        .iter()
        .position(|&c| !c.is_ascii_whitespace())
        .unwrap_or(s.len());
    let b = s
        .iter()
        .rposition(|&c| !c.is_ascii_whitespace())
        .map_or(a, |i| i + 1);
    &s[a..b]
}

/// Valeur d'une ligne « Clé: valeur » (/proc/meminfo, /proc/PID/status…).
pub fn key_value<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    data.split(|&c| c == b'\n').find_map(|l| {
        let rest = l.strip_prefix(key)?;
        let rest = rest.strip_prefix(b":")?;
        Some(trim(rest))
    })
}

fn copy_into(dst: &mut [u8], src: &[u8]) -> usize {
    let n = src.len().min(dst.len());
    dst[..n].copy_from_slice(&src[..n]);
    n
}

fn basename(s: &[u8]) -> &[u8] {
    match s.iter().rposition(|&c| c == b'/' || c == b'\\') {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

fn ends_with_ci(s: &[u8], suffix: &[u8]) -> bool {
    s.len() >= suffix.len() && s[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

pub fn fmt_u64(out: &mut [u8; 20], mut v: u64) -> &[u8] {
    let mut i = 20;
    loop {
        i -= 1;
        out[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 || i == 0 {
            break;
        }
    }
    &out[i..]
}

// ---------- Actualisation ----------

/// Relève toutes les mesures si l'intervalle est écoulé.  Renvoie vrai si
/// de nouvelles valeurs sont disponibles.
pub unsafe fn refresh() -> bool {
    let now = GetTickCount64();
    if PAUSED && SAMPLES > 0 {
        return false;
    }
    if SAMPLES > 0 && now.saturating_sub(LAST) < INTERVAL_MS as u64 {
        return false;
    }
    let elapsed_ms = if SAMPLES == 0 {
        0
    } else {
        now.saturating_sub(LAST).max(1)
    };
    LAST = now;
    if !STATIC_DONE {
        STATIC_DONE = true;
        load_cpu_info();
        MY_UID = own_uid();
    }
    read_cpu();
    read_mem();
    read_procs(elapsed_ms);
    read_disks(elapsed_ms);
    read_nets(elapsed_ms);
    let mut buf = [0u8; 64];
    let n = read(&[b"/proc/uptime"], &mut buf);
    UPTIME_SEC = num(&buf[..n]);

    let pos = HIST_POS;
    CPU_HIST[pos] = CPU_LOAD;
    MEM_HIST[pos] = mem_used_permille();
    HIST_POS = (pos + 1) % HIST;
    SAMPLES = SAMPLES.saturating_add(1);
    true
}

/// Valeur `n` mesures en arrière dans un historique (0 = la plus récente).
pub unsafe fn hist_at(hist: &[u32; HIST], back: usize) -> u32 {
    hist[(HIST_POS + HIST * 2 - 1 - back) % HIST]
}

pub unsafe fn samples() -> u32 {
    SAMPLES
}

unsafe fn own_uid() -> u32 {
    let mut buf = [0u8; 2048];
    let n = read(&[b"/proc/self/status"], &mut buf);
    key_value(&buf[..n], b"Uid").map_or(u32::MAX, |v| num(v) as u32)
}

unsafe fn read_cpu() {
    let mut buf = [0u8; 1024];
    let n = read(&[b"/proc/stat"], &mut buf);
    let Some(line) = buf[..n].split(|&c| c == b'\n').next() else {
        return;
    };
    if !line.starts_with(b"cpu ") {
        return;
    }
    let mut total = 0u64;
    let mut idle = 0u64;
    for (i, f) in line[4..]
        .split(|&c| c == b' ')
        .filter(|f| !f.is_empty())
        .enumerate()
    {
        let v = num(f);
        // user nice system idle iowait irq softirq steal (guest déjà compté)
        if i < 8 {
            total += v;
        }
        if i == 3 || i == 4 {
            idle += v;
        }
    }
    let dt = total.saturating_sub(CPU_PREV_TOTAL);
    let di = idle.saturating_sub(CPU_PREV_IDLE);
    if CPU_PREV_TOTAL != 0 && dt > 0 {
        CPU_LOAD = (dt.saturating_sub(di) * 1000 / dt) as u32;
    }
    CPU_DELTA = if CPU_PREV_TOTAL != 0 { dt } else { 0 };
    CPU_PREV_TOTAL = total;
    CPU_PREV_IDLE = idle;

    // Fréquence actuelle : moyenne de scaling_cur_freq, sinon /proc/cpuinfo.
    let mut sum = 0u64;
    let mut count = 0u64;
    for i in 0..CPU.logical.min(64) {
        let mut d = [0u8; 20];
        let mut tmp = [0u8; 32];
        let id = fmt_u64(&mut d, i as u64);
        let n = read(
            &[
                b"/sys/devices/system/cpu/cpu",
                id,
                b"/cpufreq/scaling_cur_freq",
            ],
            &mut tmp,
        );
        if n > 0 {
            sum += num(&tmp[..n]);
            count += 1;
        }
    }
    CPU_MHZ = if count > 0 {
        (sum / count / 1000) as u32
    } else {
        let mut info = [0u8; 4096];
        let n = read(&[b"/proc/cpuinfo"], &mut info);
        key_value_tab(&info[..n], b"cpu MHz").map_or(0, |v| num(v) as u32)
    };
}

/// /proc/cpuinfo sépare clé et valeur par des tabulations puis « : ».
fn key_value_tab<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    data.split(|&c| c == b'\n').find_map(|l| {
        let rest = l.strip_prefix(key)?;
        let colon = rest.iter().position(|&c| c == b':')?;
        if rest[..colon].iter().all(|&c| c == b'\t' || c == b' ') {
            Some(trim(&rest[colon + 1..]))
        } else {
            None
        }
    })
}

/// Nombre d'éléments d'une liste de plages « 0-3,6,8-9 ».
fn range_count(s: &[u8]) -> u32 {
    let mut total = 0u32;
    for part in trim(s).split(|&c| c == b',') {
        if part.is_empty() {
            continue;
        }
        match part.iter().position(|&c| c == b'-') {
            Some(d) => {
                let a = num(&part[..d]) as u32;
                let b = num(&part[d + 1..]) as u32;
                total += b.saturating_sub(a) + 1;
            }
            None => total += 1,
        }
    }
    total
}

unsafe fn load_cpu_info() {
    let mut info = [0u8; 8192];
    let n = read(&[b"/proc/cpuinfo"], &mut info);
    let data = &info[..n];
    if let Some(m) = key_value_tab(data, b"model name") {
        CPU.model_len = copy_into(&mut CPU.model, m);
    }
    let mut tmp = [0u8; 64];
    let k = read(&[b"/sys/devices/system/cpu/online"], &mut tmp);
    let logical = range_count(&tmp[..k]);
    CPU.logical = if logical > 0 {
        logical
    } else {
        data.split(|&c| c == b'\n')
            .filter(|l| l.starts_with(b"processor"))
            .count()
            .max(1) as u32
    };
    CPU.cores = key_value_tab(data, b"cpu cores").map_or(0, |v| num(v) as u32);
    // Sockets : identifiants physiques distincts (le premier bloc suffit en
    // pratique : plusieurs sockets se voient au nombre de cœurs).
    CPU.sockets = 1;
    if CPU.cores > 0 && CPU.logical > 0 {
        let siblings = key_value_tab(data, b"siblings").map_or(0, |v| num(v) as u32);
        if siblings > 0 && CPU.logical > siblings {
            CPU.sockets = CPU.logical / siblings;
            CPU.cores *= CPU.sockets;
        }
    }
    CPU.virtual_machine = key_value_tab(data, b"flags")
        .is_some_and(|f| f.split(|&c| c == b' ').any(|w| w == b"hypervisor"));
    let k = read(
        &[b"/sys/devices/system/cpu/cpu0/cpufreq/base_frequency"],
        &mut tmp,
    );
    CPU.base_mhz = (num(&tmp[..k]) / 1000) as u32;
    let k = read(
        &[b"/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq"],
        &mut tmp,
    );
    CPU.max_mhz = (num(&tmp[..k]) / 1000) as u32;
    if CPU.base_mhz == 0 {
        // Sans cpufreq (machine virtuelle) : la vitesse annoncée par le
        // processeur, sinon celle relevée par le noyau.
        CPU.base_mhz = mhz_from_model(&CPU.model[..CPU.model_len])
            .or_else(|| key_value_tab(data, b"cpu MHz").map(|v| num(v) as u32))
            .unwrap_or(0);
    }
    // Caches : index0..index4 de cpu0 ; L2/L3 partagés comptés une fois.
    for i in 0..6u8 {
        let idx = [b'0' + i];
        let mut level = [0u8; 8];
        let mut size = [0u8; 16];
        let base: &[u8] = b"/sys/devices/system/cpu/cpu0/cache/index";
        let nl = read(&[base, &idx, b"/level"], &mut level);
        if nl == 0 {
            break;
        }
        let ns = read(&[base, &idx, b"/size"], &mut size);
        let mut kb = num(&size[..ns]) as u32;
        if size[..ns].contains(&b'M') {
            kb *= 1024;
        }
        match num(&level[..nl]) {
            1 => CPU.l1_kb += kb * CPU.cores.max(1),
            2 => CPU.l2_kb += kb * CPU.cores.max(1),
            3 => CPU.l3_kb += kb,
            _ => {}
        }
    }
}

/// « … @ 2.40GHz » dans le nom du modèle.
fn mhz_from_model(model: &[u8]) -> Option<u32> {
    let at = model.iter().position(|&c| c == b'@')?;
    let rest = trim(&model[at + 1..]);
    let whole = num(rest) as u32;
    let dot = rest.iter().position(|&c| c == b'.');
    let frac = dot.map_or(0, |d| {
        let f = &rest[d + 1..];
        let digits = f.iter().take_while(|c| c.is_ascii_digit()).count().min(2);
        let v = num(&f[..digits]) as u32;
        if digits == 1 {
            v * 100
        } else {
            v * 10
        }
    });
    (whole > 0).then_some(whole * 1000 + frac)
}

unsafe fn read_mem() {
    let mut buf = [0u8; 4096];
    let n = read(&[b"/proc/meminfo"], &mut buf);
    let d = &buf[..n];
    let kb = |k: &[u8]| key_value(d, k).map_or(0, num);
    MEM_TOTAL_KB = kb(b"MemTotal");
    MEM_AVAIL_KB = kb(b"MemAvailable");
    MEM_CACHED_KB = kb(b"Cached") + kb(b"Buffers");
    SWAP_TOTAL_KB = kb(b"SwapTotal");
    SWAP_FREE_KB = kb(b"SwapFree");
    COMMIT_KB = kb(b"Committed_AS");
    COMMIT_LIMIT_KB = kb(b"CommitLimit");
}

pub unsafe fn mem_used_kb() -> u64 {
    MEM_TOTAL_KB.saturating_sub(MEM_AVAIL_KB)
}

pub unsafe fn mem_used_permille() -> u32 {
    if MEM_TOTAL_KB == 0 {
        0
    } else {
        (mem_used_kb() * 1000 / MEM_TOTAL_KB) as u32
    }
}

// ---------- Processus ----------

unsafe fn find_prev(pid: u32, start: u64, hint: &mut usize) -> Option<usize> {
    let prev = &*core::ptr::addr_of!(PREV);
    let n = PREV_COUNT;
    for k in 0..n {
        let i = (*hint + k) % n;
        if prev[i].pid == pid && prev[i].start == start {
            *hint = i + 1;
            return Some(i);
        }
    }
    None
}

unsafe fn read_procs(elapsed_ms: u64) {
    // L'échantillon courant devient le précédent.
    let procs = &mut *core::ptr::addr_of_mut!(PROCS);
    let prev = &mut *core::ptr::addr_of_mut!(PREV);
    prev[..COUNT].copy_from_slice(&procs[..COUNT]);
    PREV_COUNT = COUNT;
    COUNT = 0;
    THREADS = 0;
    let mut hint = 0usize;
    list_dir(b"/proc", &mut |name| {
        if COUNT >= MAX_PROCS || !name.iter().all(u8::is_ascii_digit) {
            return;
        }
        let pid = num(name) as u32;
        let mut stat = [0u8; 1024];
        let n = read(&[b"/proc/", name, b"/stat"], &mut stat);
        let Some(close) = stat[..n].iter().rposition(|&c| c == b')') else {
            return;
        };
        let open = stat.iter().position(|&c| c == b'(').unwrap_or(0);
        let comm = &stat[open + 1..close];
        // Champs après « ) » : 0=état 1=ppid … 11=utime 12=stime 16=nice
        // 17=threads 19=starttime 21=rss (pages).
        let mut f: [&[u8]; 24] = [b""; 24];
        for (i, x) in stat[close + 1..n]
            .split(|&c| c == b' ')
            .filter(|x| !x.is_empty())
            .take(24)
            .enumerate()
        {
            f[i] = x;
        }
        let start = num(f[19]);
        let mut p = match find_prev(pid, start, &mut hint) {
            Some(i) => {
                let mut p = (*core::ptr::addr_of!(PREV))[i];
                p.fresh = false;
                p
            }
            None => {
                let mut p = describe(pid, name, comm, start);
                p.fresh = SAMPLES > 0;
                p
            }
        };
        let ticks = num(f[11]) + num(f[12]);
        let old_ticks = p.ticks;
        p.state = f[0].first().copied().unwrap_or(b'S');
        p.ppid = num(f[1]) as u32;
        p.nice = signed(f[16]).clamp(-20, 19) as i8;
        p.threads = num(f[17]) as u32;
        p.mem_kb = (num(f[21]) * 4) as u32;
        p.dticks = if old_ticks > 0 || p.fresh {
            ticks.saturating_sub(old_ticks)
        } else {
            0
        };
        p.ticks = ticks;
        p.cpu = if CPU_DELTA > 0 && old_ticks > 0 {
            (ticks.saturating_sub(old_ticks) * 1000 / CPU_DELTA).min(1000) as u32
        } else {
            0
        };
        // Disque : octets réellement lus ou écrits (propres processus seulement).
        if p.kind != KIND_KERNEL {
            let mut io = [0u8; 512];
            let k = read(&[b"/proc/", name, b"/io"], &mut io);
            if k > 0 {
                let total = key_value(&io[..k], b"read_bytes").map_or(0, num)
                    + key_value(&io[..k], b"write_bytes").map_or(0, num);
                p.io_bps = if elapsed_ms > 0 && p.io > 0 {
                    (total.saturating_sub(p.io) * 1000 / elapsed_ms).min(u32::MAX as u64 - 1) as u32
                } else {
                    0
                };
                p.io = total.max(1);
            } else {
                p.io_bps = UNKNOWN;
            }
        }
        THREADS += p.threads;
        (*core::ptr::addr_of_mut!(PROCS))[COUNT] = p;
        COUNT += 1;
    });
}

/// Première lecture d'un processus : nom, ligne de commande, utilisateur et
/// nature (programme .exe, application Winlux, Linux ou noyau).
unsafe fn describe(pid: u32, id: &[u8], comm: &[u8], start: u64) -> Proc {
    let mut p = EMPTY;
    p.pid = pid;
    p.start = start;
    let mut status = [0u8; 2048];
    let n = read(&[b"/proc/", id, b"/status"], &mut status);
    p.uid = key_value(&status[..n], b"Uid").map_or(0, |v| num(v) as u32);
    let mut raw = [0u8; 512];
    let k = read(&[b"/proc/", id, b"/cmdline"], &mut raw);
    let raw = &raw[..k];
    let kthread = key_value(&status[..n], b"Kthread").map(num);
    if k == 0 && (kthread != Some(0) || pid == 2) {
        p.kind = KIND_KERNEL;
        p.name_len = copy_into(&mut p.name, comm) as u8;
        p.cmd_len = 0;
        return p;
    }
    let mut args = raw.split(|&c| c == 0).filter(|a| !a.is_empty());
    let arg0 = args.next().unwrap_or(b"");
    let arg1 = args.next().unwrap_or(b"");
    // Ligne de commande lisible (arguments séparés par des espaces).
    let mut cl = 0usize;
    for &c in raw.iter() {
        if cl >= p.cmd.len() {
            break;
        }
        p.cmd[cl] = if c == 0 { b' ' } else { c };
        cl += 1;
    }
    p.cmd_len = trim(&p.cmd[..cl]).len() as u8;
    let base0 = basename(arg0);
    let name: &[u8] = if base0 == b"winlux" && ends_with_ci(arg1, b".exe") {
        p.kind = KIND_EXE;
        let exe = basename(arg1);
        if exe.eq_ignore_ascii_case(b"gui_demo.exe") {
            p.kind = KIND_SHELL;
        }
        exe
    } else if base0.starts_with(b"python") && !arg1.is_empty() && arg1[0] != b'-' {
        // Scripts : le nom du script parle plus que « python3 ».
        basename(arg1)
    } else if !base0.is_empty() && base0.starts_with(&comm[..comm.len().min(base0.len())]) {
        base0
    } else {
        comm
    };
    p.name_len = copy_into(&mut p.name, name) as u8;
    if p.kind == KIND_SHELL {
        shell_app_name(&mut p, id);
    }
    p
}

/// Une application intégrée (gui_demo.exe) s'affiche sous son vrai nom.
unsafe fn shell_app_name(p: &mut Proc, id: &[u8]) {
    let mut env = [0u8; 8192];
    let n = read(&[b"/proc/", id, b"/environ"], &mut env);
    let app = env[..n]
        .split(|&c| c == 0)
        .find_map(|v| v.strip_prefix(b"WINLUX_SHELL_APP="))
        .unwrap_or(b"");
    let label: &[u8] = match app {
        b"explorer" => tr4(&[
            b"Explorateur de fichiers",
            b"File Explorer",
            b"Explorador de archivos",
            b"Explorer",
        ]),
        b"settings" => tr4(&[
            b"Param\xE8tres",
            b"Settings",
            b"Configuraci\xF3n",
            b"Einstellungen",
        ]),
        b"store" => tr4(&[
            b"Store Winlux",
            b"Winlux Store",
            b"Tienda Winlux",
            b"Winlux Store",
        ]),
        b"fonts" => tr4(&[b"Polices", b"Fonts", b"Fuentes", b"Schriftarten"]),
        b"taskmgr" => tr4(&[
            b"Gestionnaire des t\xE2ches",
            b"Task Manager",
            b"Administrador de tareas",
            b"Task-Manager",
        ]),
        b"taskschd" => tr4(&[
            b"Planificateur de t\xE2ches",
            b"Task Scheduler",
            b"Programador de tareas",
            b"Aufgabenplanung",
        ]),
        b"firewall" => tr4(&[
            b"Pare-feu Winlux",
            b"Winlux Firewall",
            b"Firewall de Winlux",
            b"Winlux-Firewall",
        ]),
        _ => tr4(&[
            b"Bureau Winlux",
            b"Winlux desktop",
            b"Escritorio de Winlux",
            b"Winlux-Desktop",
        ]),
    };
    p.name_len = copy_into(&mut p.name, label) as u8;
}

pub unsafe fn proc_by_pid(pid: u32) -> Option<&'static Proc> {
    let procs = &*core::ptr::addr_of!(PROCS);
    procs[..COUNT].iter().find(|p| p.pid == pid)
}

/// Résultat d'une action sur un processus.
#[derive(Clone, Copy, PartialEq)]
pub enum Outcome {
    Done,
    /// Le processus appartient à un autre utilisateur ou au système.
    Denied,
    /// Le processus n'existe plus.
    Gone,
    /// Priorité appliquée partiellement (la relever demande des droits).
    Limited,
}

/// Fin de tâche (TerminateProcess, donc immédiate).
pub unsafe fn end_task(pid: u32) -> Outcome {
    let h = OpenProcess(0x0001, 0, pid);
    if h.is_null() {
        return Outcome::Gone;
    }
    let ok = TerminateProcess(h, 1) != 0;
    let err = GetLastError();
    CloseHandle(h);
    match (ok, err) {
        (true, _) => Outcome::Done,
        (false, 87) => Outcome::Gone,
        _ => Outcome::Denied,
    }
}

/// Classes de priorité Windows proposées par le menu, de la plus haute à la
/// plus basse, avec la valeur « nice » que le runtime applique.
pub const PRIORITIES: [(u32, i8); 5] = [
    (0x80, -10),
    (0x8000, -5),
    (0x20, 0),
    (0x4000, 10),
    (0x40, 19),
];

/// Classe de priorité correspondant à une valeur nice.
pub fn priority_index(nice: i8) -> usize {
    match nice {
        i8::MIN..=-8 => 0,
        -7..=-3 => 1,
        -2..=4 => 2,
        5..=14 => 3,
        _ => 4,
    }
}

pub unsafe fn set_priority(pid: u32, index: usize) -> Outcome {
    let (class, want) = PRIORITIES[index.min(PRIORITIES.len() - 1)];
    let h = OpenProcess(0x0200, 0, pid);
    if h.is_null() {
        return Outcome::Gone;
    }
    let ok = SetPriorityClass(h, class) != 0;
    CloseHandle(h);
    if !ok {
        return Outcome::Denied;
    }
    // Le runtime ramène la demande à ce que Linux autorise : on relit.
    let mut d = [0u8; 20];
    let mut stat = [0u8; 1024];
    let n = read(
        &[b"/proc/", fmt_u64(&mut d, pid as u64), b"/stat"],
        &mut stat,
    );
    let got = stat[..n]
        .iter()
        .rposition(|&c| c == b')')
        .and_then(|c| {
            stat[c + 1..n]
                .split(|&x| x == b' ')
                .filter(|x| !x.is_empty())
                .nth(16)
        })
        .map(signed);
    let procs = &mut *core::ptr::addr_of_mut!(PROCS);
    if let (Some(got), Some(p)) = (got, procs[..COUNT].iter_mut().find(|p| p.pid == pid)) {
        p.nice = got as i8;
    }
    match got {
        Some(g) if g as i8 == want => Outcome::Done,
        Some(_) => Outcome::Limited,
        None => Outcome::Gone,
    }
}

// ---------- Utilisateurs ----------

const MAX_USERS: usize = 96;
static mut USER_IDS: [u32; MAX_USERS] = [0; MAX_USERS];
static mut USER_NAMES: [[u8; 32]; MAX_USERS] = [[0; 32]; MAX_USERS];
static mut USER_LENS: [u8; MAX_USERS] = [0; MAX_USERS];
static mut USER_COUNT: usize = usize::MAX;

/// Nom du compte (/etc/passwd), sinon l'identifiant numérique.
pub unsafe fn user_name(uid: u32, scratch: &mut [u8; 20]) -> &[u8] {
    if USER_COUNT == usize::MAX {
        USER_COUNT = 0;
        let mut buf = [0u8; 16384];
        let n = read(&[b"/etc/passwd"], &mut buf);
        for line in buf[..n].split(|&c| c == b'\n') {
            let mut parts = line.split(|&c| c == b':');
            let (Some(name), _, Some(id)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            if USER_COUNT < MAX_USERS && !name.is_empty() {
                USER_IDS[USER_COUNT] = num(id) as u32;
                USER_LENS[USER_COUNT] = copy_into(&mut USER_NAMES[USER_COUNT], name) as u8;
                USER_COUNT += 1;
            }
        }
    }
    for i in 0..USER_COUNT {
        if USER_IDS[i] == uid {
            let names = &*core::ptr::addr_of!(USER_NAMES);
            return &names[i][..USER_LENS[i] as usize];
        }
    }
    fmt_u64(scratch, uid as u64)
}

// ---------- Disques ----------

fn whole_disk(name: &[u8]) -> bool {
    let letters = |p: &[u8]| {
        name.len() > p.len()
            && name.starts_with(p)
            && name[p.len()..].iter().all(u8::is_ascii_lowercase)
    };
    if letters(b"sd") || letters(b"vd") || letters(b"hd") || letters(b"xvd") {
        return true;
    }
    if let Some(rest) = name.strip_prefix(b"nvme") {
        // nvme0n1 oui, nvme0n1p2 non.
        return rest.contains(&b'n') && !rest.contains(&b'p');
    }
    if let Some(rest) = name.strip_prefix(b"mmcblk") {
        return !rest.is_empty() && rest.iter().all(u8::is_ascii_digit);
    }
    false
}

unsafe fn read_disks(elapsed_ms: u64) {
    let mut buf = [0u8; 8192];
    let n = read(&[b"/proc/diskstats"], &mut buf);
    let mut seen = 0usize;
    for line in buf[..n].split(|&c| c == b'\n') {
        let mut f: [&[u8]; 14] = [b""; 14];
        for (i, x) in line
            .split(|&c| c == b' ')
            .filter(|x| !x.is_empty())
            .take(14)
            .enumerate()
        {
            f[i] = x;
        }
        let name = f[2];
        if !whole_disk(name) || seen >= MAX_DISKS {
            continue;
        }
        let read_sectors = num(f[5]);
        let write_sectors = num(f[9]);
        let busy_ms = num(f[12]);
        let disks = &mut *core::ptr::addr_of_mut!(DISKS);
        let slot = (0..DISK_COUNT).find(|&i| disks[i].name() == name);
        let i = match slot {
            Some(i) => i,
            None => {
                if DISK_COUNT >= MAX_DISKS {
                    continue;
                }
                let i = DISK_COUNT;
                DISK_COUNT += 1;
                let d = &mut disks[i];
                *d = NO_DISK;
                d.name_len = copy_into(&mut d.name, name);
                let mut tmp = [0u8; 64];
                let k = read(&[b"/sys/block/", name, b"/device/model"], &mut tmp);
                d.model_len = copy_into(&mut d.model, trim(&tmp[..k]));
                let k = read(&[b"/sys/block/", name, b"/queue/rotational"], &mut tmp);
                d.ssd = k > 0 && tmp[0] == b'0';
                let k = read(&[b"/sys/block/", name, b"/removable"], &mut tmp);
                d.removable = k > 0 && tmp[0] == b'1';
                let k = read(&[b"/sys/block/", name, b"/size"], &mut tmp);
                d.size_bytes = num(trim(&tmp[..k])) * 512;
                d.prev_read = read_sectors;
                d.prev_write = write_sectors;
                d.prev_busy = busy_ms;
                i
            }
        };
        seen += 1;
        let d = &mut disks[i];
        if elapsed_ms > 0 {
            d.read_bps = read_sectors.saturating_sub(d.prev_read) * 512 * 1000 / elapsed_ms;
            d.write_bps = write_sectors.saturating_sub(d.prev_write) * 512 * 1000 / elapsed_ms;
            d.active = (busy_ms.saturating_sub(d.prev_busy) * 1000 / elapsed_ms).min(1000) as u32;
        }
        d.prev_read = read_sectors;
        d.prev_write = write_sectors;
        d.prev_busy = busy_ms;
        d.hist[HIST_POS] = d.active;
    }
}

// ---------- Réseau ----------

fn virtual_iface(name: &[u8]) -> bool {
    name == b"lo"
        || name.starts_with(b"veth")
        || name.starts_with(b"docker")
        || name.starts_with(b"br-")
        || name.starts_with(b"virbr")
}

unsafe fn read_nets(elapsed_ms: u64) {
    let mut buf = [0u8; 8192];
    let n = read(&[b"/proc/net/dev"], &mut buf);
    for line in buf[..n].split(|&c| c == b'\n') {
        let Some(colon) = line.iter().position(|&c| c == b':') else {
            continue;
        };
        let name = trim(&line[..colon]);
        if virtual_iface(name) {
            continue;
        }
        let mut f: [&[u8]; 16] = [b""; 16];
        for (i, x) in line[colon + 1..]
            .split(|&c| c == b' ')
            .filter(|x| !x.is_empty())
            .take(16)
            .enumerate()
        {
            f[i] = x;
        }
        let rx = num(f[0]);
        let tx = num(f[8]);
        let nets = &mut *core::ptr::addr_of_mut!(NETS);
        let i = match (0..NET_COUNT).find(|&i| nets[i].name() == name) {
            Some(i) => i,
            None => {
                let mut state = [0u8; 16];
                let k = read(&[b"/sys/class/net/", name, b"/operstate"], &mut state);
                let up = matches!(trim(&state[..k]), b"up" | b"unknown");
                if !up || NET_COUNT >= MAX_NETS {
                    continue;
                }
                let i = NET_COUNT;
                NET_COUNT += 1;
                let d = &mut nets[i];
                *d = NO_NET;
                d.name_len = copy_into(&mut d.name, name);
                d.wifi = exists(&[b"/sys/class/net/", name, b"/wireless"]);
                let mut tmp = [0u8; 32];
                let k = read(&[b"/sys/class/net/", name, b"/speed"], &mut tmp);
                let speed = signed(trim(&tmp[..k]));
                d.speed_mbps = if speed > 0 { speed as u32 } else { 0 };
                let k = read(&[b"/sys/class/net/", name, b"/address"], &mut tmp);
                copy_into(&mut d.mac, trim(&tmp[..k]));
                d.prev_rx = rx;
                d.prev_tx = tx;
                i
            }
        };
        let d = &mut nets[i];
        if elapsed_ms > 0 {
            d.rx_bps = rx.saturating_sub(d.prev_rx) * 8 * 1000 / elapsed_ms;
            d.tx_bps = tx.saturating_sub(d.prev_tx) * 8 * 1000 / elapsed_ms;
        }
        d.prev_rx = rx;
        d.prev_tx = tx;
        d.hist[HIST_POS] = (d.rx_bps + d.tx_bps).min(u32::MAX as u64) as u32;
    }
}

// ---------- Applications de démarrage ----------

pub const MAX_STARTUP: usize = 48;

#[derive(Clone, Copy)]
pub struct Startup {
    pub file: [u8; 64],
    pub file_len: usize,
    pub name: [u8; 64],
    pub name_len: usize,
    pub exec: [u8; 160],
    pub exec_len: usize,
    /// Fichier présent dans ~/.config/autostart (sinon /etc/xdg/autostart).
    pub user: bool,
    pub enabled: bool,
}

const NO_STARTUP: Startup = Startup {
    file: [0; 64],
    file_len: 0,
    name: [0; 64],
    name_len: 0,
    exec: [0; 160],
    exec_len: 0,
    user: false,
    enabled: true,
};

impl Startup {
    pub fn file(&self) -> &[u8] {
        &self.file[..self.file_len]
    }
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
    pub fn exec(&self) -> &[u8] {
        &self.exec[..self.exec_len]
    }
}

pub static mut STARTUP: [Startup; MAX_STARTUP] = [NO_STARTUP; MAX_STARTUP];
pub static mut STARTUP_COUNT: usize = 0;

/// Dossier personnel en chemin Linux (« /home/damien »).
pub unsafe fn home(out: &mut [u8; 200]) -> &[u8] {
    let n = GetEnvironmentVariableA(c"HOME".as_ptr(), out.as_mut_ptr().cast(), 200) as usize;
    if n == 0 || n >= 200 {
        return b"/root";
    }
    &out[..n]
}

fn desktop_value<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut in_entry = false;
    for line in data.split(|&c| c == b'\n') {
        let line = trim(line);
        if line.starts_with(b"[") {
            in_entry = line == b"[Desktop Entry]";
            continue;
        }
        if in_entry && line.len() > key.len() && line.starts_with(key) && line[key.len()] == b'=' {
            return Some(trim(&line[key.len() + 1..]));
        }
    }
    None
}

unsafe fn add_startup(dir: &[u8], file: &[u8], user: bool) {
    if !ends_with_ci(file, b".desktop") {
        return;
    }
    let mut data = [0u8; 8192];
    let n = read(&[dir, b"/", file], &mut data);
    let d = &data[..n];
    // Session Winlux : on ignore ce qui est réservé à un autre bureau.
    if let Some(only) = desktop_value(d, b"OnlyShowIn") {
        if !only
            .split(|&c| c == b';')
            .any(|e| e == b"XFCE" || e == b"Winlux")
        {
            return;
        }
    }
    if desktop_value(d, b"NotShowIn").is_some_and(|v| {
        v.split(|&c| c == b';')
            .any(|e| e == b"XFCE" || e == b"Winlux")
    }) {
        return;
    }
    let enabled = desktop_value(d, b"Hidden") != Some(b"true")
        && desktop_value(d, b"X-GNOME-Autostart-enabled") != Some(b"false");
    let list = &mut *core::ptr::addr_of_mut!(STARTUP);
    let slot = (0..STARTUP_COUNT).find(|&i| list[i].file() == file);
    let i = match slot {
        Some(i) => i,
        None if STARTUP_COUNT < MAX_STARTUP => {
            STARTUP_COUNT += 1;
            STARTUP_COUNT - 1
        }
        None => return,
    };
    let e = &mut list[i];
    *e = NO_STARTUP;
    e.file_len = copy_into(&mut e.file, file);
    let name = desktop_value(d, b"Name").unwrap_or(file);
    e.name_len = copy_into(&mut e.name, name);
    e.exec_len = copy_into(&mut e.exec, desktop_value(d, b"Exec").unwrap_or(b""));
    e.user = user;
    e.enabled = enabled;
}

/// Relit les applications lancées à l'ouverture de session (XDG autostart ;
/// le fichier de l'utilisateur remplace celui du système de même nom).
pub unsafe fn load_startup() {
    STARTUP_COUNT = 0;
    list_dir(b"/etc/xdg/autostart", &mut |f| {
        add_startup(b"/etc/xdg/autostart", f, false)
    });
    let mut h = [0u8; 200];
    let home = home(&mut h);
    let mut dir = [0u8; 240];
    let mut len = copy_into(&mut dir, home);
    len += copy_into(&mut dir[len..], b"/.config/autostart");
    let dir = &dir[..len];
    list_dir(dir, &mut |f| add_startup(dir, f, true));
    let list = &mut *core::ptr::addr_of_mut!(STARTUP);
    list[..STARTUP_COUNT].sort_unstable_by(|a, b| a.name().cmp(b.name()));
}

/// Active ou désactive une application de démarrage : écrit une copie dans
/// ~/.config/autostart avec `Hidden=` (comme le fait XFCE), sans toucher au
/// fichier du système.
pub unsafe fn set_startup(index: usize, enabled: bool) -> bool {
    if index >= STARTUP_COUNT {
        return false;
    }
    let e = (*core::ptr::addr_of!(STARTUP))[index];
    let mut h = [0u8; 200];
    let home = home(&mut h);
    let mut src = [0u8; 8192];
    let n = if e.user {
        read(&[home, b"/.config/autostart/", e.file()], &mut src)
    } else {
        read(&[b"/etc/xdg/autostart/", e.file()], &mut src)
    };
    if n == 0 {
        return false;
    }
    let mut out = [0u8; 8400];
    let mut len = 0usize;
    for line in src[..n].split(|&c| c == b'\n') {
        if line.starts_with(b"Hidden=") || line.starts_with(b"X-GNOME-Autostart-enabled=") {
            continue;
        }
        if len + line.len() + 1 >= out.len() - 32 {
            break;
        }
        len += copy_into(&mut out[len..], line);
        out[len] = b'\n';
        len += 1;
        if trim(line) == b"[Desktop Entry]" {
            len += copy_into(
                &mut out[len..],
                if enabled {
                    b"Hidden=false\n"
                } else {
                    b"Hidden=true\n"
                },
            );
        }
    }
    while len > 1 && out[len - 1] == b'\n' && out[len - 2] == b'\n' {
        len -= 1;
    }
    CreateDirectoryA(zpath(&[home, b"/.config"]).ptr(), core::ptr::null_mut());
    CreateDirectoryA(
        zpath(&[home, b"/.config/autostart"]).ptr(),
        core::ptr::null_mut(),
    );
    let path = zpath(&[home, b"/.config/autostart/", e.file()]);
    if !crate::admin_ui::write_file(&path, &out[..len]) {
        return false;
    }
    load_startup();
    true
}

// ---------- Services ----------

pub const MAX_SERVICES: usize = 96;

#[derive(Clone, Copy)]
pub struct Service {
    pub name: [u8; 48],
    pub name_len: usize,
    pub desc: [u8; 80],
    pub desc_len: usize,
    pub pid: u32,
    pub running: bool,
}

const NO_SERVICE: Service = Service {
    name: [0; 48],
    name_len: 0,
    desc: [0; 80],
    desc_len: 0,
    pid: 0,
    running: false,
};

impl Service {
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
    pub fn desc(&self) -> &[u8] {
        &self.desc[..self.desc_len]
    }
}

pub static mut SERVICES: [Service; MAX_SERVICES] = [NO_SERVICE; MAX_SERVICES];
pub static mut SERVICE_COUNT: usize = 0;

/// Vrai si le système suit ses services par groupes de contrôle (systemd).
pub unsafe fn services_available() -> bool {
    exists(&[b"/sys/fs/cgroup/system.slice"])
}

/// Services actifs : un dossier `*.service` par service dans
/// /sys/fs/cgroup/system.slice, description lue dans le fichier d'unité.
pub unsafe fn load_services() {
    SERVICE_COUNT = 0;
    list_dir(b"/sys/fs/cgroup/system.slice", &mut |dir| {
        let Some(name) = dir.strip_suffix(b".service") else {
            return;
        };
        if SERVICE_COUNT >= MAX_SERVICES {
            return;
        }
        let list = &mut *core::ptr::addr_of_mut!(SERVICES);
        let s = &mut list[SERVICE_COUNT];
        *s = NO_SERVICE;
        s.name_len = copy_into(&mut s.name, name);
        let mut procs = [0u8; 64];
        let k = read(
            &[b"/sys/fs/cgroup/system.slice/", dir, b"/cgroup.procs"],
            &mut procs,
        );
        s.pid = num(trim(&procs[..k])) as u32;
        s.running = k > 0 && s.pid > 0;
        let mut unit = [0u8; 4096];
        for base in [
            b"/etc/systemd/system/".as_slice(),
            b"/usr/lib/systemd/system/",
            b"/lib/systemd/system/",
        ] {
            let k = read(&[base, dir], &mut unit);
            if k > 0 {
                if let Some(d) = key_eq(&unit[..k], b"Description") {
                    s.desc_len = copy_into(&mut s.desc, d);
                }
                break;
            }
        }
        SERVICE_COUNT += 1;
    });
    let list = &mut *core::ptr::addr_of_mut!(SERVICES);
    list[..SERVICE_COUNT].sort_unstable_by(|a, b| a.name().cmp(b.name()));
}

fn key_eq<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    data.split(|&c| c == b'\n').find_map(|l| {
        let l = trim(l);
        (l.len() > key.len() && l.starts_with(key) && l[key.len()] == b'=')
            .then(|| trim(&l[key.len() + 1..]))
    })
}
