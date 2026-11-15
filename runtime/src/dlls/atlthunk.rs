//! atlthunk.dll (#2700) : thunks de fenêtre d'ATL.
//!
//! D'après la documentation Learn (atlthunk.h, Windows 10 et suivants) :
//! AtlThunk_AllocateData rend un thunk (NULL en cas d'échec),
//! AtlThunk_InitData lui donne `Proc` et `FirstParameter`,
//! AtlThunk_DataToCode rend le code exécutable, de signature WNDPROC, qui
//! appelle `Proc((HWND)FirstParameter, uMsg, wParam, lParam)`, et
//! AtlThunk_FreeData libère le thunk.
//!
//! Chaque thunk est une case de 32 octets dans une page exécutable ; la
//! donnée et le code sont la même adresse. Code x64 : `mov rcx, First` ;
//! `mov rax, Proc` ; `jmp rax` (les trois autres arguments restent dans
//! rdx, r8 et r9). Servi aux programmes x64 seulement : pour un programme
//! x86, la DLL reste introuvable comme avant (convention d'appel x86 de ces
//! exports non documentée), et ATL garde son propre chemin.

#![allow(non_snake_case)]

use std::sync::Mutex;

const SLOT: usize = 32;
const PAGE: usize = 64 * 1024;
const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;

struct Slots {
    free: Vec<usize>,
}

static SLOTS: Mutex<Slots> = Mutex::new(Slots { free: Vec::new() });

fn lock() -> std::sync::MutexGuard<'static, Slots> {
    SLOTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Code du thunk : `FirstParameter` remplace le premier argument.
fn thunk_code(proc_: usize, first: usize) -> [u8; 22] {
    let mut c = [0u8; 22];
    c[0..2].copy_from_slice(&[0x48, 0xB9]); // mov rcx, imm64
    c[2..10].copy_from_slice(&(first as u64).to_le_bytes());
    c[10..12].copy_from_slice(&[0x48, 0xB8]); // mov rax, imm64
    c[12..20].copy_from_slice(&(proc_ as u64).to_le_bytes());
    c[20..22].copy_from_slice(&[0xFF, 0xE0]); // jmp rax
    c
}

/// x86 : la DLL n'est pas servie (voir l'en-tête du module).
pub(crate) fn hidden_for_x86(name: &str) -> bool {
    crate::wow32::active() && crate::dlls::normalize(name) == "atlthunk"
}

unsafe extern "win64" fn AtlThunk_AllocateData() -> usize {
    let mut s = lock();
    if s.free.is_empty() {
        let page = libc::mmap(
            std::ptr::null_mut(),
            PAGE,
            libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        if page == libc::MAP_FAILED {
            crate::process::set_last_error(ERROR_NOT_ENOUGH_MEMORY);
            return 0;
        }
        let base = page as usize;
        s.free
            .extend((0..PAGE / SLOT).rev().map(|i| base + i * SLOT));
    }
    let slot = s.free.pop().unwrap_or(0);
    if slot != 0 {
        // Avant InitData, le thunk s'arrête net (int3) plutôt que de
        // sauter n'importe où.
        std::ptr::write_bytes(slot as *mut u8, 0xCC, SLOT);
    }
    slot
}

unsafe extern "win64" fn AtlThunk_InitData(thunk: usize, proc_: usize, first: usize) {
    if thunk == 0 {
        return;
    }
    let code = thunk_code(proc_, first);
    std::ptr::copy_nonoverlapping(code.as_ptr(), thunk as *mut u8, code.len());
}

extern "win64" fn AtlThunk_DataToCode(thunk: usize) -> usize {
    thunk
}

unsafe extern "win64" fn AtlThunk_FreeData(thunk: usize) {
    if thunk == 0 {
        return;
    }
    std::ptr::write_bytes(thunk as *mut u8, 0xCC, SLOT);
    lock().free.push(thunk);
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "AtlThunk_AllocateData" => fn AtlThunk_AllocateData,
        "AtlThunk_InitData" => fn AtlThunk_InitData,
        "AtlThunk_DataToCode" => fn AtlThunk_DataToCode,
        "AtlThunk_FreeData" => fn AtlThunk_FreeData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SEEN: Mutex<Vec<(usize, u32, usize, isize)>> = Mutex::new(Vec::new());

    extern "win64" fn proc_(hwnd: usize, msg: u32, wp: usize, lp: isize) -> isize {
        SEEN.lock().unwrap().push((hwnd, msg, wp, lp));
        0x2700
    }

    /// #2700 : le thunk appelle Proc avec FirstParameter à la place du
    /// HWND et les trois autres arguments inchangés.
    #[test]
    fn thunk_atl_remplace_le_premier_argument() {
        unsafe {
            let a = AtlThunk_AllocateData();
            let b = AtlThunk_AllocateData();
            assert!(a != 0 && b != 0 && a != b);
            AtlThunk_InitData(a, proc_ as *const () as usize, 0x1111);
            AtlThunk_InitData(b, proc_ as *const () as usize, 0x2222);
            let wa: extern "win64" fn(usize, u32, usize, isize) -> isize =
                std::mem::transmute(AtlThunk_DataToCode(a));
            let wb: extern "win64" fn(usize, u32, usize, isize) -> isize =
                std::mem::transmute(AtlThunk_DataToCode(b));
            assert_eq!(wa(0xdead, 0x10, 7, -3), 0x2700);
            assert_eq!(wb(0xbeef, 0x110, 8, 9), 0x2700);
            assert_eq!(
                *SEEN.lock().unwrap(),
                [(0x1111, 0x10, 7, -3), (0x2222, 0x110, 8, 9)]
            );
            AtlThunk_FreeData(a);
            // La case libérée resert.
            assert_eq!(AtlThunk_AllocateData(), a);
            AtlThunk_FreeData(a);
            AtlThunk_FreeData(b);
        }
        assert!(crate::dlls::lookup("atlthunk", "AtlThunk_AllocateData").is_some());
    }
}
