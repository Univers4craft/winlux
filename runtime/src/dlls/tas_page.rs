//! Diagnostic `WINLUX_HEAP_DIAG=page` (#2158, pour Acrobat #1818).
//!
//! Sur le modèle du PageHeap que Microsoft documente pour Windows (outil
//! gflags, option « full page heap ») : chaque bloc rendu par HeapAlloc est
//! servi sur ses propres pages, aligné sur 16 et collé à une page de garde
//! inaccessible. HeapFree rend les pages du bloc inaccessibles, libère leur
//! mémoire physique et garde le bloc en quarantaine. Une lecture après
//! libération, ou au-delà de la fin du bloc, fait donc une faute au moment
//! même de l'accès, et le gestionnaire de fautes relie l'adresse fautive à
//! un bloc connu : taille demandée, taille servie, ordre d'allocation et de
//! libération, fil.
//!
//! Avec ce mode, chaque bloc garde aussi les adresses de retour probables
//! relevées sur la pile à son allocation et à sa libération (#2379).
//!
//! Jamais actif par défaut. Code écrit par nous ; aucune source extérieure.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

const PAGE: usize = 4096;
// Chaque bloc vivant coûte deux zones mémoire au noyau (pages du bloc,
// page de garde), un bloc en quarantaine une au plus. Linux en accorde
// 65 530 par processus par défaut (vm.max_map_count) : 2 × 20 000 + 10 000
// en laisse 15 000 au reste du processus (piles, bibliothèques).
/// Blocs libérés gardés inaccessibles ; au-delà, les plus anciens sont rendus.
const QUARANTINE: usize = 10_000;
/// Blocs vivants au-delà desquels l'allocation repasse par malloc.
const MAX_LIVE: usize = 20_000;

#[derive(Clone, Copy)]
struct Block {
    /// Début de la projection (pages du bloc puis page de garde).
    base: usize,
    /// Longueur de la projection, page de garde comprise.
    len: usize,
    /// Adresse rendue au programme.
    ptr: usize,
    /// Taille demandée.
    size: usize,
    alloc_seq: u64,
    alloc_tid: i64,
    /// 0 tant que le bloc est vivant.
    free_seq: u64,
    free_tid: i64,
    /// Adresses de retour relevées sur la pile à l'allocation et à la
    /// libération (0 = case vide).
    alloc_frames: Frames,
    free_frames: Frames,
}

/// Nombre d'adresses de retour gardées par pile.
pub const FRAMES: usize = 12;
pub type Frames = [usize; FRAMES];

thread_local! {
    /// Haut de la pile du fil courant (0 : inconnu).
    static STACK_TOP: usize = stack_top();
}

fn stack_top() -> usize {
    unsafe {
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
            return 0;
        }
        let mut addr: *mut libc::c_void = std::ptr::null_mut();
        let mut size = 0usize;
        let ok = libc::pthread_attr_getstack(&attr, &mut addr, &mut size) == 0;
        libc::pthread_attr_destroy(&mut attr);
        if ok {
            addr as usize + size
        } else {
            0
        }
    }
}

/// Plages exécutables du runtime lui-même : segments PT_LOAD exécutables
/// de l'objet principal, lus une fois par dl_iterate_phdr. Entre
/// `__executable_start` et `etext`, des pages peuvent ne pas être projetées.
fn runtime_text() -> &'static [(usize, usize)] {
    static TEXT: OnceLock<Vec<(usize, usize)>> = OnceLock::new();
    TEXT.get_or_init(|| {
        unsafe extern "C" fn each(
            info: *mut libc::dl_phdr_info,
            _size: libc::size_t,
            data: *mut libc::c_void,
        ) -> libc::c_int {
            let out = &mut *(data as *mut Vec<(usize, usize)>);
            let info = &*info;
            for i in 0..info.dlpi_phnum as usize {
                let ph = &*info.dlpi_phdr.add(i);
                if ph.p_type == libc::PT_LOAD && ph.p_flags & libc::PF_X != 0 {
                    let start = info.dlpi_addr as usize + ph.p_vaddr as usize;
                    out.push((start, start + ph.p_memsz as usize));
                }
            }
            1 // l'objet principal vient en premier : on s'arrête là
        }
        let mut v: Vec<(usize, usize)> = Vec::new();
        unsafe { libc::dl_iterate_phdr(Some(each), &mut v as *mut _ as *mut libc::c_void) };
        v
    })
}

/// Le mot est-il une adresse de retour plausible : dans une plage
/// exécutable, et juste après une instruction call (E8 rel32, ou FF /2
/// sous ses formes de 2, 3, 6 et 7 octets) ? On ne lit que dans la plage.
fn after_call(word: usize, ranges: &[(usize, usize)]) -> bool {
    let Some(&(lo, _)) = ranges.iter().find(|&&(a, b)| word >= a && word < b) else {
        return false;
    };
    let byte = |back: usize| -> Option<u8> {
        let at = word.checked_sub(back)?;
        (at >= lo).then(|| unsafe { std::ptr::read_volatile(at as *const u8) })
    };
    // FF /2 : octet ModRM dont le champ reg vaut 2.
    let ff2 =
        |back: usize| byte(back) == Some(0xFF) && byte(back - 1).is_some_and(|m| m >> 3 & 7 == 2);
    byte(5) == Some(0xE8) || ff2(2) || ff2(3) || ff2(6) || ff2(7)
}

/// Adresses de retour probables sur la pile du fil courant (#2379).
///
/// On balaie au plus 32 Kio sans dépasser le haut de la pile, et on ne garde
/// qu'un mot qui tombe dans une section exécutable (runtime ou module PE) et
/// suit une instruction call. Les premiers cadres sont ceux du runtime
/// (HeapFree, la fonction d'API qui l'appelle) : on garde les
/// `FRAMES / 2` derniers d'entre eux avant le premier cadre PE, puis le
/// premier cadre PE et ce qui le suit.
#[inline(never)]
fn scan_stack() -> Frames {
    let top = STACK_TOP.try_with(|t| *t).unwrap_or(0);
    let here = 0usize;
    let start = (&here as *const usize as usize) & !7;
    if top <= start {
        return [0; FRAMES];
    }
    let end = top.min(start + 32 * 1024);
    let runtime = runtime_text();
    // Sections exécutables des modules PE : instantané sans attendre le
    // verrou (le chargeur peut le tenir en appelant HeapAlloc).
    let mut pe: Vec<(usize, usize)> = Vec::new();
    if let Ok(modules) = crate::loader::MODULES.try_lock() {
        for m in modules.iter() {
            for sec in &m.pe.sections {
                if sec.characteristics & 0x2000_0020 != 0 {
                    let a = m.base + sec.rva as usize;
                    pe.push((a, a + sec.virtual_size as usize));
                }
            }
        }
    }
    let mut at = start;
    let words = std::iter::from_fn(|| {
        (at < end).then(|| {
            let w = unsafe { std::ptr::read_volatile(at as *const usize) };
            at += 8;
            w
        })
    });
    select_frames(words, |w| after_call(w, &pe), |w| after_call(w, runtime))
}

/// Choix des cadres parmi les mots de la pile, du plus proche au plus
/// lointain : les `FRAMES / 2` derniers cadres du runtime avant le premier
/// cadre PE, puis ce cadre et ceux qui suivent ; sans cadre PE, les premiers
/// cadres du runtime, les plus proches de l'appel.
fn select_frames(
    words: impl Iterator<Item = usize>,
    is_pe: impl Fn(usize) -> bool,
    is_runtime: impl Fn(usize) -> bool,
) -> Frames {
    let keep_before = FRAMES / 2;
    let mut first: Vec<usize> = Vec::with_capacity(FRAMES);
    let mut before: VecDeque<usize> = VecDeque::with_capacity(keep_before);
    let mut after: Vec<usize> = Vec::with_capacity(FRAMES);
    for word in words {
        if before.len() + after.len() >= FRAMES {
            break;
        }
        let in_pe = is_pe(word);
        if after.is_empty() && !in_pe {
            if is_runtime(word) && before.back() != Some(&word) {
                if first.len() < FRAMES {
                    first.push(word);
                }
                if before.len() == keep_before {
                    before.pop_front();
                }
                before.push_back(word);
            }
            continue;
        }
        if (in_pe || is_runtime(word)) && after.last() != Some(&word) {
            after.push(word);
        }
    }
    let frames: Vec<usize> = if after.is_empty() {
        first
    } else {
        before.into_iter().chain(after).collect()
    };
    let mut out = [0usize; FRAMES];
    for (slot, w) in out.iter_mut().zip(frames) {
        *slot = w;
    }
    out
}

struct Registry {
    /// Toutes les projections connues (vivantes et en quarantaine), par base.
    blocks: BTreeMap<usize, Block>,
    /// Adresse rendue → base, pour les blocs vivants.
    live: BTreeMap<usize, usize>,
    /// Bases des blocs en quarantaine, du plus ancien au plus récent.
    quarantine: VecDeque<usize>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    blocks: BTreeMap::new(),
    live: BTreeMap::new(),
    quarantine: VecDeque::new(),
});
static SEQ: AtomicU64 = AtomicU64::new(0);
static FALLBACK_SAID: AtomicBool = AtomicBool::new(false);

/// Le mode `page` est-il demandé dans `WINLUX_HEAP_DIAG` ?
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| parse(&std::env::var("WINLUX_HEAP_DIAG").unwrap_or_default()))
}

fn parse(v: &str) -> bool {
    v.split(',').any(|x| x.trim() == "page")
}

fn lock() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}

fn tid() -> i64 {
    unsafe { libc::syscall(libc::SYS_gettid) }
}

fn fallback(why: &str) {
    if !FALLBACK_SAID.swap(true, Ordering::Relaxed) {
        eprintln!("winlux[tas page] : {why} ; les allocations suivantes repassent par malloc");
    }
}

/// Bloc de `size` octets sur ses propres pages, suivi d'une page de garde.
/// `None` : plafond atteint ou projection refusée, l'appelant passe par malloc.
pub unsafe fn alloc(size: usize) -> Option<*mut libc::c_void> {
    let frames = scan_stack();
    let size = size.max(1);
    let served = size.div_ceil(16) * 16;
    let data = served.div_ceil(PAGE) * PAGE;
    let len = data + PAGE;
    let mut reg = lock();
    if reg.live.len() >= MAX_LIVE {
        drop(reg);
        fallback("plus de 20 000 blocs vivants");
        return None;
    }
    // Un programme x86 ne voit que ses 2 premiers Go.
    let low = if crate::wow32::active() {
        libc::MAP_32BIT
    } else {
        0
    };
    let base = libc::mmap(
        std::ptr::null_mut(),
        len,
        libc::PROT_NONE,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE | low,
        -1,
        0,
    );
    if base == libc::MAP_FAILED {
        drop(reg);
        fallback("projection refusée");
        return None;
    }
    let base = base as usize;
    if libc::mprotect(base as *mut _, data, libc::PROT_READ | libc::PROT_WRITE) != 0 {
        libc::munmap(base as *mut _, len);
        drop(reg);
        fallback("mprotect refusé");
        return None;
    }
    let ptr = base + data - served;
    let block = Block {
        base,
        len,
        ptr,
        size,
        alloc_seq: SEQ.fetch_add(1, Ordering::Relaxed) + 1,
        alloc_tid: tid(),
        free_seq: 0,
        free_tid: 0,
        alloc_frames: frames,
        free_frames: [0; FRAMES],
    };
    reg.blocks.insert(base, block);
    reg.live.insert(ptr, base);
    Some(ptr as *mut libc::c_void)
}

/// Le bloc vient-il de ce mode ? Si oui, il est rendu inaccessible et mis
/// en quarantaine, et la fonction renvoie `true`.
pub unsafe fn free(p: *mut libc::c_void) -> bool {
    let frames = scan_stack();
    let mut reg = lock();
    let Some(base) = reg.live.remove(&(p as usize)) else {
        return false;
    };
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    let t = tid();
    let len = match reg.blocks.get_mut(&base) {
        Some(b) => {
            b.free_seq = seq;
            b.free_tid = t;
            b.free_frames = frames;
            b.len
        }
        None => return true,
    };
    libc::mprotect(base as *mut _, len, libc::PROT_NONE);
    libc::madvise(base as *mut _, len, libc::MADV_DONTNEED);
    reg.quarantine.push_back(base);
    while reg.quarantine.len() > QUARANTINE {
        if let Some(old) = reg.quarantine.pop_front() {
            if let Some(b) = reg.blocks.remove(&old) {
                libc::munmap(b.base as *mut _, b.len);
            }
        }
    }
    true
}

/// Le pointeur est-il un bloc vivant de ce mode ?
#[cfg(test)]
fn owns(p: *const libc::c_void) -> bool {
    lock().live.contains_key(&(p as usize))
}

/// Ce que le gestionnaire de fautes écrit pour une valeur : le bloc qui la
/// contient, s'il y en a un. Sans allocation ni verrou bloquant
/// (async-signal-safe) : si le registre est pris, on le dit.
// Rendu par valeur au gestionnaire de fautes, qui ne peut pas allouer.
#[allow(clippy::large_enum_variant)]
pub enum Where {
    Busy,
    None,
    Block {
        ptr: usize,
        size: usize,
        served: usize,
        alloc_seq: u64,
        alloc_tid: i64,
        free_seq: u64,
        free_tid: i64,
        alloc_frames: Frames,
        free_frames: Frames,
        /// Décalage de la valeur par rapport au début du bloc (négatif
        /// avant, au-delà de `size` dans le bourrage ou la page de garde).
        offset: isize,
    },
}

pub fn locate(addr: usize) -> Where {
    let Ok(reg) = REGISTRY.try_lock() else {
        return Where::Busy;
    };
    let Some((_, b)) = reg.blocks.range(..=addr).next_back() else {
        return Where::None;
    };
    if addr >= b.base + b.len {
        return Where::None;
    }
    Where::Block {
        ptr: b.ptr,
        size: b.size,
        served: b.size.div_ceil(16) * 16,
        alloc_seq: b.alloc_seq,
        alloc_tid: b.alloc_tid,
        free_seq: b.free_seq,
        free_tid: b.free_tid,
        alloc_frames: b.alloc_frames,
        free_frames: b.free_frames,
        offset: addr as isize - b.ptr as isize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2379 : un mot n'est gardé que s'il suit une instruction call, lue
    /// sans sortir de la plage exécutable.
    #[test]
    fn adresse_de_retour_seulement_apres_un_call() {
        // call rel32 ; call [rax] (FF 10) ; call qword [rip+d] (FF 15 d32) ; nop
        let code: [u8; 16] = [
            0x90, 0xE8, 1, 2, 3, 4, 0xFF, 0x10, 0xFF, 0x15, 1, 2, 3, 4, 0x90, 0x90,
        ];
        let base = code.as_ptr() as usize;
        let r = [(base, base + code.len())];
        assert!(after_call(base + 6, &r), "après E8 rel32");
        assert!(after_call(base + 8, &r), "après FF /2 de 2 octets");
        assert!(after_call(base + 14, &r), "après FF /2 de 6 octets");
        let nops = [0x90u8; 16];
        let n = nops.as_ptr() as usize;
        assert!(!after_call(n + 12, &[(n, n + 16)]), "après des nop");
        assert!(!after_call(base + 2, &r), "lecture avant la plage refusée");
        assert!(!after_call(base + 6, &[]), "hors de toute plage");
    }

    /// #2379 : les cadres du runtime juste avant le premier cadre PE sont
    /// gardés (la fonction d'API qui libère), puis le cadre PE et la suite.
    #[test]
    fn cadres_autour_du_premier_cadre_pe() {
        // 1..=9 : runtime ; 100.. : PE ; 50 : ni l'un ni l'autre.
        let words = [
            1, 50, 2, 3, 4, 5, 6, 7, 8, 9, 100, 50, 10, 101, 102, 103, 104, 105, 106,
        ];
        let f = select_frames(words.into_iter(), |w| w >= 100, |w| w < 50);
        assert_eq!(f, [4, 5, 6, 7, 8, 9, 100, 10, 101, 102, 103, 104]);
        // Sans cadre PE : les premiers cadres du runtime.
        let f = select_frames((1..30).filter(|w| *w < 50), |_| false, |w| w < 50);
        assert_eq!(f, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    }

    #[test]
    fn mode_page_lu_dans_la_variable() {
        assert!(parse("page"));
        assert!(parse("trace, page"));
        assert!(!parse("trace,nofree"));
        assert!(!parse(""));
    }

    /// Un bloc page est aligné sur 16, collé à sa page de garde, et retrouvé
    /// par le gestionnaire de fautes, vivant puis libéré.
    #[test]
    fn bloc_page_colle_a_sa_garde_et_retrouve_apres_liberation() {
        unsafe {
            let p = alloc(40).expect("projection") as usize;
            assert_eq!(p % 16, 0);
            // 40 → 48 servis : la page de garde commence 48 octets plus loin.
            assert_eq!((p + 48) % PAGE, 0);
            std::ptr::write_bytes(p as *mut u8, 0xAB, 40);
            assert!(owns(p as *const _));
            match locate(p + 8) {
                Where::Block {
                    ptr,
                    size,
                    served,
                    free_seq,
                    offset,
                    ..
                } => {
                    assert_eq!((ptr, size, served, free_seq, offset), (p, 40, 48, 0, 8));
                }
                _ => panic!("bloc vivant introuvable"),
            }
            // Page de garde : retrouvée aussi, au-delà de la taille.
            match locate(p + 48) {
                Where::Block { offset, .. } => assert_eq!(offset, 48),
                _ => panic!("page de garde introuvable"),
            }
            assert!(free(p as *mut _));
            assert!(!owns(p as *const _));
            assert!(!free(p as *mut _), "deuxième libération : plus à nous");
            match locate(p) {
                Where::Block {
                    alloc_seq,
                    free_seq,
                    ..
                } => assert!(free_seq > alloc_seq),
                _ => panic!("bloc libéré introuvable"),
            }
            assert!(matches!(locate(8), Where::None));
        }
    }
}
