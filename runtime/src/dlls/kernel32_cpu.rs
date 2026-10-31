//! kernel32 : affinité, priorités, topologie processeur et NUMA.
//!
//! Les processeurs logiques Windows sont numérotés dans l'ordre des
//! processeurs Linux en ligne (groupe 0, 64 au plus). Chaque appel s'appuie
//! sur `crate::cpu` : sans NUMA, une seule machine « nœud 0 » ; sans CPU
//! hybride, tous les cœurs ont la même classe d'efficacité.

use crate::cpu::{self, Caps, CoreKind, Priority, Privileges, MASK_BITS};
use crate::dlls::lock;
use crate::process::set_last_error;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INVALID_HANDLE: u32 = 6;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

static PROCESS_CLASS: AtomicU32 = AtomicU32::new(0x20);
static THREAD_PRIORITIES: OnceLock<Mutex<HashMap<i32, i32>>> = OnceLock::new();
static IDEAL: OnceLock<Mutex<HashMap<i32, u32>>> = OnceLock::new();

fn caps() -> &'static Caps {
    static CAPS: OnceLock<Caps> = OnceLock::new();
    CAPS.get_or_init(|| cpu::detect(&cpu::Sys::host()))
}

/// Processeurs Linux visibles par Windows, dans l'ordre des index logiques.
fn logical() -> Vec<usize> {
    let mut online = caps().online();
    if online.is_empty() {
        online = vec![0];
    }
    online.truncate(MASK_BITS);
    online
}

fn system_mask() -> u64 {
    let n = logical().len();
    if n >= 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}

fn mask_to_cpus(mask: u64) -> Vec<usize> {
    logical()
        .into_iter()
        .enumerate()
        .filter(|(i, _)| mask & (1u64 << i) != 0)
        .map(|(_, cpu)| cpu)
        .collect()
}

fn cpus_to_mask(cpus: &[usize]) -> u64 {
    logical()
        .into_iter()
        .enumerate()
        .filter(|(_, cpu)| cpus.contains(cpu))
        .fold(0, |m, (i, _)| m | (1u64 << i))
}

fn process_mask() -> u64 {
    let mask = cpus_to_mask(&cpu::get_affinity(std::process::id() as i32));
    if mask == 0 {
        system_mask()
    } else {
        mask
    }
}

fn is_current_process(h: usize) -> bool {
    h == super::kernel32::get_current_process()
        || super::kernel32::process_os_pid(h) == Some(std::process::id() as i32)
}

// --- Affinité ----------------------------------------------------------------

/// GetProcessGroupAffinity : Winlux présente tous les processeurs dans un
/// seul groupe (le groupe 0), comme un PC de 64 cœurs ou moins.
pub unsafe extern "win64" fn get_process_group_affinity(
    _h: usize,
    count: *mut u16,
    groups: *mut u16,
) -> i32 {
    if count.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if *count < 1 || groups.is_null() {
        *count = 1;
        set_last_error(122); // ERROR_INSUFFICIENT_BUFFER
        return 0;
    }
    *groups = 0;
    *count = 1;
    1
}

/// Les processus Winlux ne sont jamais placés dans un objet tâche (job).
pub unsafe extern "win64" fn is_process_in_job(
    _process: usize,
    _job: usize,
    result: *mut i32,
) -> i32 {
    if result.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *result = 0;
    1
}

/// QueryInformationJobObject : sans objet tâche, aucune limite ne
/// s'applique ; la structure demandée est rendue à zéro (aucun drapeau de
/// limite, aucun plafond de CPU).
pub unsafe extern "win64" fn query_information_job_object(
    _job: usize,
    _class: u32,
    info: *mut u8,
    len: u32,
    ret_len: *mut u32,
) -> i32 {
    if info.is_null() && len != 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if !info.is_null() {
        std::ptr::write_bytes(info, 0, len as usize);
    }
    if !ret_len.is_null() {
        *ret_len = len;
    }
    1
}

thread_local! {
    static STACK_GUARANTEE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}
/// SetThreadStackGuarantee : mémorise la réserve demandée pour le
/// traitement d'un débordement de pile et renvoie la précédente.
pub unsafe extern "win64" fn set_thread_stack_guarantee(size: *mut u32) -> i32 {
    if size.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let previous = STACK_GUARANTEE.with(|g| g.replace((*size).max(g.get())));
    *size = previous;
    1
}

pub unsafe extern "win64" fn get_process_affinity_mask(
    h: usize,
    process: *mut u64,
    system: *mut u64,
) -> i32 {
    if process.is_null() || system.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mask = match super::kernel32::process_os_pid(h) {
        Some(pid) if pid == std::process::id() as i32 => process_mask(),
        Some(pid) => cpus_to_mask(&cpu::get_affinity(pid)),
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            return 0;
        }
    };
    *process = if mask == 0 { system_mask() } else { mask };
    *system = system_mask();
    1
}

pub extern "win64" fn set_process_affinity_mask(h: usize, mask: u64) -> i32 {
    if mask == 0 || mask & !system_mask() != 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let cpus = mask_to_cpus(mask);
    let ok = match super::kernel32::process_os_pid(h) {
        Some(pid) if pid == std::process::id() as i32 => cpu::task_ids()
            .into_iter()
            .all(|tid| cpu::set_affinity(tid, &cpus)),
        Some(pid) => cpu::set_affinity(pid, &cpus),
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            return 0;
        }
    };
    if !ok {
        set_last_error(ERROR_ACCESS_DENIED);
    }
    i32::from(ok)
}

/// Retourne l'ancien masque, 0 en cas d'échec (comme Windows).
pub extern "win64" fn set_thread_affinity_mask(h: usize, mask: u64) -> u64 {
    let process = process_mask();
    if mask == 0 || mask & !process != 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let cpus = mask_to_cpus(mask);
    match super::kernel32::thread_target(h) {
        Some(super::kernel32::ThreadTarget::Running(tid)) => {
            let old = cpus_to_mask(&cpu::get_affinity(tid));
            if cpu::set_affinity(tid, &cpus) {
                if old == 0 {
                    process
                } else {
                    old
                }
            } else {
                set_last_error(ERROR_ACCESS_DENIED);
                0
            }
        }
        Some(super::kernel32::ThreadTarget::NotStarted(t)) => {
            let mut pending = lock(&t.pending_cpus);
            let old = pending.as_ref().map(|c| cpus_to_mask(c)).unwrap_or(process);
            match t.os_tid.load(Ordering::Acquire) {
                0 => *pending = Some(cpus),
                tid => {
                    cpu::set_affinity(tid, &cpus);
                }
            }
            old
        }
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            0
        }
    }
}

#[repr(C)]
pub struct GroupAffinity {
    mask: u64,
    group: u16,
    reserved: [u16; 3],
}

pub unsafe extern "win64" fn get_thread_group_affinity(h: usize, out: *mut GroupAffinity) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mask = match super::kernel32::thread_target(h) {
        Some(super::kernel32::ThreadTarget::Running(tid)) => cpus_to_mask(&cpu::get_affinity(tid)),
        Some(super::kernel32::ThreadTarget::NotStarted(t)) => lock(&t.pending_cpus)
            .as_ref()
            .map(|c| cpus_to_mask(c))
            .unwrap_or_else(process_mask),
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            return 0;
        }
    };
    out.write(GroupAffinity {
        mask: if mask == 0 { process_mask() } else { mask },
        group: 0,
        reserved: [0; 3],
    });
    1
}

pub unsafe extern "win64" fn set_thread_group_affinity(
    h: usize,
    affinity: *const GroupAffinity,
    previous: *mut GroupAffinity,
) -> i32 {
    if affinity.is_null() || (*affinity).group != 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let old = set_thread_affinity_mask(h, (*affinity).mask);
    if old == 0 {
        return 0;
    }
    if !previous.is_null() {
        previous.write(GroupAffinity {
            mask: old,
            group: 0,
            reserved: [0; 3],
        });
    }
    1
}

/// Le processeur idéal est un indice pour l'ordonnanceur Windows ; Linux n'a
/// pas d'équivalent sans contrainte, il est donc mémorisé et restitué.
pub extern "win64" fn set_thread_ideal_processor(h: usize, ideal: u32) -> u32 {
    let n = logical().len() as u32;
    if ideal != 64 && ideal >= n {
        set_last_error(ERROR_INVALID_PARAMETER);
        return u32::MAX;
    }
    let key = match super::kernel32::thread_target(h) {
        Some(super::kernel32::ThreadTarget::Running(tid)) => tid,
        Some(super::kernel32::ThreadTarget::NotStarted(_)) => -(h as i32),
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            return u32::MAX;
        }
    };
    let mut map = lock(IDEAL.get_or_init(|| Mutex::new(HashMap::new())));
    let old = map.get(&key).copied().unwrap_or(0);
    if ideal != 64 {
        map.insert(key, ideal);
    }
    old
}

fn current_index() -> u32 {
    let cpu = unsafe { libc::sched_getcpu() };
    if cpu < 0 {
        return 0;
    }
    logical()
        .iter()
        .position(|&c| c == cpu as usize)
        .unwrap_or(0) as u32
}

pub extern "win64" fn get_current_processor_number() -> u32 {
    current_index()
}

#[repr(C)]
pub struct ProcessorNumber {
    group: u16,
    number: u8,
    reserved: u8,
}

pub unsafe extern "win64" fn get_current_processor_number_ex(out: *mut ProcessorNumber) {
    if !out.is_null() {
        out.write(ProcessorNumber {
            group: 0,
            number: current_index() as u8,
            reserved: 0,
        });
    }
}

pub extern "win64" fn get_active_processor_group_count() -> u16 {
    1
}
pub extern "win64" fn get_maximum_processor_group_count() -> u16 {
    1
}
pub extern "win64" fn get_maximum_processor_count(group: u16) -> u32 {
    if group == 0 || group == 0xffff {
        logical().len() as u32
    } else {
        0
    }
}

// --- Priorités ---------------------------------------------------------------

pub extern "win64" fn get_priority_class(h: usize) -> u32 {
    if is_current_process(h) {
        PROCESS_CLASS.load(Ordering::Relaxed)
    } else if super::kernel32::process_os_pid(h).is_some() {
        0x20
    } else {
        set_last_error(ERROR_INVALID_HANDLE);
        0
    }
}

/// Comme Windows, « temps réel » sans privilège devient « haute » sans
/// échouer ; une priorité inatteignable sous Linux est ramenée à la plus
/// proche autorisée au lieu de casser le programme.
pub extern "win64" fn set_priority_class(h: usize, class: u32) -> i32 {
    // PROCESS_MODE_BACKGROUND_BEGIN/END : réglage d'E/S, sans effet ici.
    if class == 0x0010_0000 || class == 0x0020_0000 {
        return 1;
    }
    let Some(priority) = Priority::from_class(class) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let privileges = Privileges::current();
    if is_current_process(h) {
        let got = cpu::apply_priority(priority, &privileges);
        PROCESS_CLASS.store(got.class(), Ordering::Relaxed);
        return 1;
    }
    match super::kernel32::process_os_pid(h) {
        Some(pid) => {
            cpu::set_nice(pid, cpu::reachable_nice(priority.nice(), &privileges));
            1
        }
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            0
        }
    }
}

fn process_nice() -> i32 {
    Priority::from_class(PROCESS_CLASS.load(Ordering::Relaxed))
        .map(Priority::nice)
        .unwrap_or(0)
}

/// `nice` d'un thread relatif à la classe du processus.
pub fn thread_nice(base: i32, priority: i32) -> i32 {
    match priority {
        15 => base - 10,
        -15 => 19,
        p => base - 2 * p,
    }
    .clamp(-20, 19)
}

/// Applique seulement si le retour en arrière restera possible : sans
/// privilège, Linux interdit de redescendre un `nice` relevé.
fn apply_thread_priority(tid: i32, priority: i32) {
    let privileges = Privileges::current();
    let base = process_nice();
    let target = thread_nice(base, priority);
    if privileges.nice_floor() <= target.min(base) {
        cpu::set_nice(tid, target);
    }
}

pub(crate) fn thread_started(tid: i32, priority: i32) {
    if priority != 0 {
        lock(THREAD_PRIORITIES.get_or_init(|| Mutex::new(HashMap::new()))).insert(tid, priority);
        apply_thread_priority(tid, priority);
    }
}

pub extern "win64" fn set_thread_priority(h: usize, priority: i32) -> i32 {
    if !matches!(priority, -15 | -2..=2 | 15) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    match super::kernel32::thread_target(h) {
        Some(super::kernel32::ThreadTarget::Running(tid)) => {
            lock(THREAD_PRIORITIES.get_or_init(|| Mutex::new(HashMap::new())))
                .insert(tid, priority);
            apply_thread_priority(tid, priority);
            1
        }
        Some(super::kernel32::ThreadTarget::NotStarted(t)) => {
            let _pending = lock(&t.pending_cpus);
            t.priority.store(priority, Ordering::Relaxed);
            let tid = t.os_tid.load(Ordering::Acquire);
            if tid != 0 {
                thread_started(tid, priority);
            }
            1
        }
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            0
        }
    }
}

pub extern "win64" fn get_thread_priority(h: usize) -> i32 {
    match super::kernel32::thread_target(h) {
        Some(super::kernel32::ThreadTarget::Running(tid)) => {
            lock(THREAD_PRIORITIES.get_or_init(|| Mutex::new(HashMap::new())))
                .get(&tid)
                .copied()
                .unwrap_or(0)
        }
        Some(super::kernel32::ThreadTarget::NotStarted(t)) => t.priority.load(Ordering::Relaxed),
        None => {
            set_last_error(ERROR_INVALID_HANDLE);
            0x7fff_ffff
        }
    }
}

pub extern "win64" fn set_thread_priority_boost(_h: usize, _disable: i32) -> i32 {
    1
}
pub unsafe extern "win64" fn get_thread_priority_boost(_h: usize, disabled: *mut i32) -> i32 {
    if disabled.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *disabled = 0;
    1
}
pub extern "win64" fn set_process_priority_boost(_h: usize, _disable: i32) -> i32 {
    1
}

// --- Topologie -----------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct Cache {
    level: u8,
    kind: u32,
    size: u32,
    line: u16,
    ways: u8,
    mask: u64,
}

fn caches() -> Vec<Cache> {
    let sys = cpu::Sys::host();
    let mut out: Vec<Cache> = Vec::new();
    for (index, linux) in logical().into_iter().enumerate() {
        let base = format!("sys/devices/system/cpu/cpu{linux}/cache");
        for entry in sys.list(&base, "index") {
            let dir = format!("{base}/{entry}");
            let Some(level) = sys.read_num(&format!("{dir}/level")) else {
                continue;
            };
            let kind = match sys.read(&format!("{dir}/type")).as_deref() {
                Some("Unified") => 0,
                Some("Instruction") => 1,
                Some("Data") => 2,
                _ => continue,
            };
            let size = sys.read(&format!("{dir}/size")).unwrap_or_default();
            let size = match size.strip_suffix('K') {
                Some(k) => k.parse::<u32>().unwrap_or(0).saturating_mul(1024),
                None => match size.strip_suffix('M') {
                    Some(m) => m.parse::<u32>().unwrap_or(0).saturating_mul(1024 * 1024),
                    None => size.parse().unwrap_or(0),
                },
            };
            let shared = sys
                .read(&format!("{dir}/shared_cpu_list"))
                .map(|t| cpus_to_mask(&cpu::parse_cpulist(&t)))
                .filter(|m| *m != 0)
                .unwrap_or(1 << index);
            let cache = Cache {
                level: level.clamp(1, 4) as u8,
                kind,
                size,
                line: sys
                    .read_num(&format!("{dir}/coherency_line_size"))
                    .unwrap_or(64)
                    .clamp(1, 4096) as u16,
                ways: sys
                    .read_num(&format!("{dir}/ways_of_associativity"))
                    .unwrap_or(0)
                    .clamp(0, 255) as u8,
                mask: shared,
            };
            if !out.contains(&cache) {
                out.push(cache);
            }
        }
    }
    out
}

/// Cœurs physiques : (masque des processeurs logiques, classe d'efficacité).
fn cores(caps: &Caps) -> Vec<(u64, u8)> {
    let index: Vec<usize> = logical();
    let mut groups: Vec<((usize, usize), u64, u8)> = Vec::new();
    for c in &caps.cpus {
        let Some(i) = index.iter().position(|&l| l == c.id) else {
            continue;
        };
        let class = u8::from(caps.hybrid() && c.kind == CoreKind::Performance);
        match groups
            .iter_mut()
            .find(|(k, _, _)| *k == (c.package, c.core))
        {
            Some((_, mask, _)) => *mask |= 1 << i,
            None => groups.push(((c.package, c.core), 1 << i, class)),
        }
    }
    if groups.is_empty() {
        groups.push(((0, 0), 1, 0));
    }
    groups.into_iter().map(|(_, m, c)| (m, c)).collect()
}

fn packages(caps: &Caps) -> Vec<u64> {
    let index = logical();
    let mut out: Vec<(usize, u64)> = Vec::new();
    for c in &caps.cpus {
        let Some(i) = index.iter().position(|&l| l == c.id) else {
            continue;
        };
        match out.iter_mut().find(|(p, _)| *p == c.package) {
            Some((_, m)) => *m |= 1 << i,
            None => out.push((c.package, 1 << i)),
        }
    }
    if out.is_empty() {
        out.push((0, system_mask()));
    }
    out.into_iter().map(|(_, m)| m).collect()
}

fn nodes(caps: &Caps) -> Vec<(u32, u64)> {
    if !caps.numa() {
        return vec![(0, system_mask())];
    }
    caps.numa_nodes
        .iter()
        .map(|(n, cpus)| (*n as u32, cpus_to_mask(cpus)))
        .filter(|(_, m)| *m != 0)
        .collect()
}

fn put<T: Copy>(buf: &mut Vec<u8>, value: T) {
    let bytes = unsafe {
        std::slice::from_raw_parts(&value as *const T as *const u8, std::mem::size_of::<T>())
    };
    buf.extend_from_slice(bytes);
}

/// SYSTEM_LOGICAL_PROCESSOR_INFORMATION (32 octets en x64).
fn legacy_info(caps: &Caps) -> Vec<u8> {
    let mut out = Vec::new();
    let mut entry = |mask: u64, relation: u32, payload: [u8; 16]| {
        put(&mut out, mask);
        put(&mut out, relation);
        put(&mut out, 0u32);
        out.extend_from_slice(&payload);
    };
    for (mask, _) in cores(caps) {
        let mut p = [0u8; 16];
        p[0] = u8::from(mask.count_ones() > 1);
        entry(mask, 0, p);
    }
    for (node, mask) in nodes(caps) {
        let mut p = [0u8; 16];
        p[..4].copy_from_slice(&node.to_le_bytes());
        entry(mask, 1, p);
    }
    for c in caches() {
        let mut p = [0u8; 16];
        p[0] = c.level;
        p[1] = c.ways;
        p[2..4].copy_from_slice(&c.line.to_le_bytes());
        p[4..8].copy_from_slice(&c.size.to_le_bytes());
        p[8..12].copy_from_slice(&c.kind.to_le_bytes());
        entry(c.mask, 2, p);
    }
    for mask in packages(caps) {
        entry(mask, 3, [0; 16]);
    }
    out
}

fn group_affinity(buf: &mut Vec<u8>, mask: u64) {
    put(buf, mask);
    put(buf, 0u16);
    put(buf, [0u16; 3]);
}

/// SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, entrées de taille variable.
fn extended_info(caps: &Caps, relation: u32) -> Vec<u8> {
    let all = relation == 0xffff;
    let mut out = Vec::new();
    let processor = |out: &mut Vec<u8>, rel: u32, mask: u64, flags: u8, class: u8| {
        put(out, rel);
        put(out, 48u32);
        put(out, flags);
        put(out, class);
        out.extend_from_slice(&[0u8; 20]);
        put(out, 1u16);
        group_affinity(out, mask);
    };
    if all || relation == 0 {
        for (mask, class) in cores(caps) {
            processor(&mut out, 0, mask, u8::from(mask.count_ones() > 1), class);
        }
    }
    if all || relation == 1 || relation == 6 {
        for (node, mask) in nodes(caps) {
            put(&mut out, 1u32);
            put(&mut out, 48u32);
            put(&mut out, node);
            out.extend_from_slice(&[0u8; 18]);
            put(&mut out, 1u16);
            group_affinity(&mut out, mask);
        }
    }
    if all || relation == 2 {
        for c in caches() {
            put(&mut out, 2u32);
            put(&mut out, 56u32);
            put(&mut out, c.level);
            put(&mut out, c.ways);
            put(&mut out, c.line);
            put(&mut out, c.size);
            put(&mut out, c.kind);
            out.extend_from_slice(&[0u8; 18]);
            put(&mut out, 1u16);
            group_affinity(&mut out, c.mask);
        }
    }
    if all || relation == 3 {
        for mask in packages(caps) {
            processor(&mut out, 3, mask, 0, 0);
        }
    }
    if all || relation == 4 {
        let n = logical().len() as u8;
        put(&mut out, 4u32);
        put(&mut out, 80u32);
        put(&mut out, 1u16);
        put(&mut out, 1u16);
        out.extend_from_slice(&[0u8; 20]);
        put(&mut out, n);
        put(&mut out, n);
        out.extend_from_slice(&[0u8; 38]);
        put(&mut out, system_mask());
    }
    out
}

unsafe fn copy_out(data: &[u8], buf: *mut u8, len: *mut u32) -> i32 {
    if len.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let need = data.len() as u32;
    if buf.is_null() || *len < need {
        *len = need;
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(), buf, data.len());
    *len = need;
    1
}

pub unsafe extern "win64" fn get_logical_processor_information(buf: *mut u8, len: *mut u32) -> i32 {
    copy_out(&legacy_info(caps()), buf, len)
}

pub unsafe extern "win64" fn get_logical_processor_information_ex(
    relation: u32,
    buf: *mut u8,
    len: *mut u32,
) -> i32 {
    if !matches!(relation, 0..=4 | 6 | 0xffff) {
        // Dies et modules : non exposés par Linux de façon fiable.
        if !len.is_null() {
            *len = 0;
        }
        return copy_out(&[], buf, len);
    }
    copy_out(&extended_info(caps(), relation), buf, len)
}

// --- NUMA ----------------------------------------------------------------------

pub unsafe extern "win64" fn get_numa_highest_node_number(out: *mut u32) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    *out = nodes(caps()).iter().map(|(n, _)| *n).max().unwrap_or(0);
    1
}

pub unsafe extern "win64" fn get_numa_node_processor_mask(node: u8, out: *mut u64) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    match nodes(caps()).into_iter().find(|(n, _)| *n == node as u32) {
        Some((_, mask)) => {
            *out = mask;
            1
        }
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

pub unsafe extern "win64" fn get_numa_node_processor_mask_ex(
    node: u16,
    out: *mut GroupAffinity,
) -> i32 {
    let mut mask = 0u64;
    if out.is_null() || get_numa_node_processor_mask(node.min(255) as u8, &mut mask) == 0 {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    out.write(GroupAffinity {
        mask,
        group: 0,
        reserved: [0; 3],
    });
    1
}

pub unsafe extern "win64" fn get_numa_processor_node(processor: u8, out: *mut u8) -> i32 {
    if out.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let bit = 1u64.checked_shl(processor as u32).unwrap_or(0);
    match nodes(caps()).into_iter().find(|(_, m)| m & bit != 0) {
        Some((node, _)) => {
            *out = node as u8;
            1
        }
        None => {
            *out = 0xff;
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

pub unsafe extern "win64" fn get_numa_available_memory_node(node: u8, out: *mut u64) -> i32 {
    if out.is_null() || !nodes(caps()).iter().any(|(n, _)| *n == node as u32) {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let meminfo = cpu::Sys::host()
        .read(&format!("sys/devices/system/node/node{node}/meminfo"))
        .or_else(|| cpu::Sys::host().read("proc/meminfo"))
        .unwrap_or_default();
    let free_kb = meminfo
        .lines()
        .find(|l| l.contains("MemFree:"))
        .and_then(|l| l.split_whitespace().rev().nth(1)?.parse::<u64>().ok())
        .unwrap_or(0);
    *out = free_kb.saturating_mul(1024);
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetProcessAffinityMask" => fn get_process_affinity_mask,
        "GetProcessGroupAffinity" => fn get_process_group_affinity,
        "IsProcessInJob" => fn is_process_in_job,
        "QueryInformationJobObject" => fn query_information_job_object,
        "SetThreadStackGuarantee" => fn set_thread_stack_guarantee,
        "SetProcessAffinityMask" => fn set_process_affinity_mask,
        "SetThreadAffinityMask" => fn set_thread_affinity_mask,
        "GetThreadGroupAffinity" => fn get_thread_group_affinity,
        "SetThreadGroupAffinity" => fn set_thread_group_affinity,
        "SetThreadIdealProcessor" => fn set_thread_ideal_processor,
        "GetCurrentProcessorNumber" => fn get_current_processor_number,
        "GetCurrentProcessorNumberEx" => fn get_current_processor_number_ex,
        "GetActiveProcessorGroupCount" => fn get_active_processor_group_count,
        "GetMaximumProcessorGroupCount" => fn get_maximum_processor_group_count,
        "GetMaximumProcessorCount" => fn get_maximum_processor_count,
        "GetPriorityClass" => fn get_priority_class,
        "SetPriorityClass" => fn set_priority_class,
        "GetThreadPriority" => fn get_thread_priority,
        "SetThreadPriority" => fn set_thread_priority,
        "GetThreadPriorityBoost" => fn get_thread_priority_boost,
        "SetThreadPriorityBoost" => fn set_thread_priority_boost,
        "SetProcessPriorityBoost" => fn set_process_priority_boost,
        "GetLogicalProcessorInformation" => fn get_logical_processor_information,
        "GetLogicalProcessorInformationEx" => fn get_logical_processor_information_ex,
        "GetNumaHighestNodeNumber" => fn get_numa_highest_node_number,
        "GetNumaNodeProcessorMask" => fn get_numa_node_processor_mask,
        "GetNumaNodeProcessorMaskEx" => fn get_numa_node_processor_mask_ex,
        "GetNumaProcessorNode" => fn get_numa_processor_node,
        "GetNumaAvailableMemoryNode" => fn get_numa_available_memory_node,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masques_et_affinite_du_thread_courant() {
        let _teb = crate::process::test_teb();
        let system = system_mask();
        assert_ne!(system, 0);
        assert_eq!(cpus_to_mask(&mask_to_cpus(system)), system);
        let (mut p, mut s) = (0u64, 0u64);
        unsafe {
            assert_eq!(get_process_affinity_mask(usize::MAX, &mut p, &mut s), 1);
        }
        assert_eq!(s, system);
        assert_ne!(p & s, 0);
        // Masque hors du système : refusé sans rien modifier.
        if system != u64::MAX {
            assert_eq!(set_thread_affinity_mask(usize::MAX - 1, !system), 0);
            assert_eq!(crate::process::last_error(), ERROR_INVALID_PARAMETER);
        }
        assert_eq!(set_thread_affinity_mask(usize::MAX - 1, 0), 0);
        // Épingler le thread courant sur le premier processeur autorisé puis
        // restaurer : l'ancien masque est renvoyé.
        let first = p & p.wrapping_neg();
        let old = set_thread_affinity_mask(usize::MAX - 1, first);
        assert_ne!(old, 0);
        let mut g = GroupAffinity {
            mask: 0,
            group: 9,
            reserved: [0; 3],
        };
        unsafe {
            assert_eq!(get_thread_group_affinity(usize::MAX - 1, &mut g), 1);
        }
        assert_eq!((g.mask, g.group), (first, 0));
        assert_eq!(set_thread_affinity_mask(usize::MAX - 1, old), first);
        assert_eq!(set_thread_affinity_mask(0x1234_5678, 1), 0);
        assert_eq!(crate::process::last_error(), ERROR_INVALID_HANDLE);
    }

    #[test]
    fn priorites_comme_le_gestionnaire_des_taches() {
        let _teb = crate::process::test_teb();
        assert_eq!(get_priority_class(usize::MAX), 0x20);
        assert_eq!(set_priority_class(usize::MAX, 0x1234), 0);
        assert_eq!(set_thread_priority(usize::MAX - 1, 7), 0);
        assert_eq!(set_thread_priority(usize::MAX - 1, 0), 1);
        assert_eq!(get_thread_priority(usize::MAX - 1), 0);
        assert_eq!(thread_nice(0, 2), -4);
        assert_eq!(thread_nice(0, -15), 19);
        assert_eq!(thread_nice(-10, 15), -20);
        assert_eq!(thread_nice(10, -2), 14);
    }

    #[test]
    fn topologie_win32_coherente() {
        let _teb = crate::process::test_teb();
        let caps = caps();
        let legacy = legacy_info(caps);
        assert_eq!(legacy.len() % 32, 0);
        // Chaque processeur logique appartient à exactement un cœur.
        let cores: u64 = super::cores(caps).iter().fold(0, |m, (c, _)| {
            assert_eq!(m & c, 0);
            m | c
        });
        assert_eq!(cores & system_mask(), cores);
        let ex = extended_info(caps, 0xffff);
        let mut offset = 0;
        while offset < ex.len() {
            let size = u32::from_le_bytes(ex[offset + 4..offset + 8].try_into().unwrap());
            assert!(size >= 48);
            offset += size as usize;
        }
        assert_eq!(offset, ex.len());
        let mut need = 0u32;
        unsafe {
            assert_eq!(
                get_logical_processor_information_ex(0xffff, std::ptr::null_mut(), &mut need),
                0
            );
            assert_eq!(crate::process::last_error(), ERROR_INSUFFICIENT_BUFFER);
            let mut buf = vec![0u8; need as usize];
            assert_eq!(
                get_logical_processor_information_ex(0xffff, buf.as_mut_ptr(), &mut need),
                1
            );
            let mut highest = 9;
            assert_eq!(get_numa_highest_node_number(&mut highest), 1);
            let mut mask = 0;
            assert_eq!(get_numa_node_processor_mask(highest as u8, &mut mask), 1);
            assert_ne!(mask, 0);
            assert_eq!(get_numa_node_processor_mask(200, &mut mask), 0);
            let mut node = 0;
            assert_eq!(get_numa_processor_node(0, &mut node), 1);
            assert_eq!(get_numa_processor_node(255, &mut node), 0);
            assert_eq!(node, 0xff);
        }
    }

    #[test]
    fn thread_suspendu_recoit_son_affinite_au_demarrage() {
        let _teb = crate::process::test_teb();
        extern "win64" fn worker(out: usize) -> u32 {
            unsafe {
                *(out as *mut u64) = cpus_to_mask(&cpu::get_affinity(0));
            }
            0
        }
        let mut seen = 0u64;
        let mut tid = 0u32;
        let process = process_mask();
        let first = process & process.wrapping_neg();
        unsafe {
            let h = super::super::kernel32::create_thread(
                0,
                0,
                worker as usize,
                &mut seen as *mut u64 as usize,
                4,
                &mut tid,
            );
            assert_ne!(h, 0);
            assert_eq!(set_thread_affinity_mask(h, first), process);
            assert_eq!(set_thread_priority(h, -1), 1);
            assert_eq!(get_thread_priority(h), -1);
            super::super::kernel32::resume_thread(h);
            super::super::kernel32::wait_for_single_object(h, 5000);
            super::super::kernel32::close_handle(h);
        }
        assert_eq!(seen, first);
    }
}
