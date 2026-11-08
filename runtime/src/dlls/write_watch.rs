//! Suivi des écritures (`VirtualAlloc(MEM_RESERVE | MEM_WRITE_WATCH)`,
//! `GetWriteWatch`, `ResetWriteWatch`, #965).
//!
//! Windows note les pages d'une région « surveillée » écrites depuis la
//! dernière remise à zéro. Le ramasse-miettes de coreclr et la JVM s'en
//! servent pour leur marquage concurrent. Ici, le noyau Linux tient le même
//! renseignement par page, le bit « soft-dirty » de `/proc/self/pagemap`,
//! remis à zéro par `/proc/self/clear_refs`. Cette remise à zéro vaut pour
//! tout le processus : avant de l'appliquer, les pages déjà écrites des
//! autres régions surveillées sont mises de côté, pour que chaque région
//! garde sa propre histoire.
//!
//! Sans soft-dirty (noyau compilé sans), toute page engagée est déclarée
//! écrite : la réponse est trop large mais jamais fausse, ce qui suffit à
//! un ramasse-miettes (il revisite plus de pages, rien de plus).

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Mutex, OnceLock};

const PAGE: usize = 4096;
/// WRITE_WATCH_FLAG_RESET : GetWriteWatch remet aussi les pages à zéro.
const WRITE_WATCH_FLAG_RESET: u32 = 1;
const ERROR_INVALID_PARAMETER: u32 = 87;

struct Watched {
    size: usize,
    /// Pages (indice dans la région) écrites, mises de côté lors d'une
    /// remise à zéro faite pour une autre région.
    written: BTreeSet<usize>,
}

fn regions() -> &'static Mutex<HashMap<usize, Watched>> {
    static R: OnceLock<Mutex<HashMap<usize, Watched>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<usize, Watched>> {
    regions()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Région réservée avec MEM_WRITE_WATCH.
pub fn register(base: usize, size: usize) {
    let mut r = lock();
    r.insert(
        base,
        Watched {
            size,
            written: BTreeSet::new(),
        },
    );
}

/// Région libérée (VirtualFree MEM_RELEASE).
pub fn unregister(base: usize) {
    lock().remove(&base);
}

/// Le noyau tient-il le bit soft-dirty ? Vérifié une fois sur une page de
/// test : écrite, remise à zéro, relue propre, réécrite, relue sale.
pub fn soft_dirty() -> bool {
    static S: OnceLock<bool> = OnceLock::new();
    *S.get_or_init(|| unsafe {
        let p = libc::mmap(
            std::ptr::null_mut(),
            PAGE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        if p == libc::MAP_FAILED {
            return false;
        }
        let page = p as *mut u8;
        page.write_volatile(1);
        let ok = clear_refs() && dirty_bits(p as usize, 1) == Some(vec![false]) && {
            page.write_volatile(2);
            dirty_bits(p as usize, 1) == Some(vec![true])
        };
        libc::munmap(p, PAGE);
        ok
    })
}

fn clear_refs() -> bool {
    std::fs::OpenOptions::new()
        .write(true)
        .open("/proc/self/clear_refs")
        .and_then(|mut f| f.write_all(b"4"))
        .is_ok()
}

/// Bit soft-dirty de `count` pages à partir de `addr` (aligné).
fn dirty_bits(addr: usize, count: usize) -> Option<Vec<bool>> {
    let mut f = std::fs::File::open("/proc/self/pagemap").ok()?;
    f.seek(SeekFrom::Start((addr / PAGE * 8) as u64)).ok()?;
    let mut raw = vec![0u8; count * 8];
    f.read_exact(&mut raw).ok()?;
    Some(
        raw.as_chunks::<8>()
            .0
            .iter()
            .map(|c| u64::from_le_bytes(*c) >> 55 & 1 == 1)
            .collect(),
    )
}

/// Pages engagées de la région `base` dans [first, end) (indices).
fn committed(base: usize, first: usize, end: usize) -> Vec<usize> {
    crate::dlls::kernel32::committed_pages(base, first, end)
}

/// Remise à zéro globale du noyau : les pages écrites des régions
/// surveillées (sauf `except`, base de la région remise à zéro) sont
/// d'abord mises de côté.
fn clear_all_keeping_others(except: Option<usize>) {
    let mut r = lock();
    for (&base, w) in r.iter_mut() {
        if Some(base) == except {
            continue;
        }
        let pages = committed(base, 0, w.size / PAGE);
        for run in runs(&pages) {
            if let Some(bits) = dirty_bits(base + run.0 * PAGE, run.1 - run.0) {
                for (i, d) in bits.into_iter().enumerate() {
                    if d {
                        w.written.insert(run.0 + i);
                    }
                }
            }
        }
    }
    clear_refs();
}

/// Plages contiguës [début, fin) d'une liste triée d'indices.
fn runs(pages: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &p in pages {
        match out.last_mut() {
            Some(last) if last.1 == p => last.1 = p + 1,
            _ => out.push((p, p + 1)),
        }
    }
    out
}

/// Région surveillée qui contient [addr, addr + size) : (base, première
/// page, fin) en indices.
fn locate(addr: usize, size: usize) -> Option<(usize, usize, usize)> {
    let end = addr.checked_add(size)?;
    let r = lock();
    let (&base, _) = r.iter().find(|(&b, w)| addr >= b && end <= b + w.size)?;
    Some((base, (addr - base) / PAGE, (end - base).div_ceil(PAGE)))
}

/// Pages écrites de la région `base` dans [first, end).
fn written(base: usize, first: usize, end: usize) -> Vec<usize> {
    let pages = committed(base, first, end);
    if !soft_dirty() {
        return pages;
    }
    let kept: BTreeSet<usize> = lock()
        .get(&base)
        .map(|w| w.written.range(first..end).copied().collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for run in runs(&pages) {
        let bits = dirty_bits(base + run.0 * PAGE, run.1 - run.0)
            .unwrap_or_else(|| vec![true; run.1 - run.0]);
        for (i, d) in bits.into_iter().enumerate() {
            let page = run.0 + i;
            if d || kept.contains(&page) {
                out.push(page);
            }
        }
    }
    out
}

/// Remet à zéro [first, end) de la région `base` (soft-dirty présent).
fn reset(base: usize, first: usize, end: usize) {
    clear_all_keeping_others(Some(base));
    if let Some(w) = lock().get_mut(&base) {
        let gone: Vec<usize> = w.written.range(first..end).copied().collect();
        for p in gone {
            w.written.remove(&p);
        }
    }
}

/// `GetWriteWatch(flags, base, size, addresses, &count, &granularity)`.
pub unsafe extern "win64" fn get_write_watch(
    flags: u32,
    base: usize,
    size: usize,
    addresses: *mut usize,
    count: *mut usize,
    granularity: *mut u32,
) -> u32 {
    if flags & !WRITE_WATCH_FLAG_RESET != 0 || count.is_null() || granularity.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 1;
    }
    let Some((region, first, end)) = locate(base, size) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 1;
    };
    let capacity = *count;
    if capacity > 0 && addresses.is_null() {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 1;
    }
    let pages = written(region, first, end);
    let n = pages.len().min(capacity);
    for (i, &p) in pages.iter().take(n).enumerate() {
        *addresses.add(i) = region + p * PAGE;
    }
    *count = n;
    *granularity = PAGE as u32;
    if flags & WRITE_WATCH_FLAG_RESET != 0 {
        // Seules les pages rendues sont remises à zéro.
        let last = pages.get(n.wrapping_sub(1)).map_or(first, |&p| p + 1);
        reset_pages(region, first, if n == pages.len() { end } else { last });
    }
    0
}

/// Remise à zéro qui garde de côté les pages de la région hors de
/// [first, end) déjà écrites.
fn reset_pages(region: usize, first: usize, end: usize) {
    if !soft_dirty() {
        return;
    }
    let size = lock().get(&region).map_or(0, |w| w.size);
    let total = size / PAGE;
    let mut outside: Vec<usize> = written(region, 0, first);
    outside.extend(written(region, end, total));
    reset(region, first, end);
    if let Some(w) = lock().get_mut(&region) {
        w.written.extend(outside);
    }
}

/// `ResetWriteWatch(base, size)`.
pub unsafe extern "win64" fn reset_write_watch(base: usize, size: usize) -> u32 {
    let Some((region, first, end)) = locate(base, size) else {
        crate::process::set_last_error(ERROR_INVALID_PARAMETER);
        return 1;
    };
    reset_pages(region, first, end);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plages_contigues() {
        assert_eq!(runs(&[1, 2, 3, 7, 8, 10]), vec![(1, 4), (7, 9), (10, 11)]);
        assert!(runs(&[]).is_empty());
    }

    #[test]
    fn pages_ecrites_puis_remises_a_zero() {
        unsafe {
            let size = 16 * PAGE;
            let p = crate::dlls::kernel32::virtual_alloc(
                std::ptr::null_mut(),
                size,
                0x2000 | 0x20_0000,
                4,
            ) as usize;
            assert_ne!(p, 0, "MEM_RESERVE | MEM_WRITE_WATCH accepté");
            assert_eq!(
                crate::dlls::kernel32::virtual_alloc(p as *mut _, 8 * PAGE, 0x1000, 4) as usize,
                p
            );
            assert_eq!(reset_write_watch(p, size), 0);
            *((p + 3 * PAGE) as *mut u8) = 1;
            *((p + 5 * PAGE) as *mut u8) = 1;
            let mut out = [0usize; 16];
            let mut n = out.len();
            let mut g = 0u32;
            assert_eq!(
                get_write_watch(0, p, size, out.as_mut_ptr(), &mut n, &mut g),
                0
            );
            assert_eq!(g, 4096);
            let got = &out[..n];
            assert!(got.contains(&(p + 3 * PAGE)) && got.contains(&(p + 5 * PAGE)));
            assert!(
                got.iter().all(|&a| a < p + 8 * PAGE),
                "seulement des pages engagées"
            );
            if soft_dirty() {
                assert_eq!(got, &[p + 3 * PAGE, p + 5 * PAGE]);
                let mut n = out.len();
                get_write_watch(1, p, size, out.as_mut_ptr(), &mut n, &mut g);
                let mut n = out.len();
                get_write_watch(0, p, size, out.as_mut_ptr(), &mut n, &mut g);
                assert_eq!(n, 0, "WRITE_WATCH_FLAG_RESET remet à zéro");
            }
            let mut n = 0usize;
            assert_eq!(
                get_write_watch(0, p + size, PAGE, out.as_mut_ptr(), &mut n, &mut g),
                1
            );
            crate::dlls::kernel32::virtual_free(p as *mut _, 0, 0x8000);
        }
    }
}
