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
    };
    reg.blocks.insert(base, block);
    reg.live.insert(ptr, base);
    Some(ptr as *mut libc::c_void)
}

/// Le bloc vient-il de ce mode ? Si oui, il est rendu inaccessible et mis
/// en quarantaine, et la fonction renvoie `true`.
pub unsafe fn free(p: *mut libc::c_void) -> bool {
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
        offset: addr as isize - b.ptr as isize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
