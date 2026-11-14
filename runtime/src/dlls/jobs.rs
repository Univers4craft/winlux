//! Objets job (#19) : un groupe de processus que l'on ferme d'un coup.
//!
//! libuv (Node, Electron), Chromium et les installateurs créent un job avec
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, y rangent leurs enfants (ou s'y
//! rangent eux-mêmes pour que leurs enfants en héritent), puis comptent sur
//! Windows pour tuer tout le groupe quand le job se ferme ou quand ils
//! meurent. Ici, chaque job vit dans le processus qui l'a créé :
//! - `AssignProcessToJobObject` y range le processus (son PID Linux) ;
//! - un processus du job qui en lance un autre l'y range aussi, sauf
//!   `CREATE_BREAKAWAY_FROM_JOB` autorisé par `JOB_OBJECT_LIMIT_BREAKAWAY_OK`,
//!   ou `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK` ;
//! - `TerminateJobObject` tue chaque membre avec le code donné ;
//! - fermer le job, ou la fin du processus qui le tient, tue les membres
//!   quand `KILL_ON_JOB_CLOSE` est demandé ;
//! - `IsProcessInJob` et `QueryInformationJobObject` (limites, comptage,
//!   liste des PID) répondent d'après cet état.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::Mutex;

const KILL_ON_JOB_CLOSE: u32 = 0x2000;
const BREAKAWAY_OK: u32 = 0x0800;
const SILENT_BREAKAWAY_OK: u32 = 0x1000;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

const CLASS_BASIC_ACCOUNTING: u32 = 1;
const CLASS_BASIC_LIMIT: u32 = 2;
const CLASS_PROCESS_ID_LIST: u32 = 3;
const CLASS_EXTENDED_LIMIT: u32 = 9;

/// JOBOBJECT_BASIC_LIMIT_INFORMATION (64 bits) et sa version étendue.
const BASIC_LIMIT_SIZE: usize = 64;
const EXTENDED_LIMIT_SIZE: usize = 144;
const ACCOUNTING_SIZE: usize = 48;

#[derive(Default)]
struct Job {
    /// Copie de JOBOBJECT_EXTENDED_LIMIT_INFORMATION telle que donnée.
    limits: Vec<u8>,
    /// Membres : PID Linux et handle de processus quand on en a un.
    members: Vec<(i32, usize)>,
    total: u32,
    terminated: u32,
    name: Option<String>,
}

impl Job {
    fn flags(&self) -> u32 {
        self.limits
            .get(16..20)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn active(&mut self) -> Vec<(i32, usize)> {
        self.members.retain(|&(pid, _)| alive(pid));
        self.members.clone()
    }
}

static NEXT_JOB: AtomicUsize = AtomicUsize::new(0x4A4F_0000);

fn jobs() -> &'static Mutex<HashMap<usize, Job>> {
    static J: std::sync::OnceLock<Mutex<HashMap<usize, Job>>> = std::sync::OnceLock::new();
    J.get_or_init(|| Mutex::new(HashMap::new()))
}

/// PID à tuer au plantage du processus : membres des jobs
/// KILL_ON_JOB_CLOSE, sauf soi (#2220). Le gestionnaire de signal les lit
/// sans prendre le verrou des jobs, qu'un autre fil (ou le fil qui plante)
/// peut tenir à cet instant ; sous Windows, la fermeture des jobs à la mort
/// du processus ne dépend d'aucun verrou non plus. 0 = case libre.
const CRASH_SLOTS: usize = 512;
static CRASH_KILL: [AtomicI32; CRASH_SLOTS] = [const { AtomicI32::new(0) }; CRASH_SLOTS];

/// Verrou des jobs qui, en le rendant, recopie dans `CRASH_KILL` les
/// membres à tuer au plantage : toute modification passe par lui.
struct Jobs(std::sync::MutexGuard<'static, HashMap<usize, Job>>);

impl std::ops::Deref for Jobs {
    type Target = HashMap<usize, Job>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Jobs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        let me = std::process::id() as i32;
        let wanted: Vec<i32> = self
            .0
            .values()
            .filter(|j| j.flags() & KILL_ON_JOB_CLOSE != 0)
            .flat_map(|j| j.members.iter().map(|&(pid, _)| pid))
            .filter(|&pid| pid > 1 && pid != me)
            .collect();
        for pid in sync_crash_table(&CRASH_KILL, &wanted) {
            crate::log_warn!(
                "jobs",
                "plus de {CRASH_SLOTS} membres KILL_ON_JOB_CLOSE : {pid} ne sera pas tué si le processus plante (il le reste à la fermeture du job et à la fin normale)"
            );
        }
    }
}

/// Met `table` en accord avec `wanted` ; rend les PID qui n'y ont pas
/// trouvé de case (table pleine). Ces PID-là ne sont pas tués au plantage ;
/// la fermeture du job et la fin normale du processus, qui passent par le
/// verrou, les tuent toujours.
fn sync_crash_table(table: &[AtomicI32], wanted: &[i32]) -> Vec<i32> {
    let mut lost = Vec::new();
    // ORDRE VOULU : ajouts d'abord, retraits ensuite. Le gestionnaire de
    // signal lit la table à tout instant, sans verrou : un plantage au
    // milieu de cette mise à jour trouve toujours chaque membre actuel (au
    // pire, il tue aussi un membre qui vient de quitter le job), jamais
    // l'inverse.
    for &pid in wanted {
        if table.iter().any(|c| c.load(Ordering::Relaxed) == pid) {
            continue;
        }
        let placed = table.iter().any(|c| {
            c.compare_exchange(0, pid, Ordering::Release, Ordering::Relaxed)
                .is_ok()
        });
        if !placed {
            lost.push(pid);
        }
    }
    for c in table {
        let pid = c.load(Ordering::Relaxed);
        if pid != 0 && !wanted.contains(&pid) {
            c.store(0, Ordering::Release);
        }
    }
    // Les retraits ont pu libérer des cases : seconde chance.
    lost.retain(|&pid| {
        !table.iter().any(|c| {
            c.compare_exchange(0, pid, Ordering::Release, Ordering::Relaxed)
                .is_ok()
        })
    });
    lost
}

fn lock_jobs() -> Jobs {
    Jobs(crate::dlls::lock(jobs()))
}

fn alive(pid: i32) -> bool {
    if pid == std::process::id() as i32 {
        return true;
    }
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !matches!(
            stat.rfind(')')
                .and_then(|i| stat[i + 1..].trim_start().chars().next()),
            Some('Z' | 'X' | 'x') | None
        ),
        Err(_) => false,
    }
}

fn create(name: Option<String>) -> usize {
    let mut all = lock_jobs();
    if let Some(n) = name.as_deref() {
        if let Some((&h, _)) = all.iter().find(|(_, j)| j.name.as_deref() == Some(n)) {
            crate::process::set_last_error(183); // ERROR_ALREADY_EXISTS
            return h;
        }
    }
    let h = NEXT_JOB.fetch_add(4, Ordering::Relaxed);
    all.insert(
        h,
        Job {
            limits: vec![0; EXTENDED_LIMIT_SIZE],
            name,
            ..Job::default()
        },
    );
    crate::process::set_last_error(0);
    h
}

pub unsafe extern "win64" fn create_job_object_w(_sa: usize, name: *const u16) -> usize {
    let name = (!name.is_null())
        .then(|| crate::dlls::wstr(name))
        .filter(|n| !n.is_empty());
    create(name)
}

pub unsafe extern "win64" fn create_job_object_a(_sa: usize, name: *const u8) -> usize {
    let name = (!name.is_null())
        .then(|| crate::dlls::cstr(name))
        .filter(|n| !n.is_empty());
    create(name)
}

fn open(name: Option<String>) -> usize {
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        crate::process::set_last_error(87);
        return 0;
    };
    let all = lock_jobs();
    match all
        .iter()
        .find(|(_, j)| j.name.as_deref() == Some(name.as_str()))
    {
        Some((&h, _)) => h,
        None => {
            crate::process::set_last_error(2); // ERROR_FILE_NOT_FOUND
            0
        }
    }
}

pub unsafe extern "win64" fn open_job_object_w(
    _access: u32,
    _inherit: i32,
    name: *const u16,
) -> usize {
    open((!name.is_null()).then(|| crate::dlls::wstr(name)))
}

pub unsafe extern "win64" fn open_job_object_a(
    _access: u32,
    _inherit: i32,
    name: *const u8,
) -> usize {
    open((!name.is_null()).then(|| crate::dlls::cstr(name)))
}

fn add_member(job: usize, pid: i32, process: usize) -> bool {
    let mut all = lock_jobs();
    let Some(j) = all.get_mut(&job) else {
        return false;
    };
    if !j.members.iter().any(|&(p, _)| p == pid) {
        j.members.push((pid, process));
        j.total += 1;
    }
    true
}

pub unsafe extern "win64" fn assign_process_to_job_object(job: usize, process: usize) -> i32 {
    let Some(pid) = crate::dlls::kernel32::process_os_pid(process) else {
        crate::process::set_last_error(6); // ERROR_INVALID_HANDLE
        return 0;
    };
    if !add_member(job, pid, process) {
        crate::process::set_last_error(6);
        return 0;
    }
    1
}

pub unsafe extern "win64" fn set_information_job_object(
    job: usize,
    class: u32,
    info: *const u8,
    len: u32,
) -> i32 {
    let mut all = lock_jobs();
    let Some(j) = all.get_mut(&job) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let size = match class {
        CLASS_BASIC_LIMIT => BASIC_LIMIT_SIZE,
        CLASS_EXTENDED_LIMIT => EXTENDED_LIMIT_SIZE,
        // Autres classes (interface, ports de fin d'E/S, CPU) : acceptées
        // sans effet, comme avant.
        _ => return 1,
    };
    if info.is_null() || (len as usize) < size {
        crate::process::set_last_error(24); // ERROR_BAD_LENGTH
        return 0;
    }
    j.limits[..size].copy_from_slice(std::slice::from_raw_parts(info, size));
    1
}

pub unsafe extern "win64" fn query_information_job_object(
    job: usize,
    class: u32,
    info: *mut u8,
    len: u32,
    ret_len: *mut u32,
) -> i32 {
    if info.is_null() && len != 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let mut all = lock_jobs();
    // Job nul : celui du processus appelant (aucun ici, d'où des zéros).
    let mut empty = Job {
        limits: vec![0; EXTENDED_LIMIT_SIZE],
        ..Job::default()
    };
    let j = if job == 0 {
        &mut empty
    } else {
        match all.get_mut(&job) {
            Some(j) => j,
            None => {
                crate::process::set_last_error(6);
                return 0;
            }
        }
    };
    let out = std::slice::from_raw_parts_mut(info, len as usize);
    let written = match class {
        CLASS_BASIC_LIMIT | CLASS_EXTENDED_LIMIT => {
            let size = if class == CLASS_BASIC_LIMIT {
                BASIC_LIMIT_SIZE
            } else {
                EXTENDED_LIMIT_SIZE
            };
            if out.len() < size {
                crate::process::set_last_error(24);
                return 0;
            }
            out[..size].copy_from_slice(&j.limits[..size]);
            size
        }
        CLASS_BASIC_ACCOUNTING => {
            if out.len() < ACCOUNTING_SIZE {
                crate::process::set_last_error(24);
                return 0;
            }
            let active = j.active().len() as u32;
            out[..ACCOUNTING_SIZE].fill(0);
            out[36..40].copy_from_slice(&j.total.to_le_bytes());
            out[40..44].copy_from_slice(&active.to_le_bytes());
            out[44..48].copy_from_slice(&j.terminated.to_le_bytes());
            ACCOUNTING_SIZE
        }
        CLASS_PROCESS_ID_LIST => {
            if out.len() < 16 {
                crate::process::set_last_error(24);
                return 0;
            }
            let pids = j.active();
            let room = (out.len() - 8) / 8;
            let shown = pids.len().min(room);
            out[0..4].copy_from_slice(&(pids.len() as u32).to_le_bytes());
            out[4..8].copy_from_slice(&(shown as u32).to_le_bytes());
            for (i, (pid, _)) in pids.iter().take(shown).enumerate() {
                out[8 + i * 8..16 + i * 8].copy_from_slice(&(*pid as u64).to_le_bytes());
            }
            if shown < pids.len() {
                crate::process::set_last_error(234); // ERROR_MORE_DATA
                if !ret_len.is_null() {
                    *ret_len = (8 + shown * 8) as u32;
                }
                return 0;
            }
            8 + shown * 8
        }
        _ => {
            out.fill(0);
            out.len()
        }
    };
    if !ret_len.is_null() {
        *ret_len = written as u32;
    }
    1
}

pub unsafe extern "win64" fn is_process_in_job(
    process: usize,
    job: usize,
    result: *mut i32,
) -> i32 {
    if result.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    let Some(pid) = crate::dlls::kernel32::process_os_pid(process) else {
        crate::process::set_last_error(6);
        return 0;
    };
    let all = lock_jobs();
    let member = |j: &Job| j.members.iter().any(|&(p, _)| p == pid);
    *result = i32::from(if job == 0 {
        all.values().any(member)
    } else {
        all.get(&job).is_some_and(member)
    });
    1
}

/// Tue les membres vivants (sauf le processus courant) avec `code`.
fn kill_members(members: Vec<(i32, usize)>, code: u32) -> u32 {
    let me = std::process::id() as i32;
    let mut killed = 0;
    for (pid, handle) in members {
        if pid == me {
            continue;
        }
        let ours = handle != 0 && crate::dlls::kernel32::process_os_pid(handle) == Some(pid);
        // Par le handle quand on l'a : GetExitCodeProcess rendra `code`.
        let done = ours && unsafe { crate::dlls::kernel32::terminate_process(handle, code) } != 0;
        if !done {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        killed += 1;
    }
    killed
}

pub unsafe extern "win64" fn terminate_job_object(job: usize, code: u32) -> i32 {
    let members = {
        let mut all = lock_jobs();
        let Some(j) = all.get_mut(&job) else {
            crate::process::set_last_error(6);
            return 0;
        };
        j.active()
    };
    let killed = kill_members(members, code);
    if let Some(j) = lock_jobs().get_mut(&job) {
        j.terminated += killed;
    }
    // Le processus courant fait partie du job : il s'arrête aussi.
    let me = std::process::id() as i32;
    let self_member = lock_jobs()
        .get(&job)
        .is_some_and(|j| j.members.iter().any(|&(p, _)| p == me));
    if self_member {
        crate::dlls::kernel32::terminate_self(code);
    }
    1
}

/// CloseHandle d'un job (vrai si `h` en était un).
pub(crate) fn close(h: usize) -> bool {
    let Some(mut j) = lock_jobs().remove(&h) else {
        return false;
    };
    if j.flags() & KILL_ON_JOB_CLOSE != 0 {
        let members = j.active();
        kill_members(members, 1);
    }
    true
}

/// Fin du processus : ses handles de job se ferment, comme sous Windows.
pub(crate) fn on_exit() {
    let all: Vec<Job> = {
        let Ok(mut g) = jobs().try_lock() else {
            return;
        };
        g.drain().map(|(_, j)| j).collect()
    };
    for mut j in all {
        if j.flags() & KILL_ON_JOB_CLOSE != 0 {
            let members = j.active();
            kill_members(members, 1);
        }
    }
}

/// Plantage du processus (faute non rattrapée) : Windows ferme aussi ses
/// handles, donc ses jobs KILL_ON_JOB_CLOSE tuent leurs membres (#1966).
/// Appelé depuis le gestionnaire de signal : pas d'attente de verrou ni de
/// handle de processus, seulement `kill(SIGKILL)` sur les membres vivants.
pub(crate) fn on_crash() {
    // Appelé depuis le gestionnaire de signal : seulement des opérations
    // async-signal-safe, lecture d'atomiques puis kill(2). Ni verrou, ni
    // allocation, ni journal (#2220). La table ne contient jamais le
    // processus courant ; un PID mort reçoit un kill sans effet.
    for c in &CRASH_KILL {
        let pid = c.load(Ordering::Acquire);
        if pid > 1 {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
}

/// Enfant lancé par CreateProcess : s'il est lancé par un membre d'un job
/// (le processus courant), il rejoint ce job, sauf départ autorisé.
pub(crate) fn on_child_created(pid: i32, process: usize, creation_flags: u32) {
    let me = std::process::id() as i32;
    let mut all = lock_jobs();
    for j in all.values_mut() {
        if !j.members.iter().any(|&(p, _)| p == me) {
            continue;
        }
        let flags = j.flags();
        let breakaway = flags & SILENT_BREAKAWAY_OK != 0
            || (flags & BREAKAWAY_OK != 0 && creation_flags & CREATE_BREAKAWAY_FROM_JOB != 0);
        if !breakaway && !j.members.iter().any(|&(p, _)| p == pid) {
            j.members.push((pid, process));
            j.total += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// on_crash tue les membres KILL_ON_JOB_CLOSE de tous les jobs du
    /// processus, donc aussi ceux d'un autre test qui tourne en parallèle :
    /// les tests qui lancent des membres passent un par un (#2220).
    static SERIE: Mutex<()> = Mutex::new(());

    fn serie() -> std::sync::MutexGuard<'static, ()> {
        SERIE.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn limits(flags: u32) -> Vec<u8> {
        let mut v = vec![0u8; EXTENDED_LIMIT_SIZE];
        v[16..20].copy_from_slice(&flags.to_le_bytes());
        v
    }

    #[test]
    fn job_tue_ses_membres_a_la_fermeture() {
        let _serie = serie();
        let job = create(None);
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        assert!(add_member(job, pid, 0));
        let l = limits(KILL_ON_JOB_CLOSE);
        assert_eq!(
            unsafe {
                set_information_job_object(job, CLASS_EXTENDED_LIMIT, l.as_ptr(), l.len() as u32)
            },
            1
        );
        let mut out = [0u8; ACCOUNTING_SIZE];
        let mut ret = 0u32;
        assert_eq!(
            unsafe {
                query_information_job_object(
                    job,
                    CLASS_BASIC_ACCOUNTING,
                    out.as_mut_ptr(),
                    out.len() as u32,
                    &mut ret,
                )
            },
            1
        );
        assert_eq!(
            u32::from_le_bytes(out[40..44].try_into().unwrap()),
            1,
            "un processus actif"
        );
        assert!(close(job));
        let status = child.wait().unwrap();
        assert!(!status.success(), "l'enfant est tué à la fermeture du job");
        assert!(!lock_jobs().contains_key(&job));
    }

    #[test]
    fn plantage_tue_les_membres_kill_on_close() {
        let _serie = serie();
        let job = create(None);
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        assert!(add_member(job, child.id() as i32, 0));
        let l = limits(KILL_ON_JOB_CLOSE);
        assert_eq!(
            unsafe {
                set_information_job_object(job, CLASS_EXTENDED_LIMIT, l.as_ptr(), l.len() as u32)
            },
            1
        );
        on_crash();
        let status = child.wait().unwrap();
        assert!(!status.success(), "l'enfant est tué au plantage du parent");
        assert!(close(job));
    }

    /// #2220 : table pleine, comportement défini. Les PID sans case sont
    /// rendus (pour le journal), ceux déjà placés restent, et un retrait
    /// libère sa case pour le suivant.
    #[test]
    fn table_de_plantage_pleine() {
        let _serie = serie();
        let t: [AtomicI32; 2] = [const { AtomicI32::new(0) }; 2];
        let get = |t: &[AtomicI32]| {
            let mut v: Vec<i32> = t.iter().map(|c| c.load(Ordering::Relaxed)).collect();
            v.sort();
            v
        };
        assert_eq!(sync_crash_table(&t, &[10, 11, 12]), vec![12]);
        assert_eq!(get(&t), vec![10, 11]);
        // 10 part, 12 arrive : 12 prend la case libérée par le retrait.
        assert_eq!(sync_crash_table(&t, &[11, 12]), Vec::<i32>::new());
        assert_eq!(get(&t), vec![11, 12]);
        assert_eq!(sync_crash_table(&t, &[]), Vec::<i32>::new());
        assert_eq!(get(&t), vec![0, 0]);
    }

    /// #2220 : un autre fil (ou le fil qui plante) tient le verrou des jobs
    /// au moment du plantage ; les membres KILL_ON_JOB_CLOSE meurent quand
    /// même, comme sous Windows où la fermeture ne dépend d'aucun verrou.
    #[test]
    fn plantage_tue_les_membres_meme_verrou_tenu() {
        let _serie = serie();
        let job = create(None);
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        assert!(add_member(job, child.id() as i32, 0));
        let l = limits(KILL_ON_JOB_CLOSE);
        assert_eq!(
            unsafe {
                set_information_job_object(job, CLASS_EXTENDED_LIMIT, l.as_ptr(), l.len() as u32)
            },
            1
        );
        let tenu = lock_jobs();
        on_crash();
        drop(tenu);
        let status = child.wait().unwrap();
        assert!(!status.success(), "tué malgré le verrou tenu");
        assert!(close(job));
    }

    #[test]
    fn job_sans_kill_on_close_laisse_vivre() {
        let _serie = serie();
        let job = create(None);
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        assert!(add_member(job, child.id() as i32, 0));
        assert!(close(job));
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(child.try_wait().unwrap().is_none(), "toujours en vie");
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn terminate_job_object_et_liste_des_pid() {
        let _serie = serie();
        let job = create(Some("winlux-test-job".into()));
        assert_eq!(open(Some("winlux-test-job".into())), job);
        let mut a = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let mut b = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        add_member(job, a.id() as i32, 0);
        add_member(job, b.id() as i32, 0);
        let mut out = [0u8; 8 + 8 * 4];
        let mut ret = 0u32;
        assert_eq!(
            unsafe {
                query_information_job_object(
                    job,
                    CLASS_PROCESS_ID_LIST,
                    out.as_mut_ptr(),
                    out.len() as u32,
                    &mut ret,
                )
            },
            1
        );
        assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()), 2);
        assert_eq!(unsafe { terminate_job_object(job, 7) }, 1);
        assert!(!a.wait().unwrap().success() && !b.wait().unwrap().success());
        let mut acc = [0u8; ACCOUNTING_SIZE];
        unsafe {
            query_information_job_object(
                job,
                CLASS_BASIC_ACCOUNTING,
                acc.as_mut_ptr(),
                acc.len() as u32,
                &mut ret,
            )
        };
        assert_eq!(
            u32::from_le_bytes(acc[44..48].try_into().unwrap()),
            2,
            "deux processus terminés"
        );
        assert_eq!(
            u32::from_le_bytes(acc[36..40].try_into().unwrap()),
            2,
            "deux processus au total"
        );
        close(job);
    }

    #[test]
    fn enfant_d_un_membre_rejoint_le_job_sauf_depart_autorise() {
        let _serie = serie();
        let job = create(None);
        let me = std::process::id() as i32;
        add_member(job, me, 0);
        on_child_created(4_000_001, 0, 0);
        let l = limits(BREAKAWAY_OK);
        unsafe {
            set_information_job_object(job, CLASS_BASIC_LIMIT, l.as_ptr(), BASIC_LIMIT_SIZE as u32)
        };
        on_child_created(4_000_002, 0, CREATE_BREAKAWAY_FROM_JOB);
        on_child_created(4_000_003, 0, 0);
        let all = lock_jobs();
        let pids: Vec<i32> = all[&job].members.iter().map(|&(p, _)| p).collect();
        drop(all);
        assert!(pids.contains(&4_000_001) && pids.contains(&4_000_003));
        assert!(!pids.contains(&4_000_002), "départ demandé et autorisé");
        lock_jobs().remove(&job);
    }
}
