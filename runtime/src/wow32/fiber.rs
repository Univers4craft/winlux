//! Fibres des programmes x86 (étape 7 de #232, WinSCP).
//!
//! Une fibre est un bloc sous 2 Go dont le premier mot est le paramètre
//! (`GetFiberData()` lit `*(fs:[0x10])`, `GetCurrentFiber()` lit
//! `fs:[0x10]`, deux macros des en-têtes publics). Le changement de fibre
//! se fait entièrement en code x86 : on empile les registres que la
//! convention stdcall préserve (ebx, esi, edi, ebp), on note la pile, la
//! chaîne SEH et les bornes de pile du TEB 32 bits, puis on reprend celles
//! de la fibre visée.
//!
//! Disposition du bloc (la nôtre ; seul le premier mot est public) :
//! +0 paramètre, +4 pile sauvegardée, +8 chaîne SEH (fs:[0]),
//! +12 base de pile (fs:[4]), +16 limite de pile (fs:[8]), +20 routine,
//! +24 début de l'allocation, +28 taille de l'allocation, +32 drapeaux.

use std::sync::OnceLock;

const F_PARAM: usize = 0;
const F_ESP: usize = 4;
const F_SEH: usize = 8;
const F_BASE: usize = 12;
const F_LIMIT: usize = 16;
const F_START: usize = 20;
const F_ALLOC: usize = 24;
const F_SIZE: usize = 28;
const F_FLAGS: usize = 32;
/// Fibre née d'un thread (ConvertThreadToFiber) : pas de pile à elle.
const FLAG_THREAD: u32 = 1;

const TEB_FIBER_DATA: usize = 0x10;
const PAGE: usize = 4096;
const DEFAULT_STACK: usize = 1024 * 1024;

const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_ALREADY_FIBER: u32 = 1280;
const ERROR_ALREADY_THREAD: u32 = 1281;

fn rd(a: usize) -> u32 {
    unsafe { (a as *const u32).read_unaligned() }
}

fn wr(a: usize, v: u32) {
    unsafe { (a as *mut u32).write_unaligned(v) }
}

fn teb() -> usize {
    super::teb32_addr()
}

fn set_error(code: u32) {
    crate::dlls::kernel32::set_last_error(code);
}

fn fiber_block(size: usize) -> usize {
    super::alloc_low(size, false)
}

/// ConvertThreadToFiber(Ex) : le thread devient une fibre.
pub extern "win64" fn convert_thread_to_fiber_ex(param: u32, _flags: u32) -> u32 {
    let t = teb();
    if t == 0 {
        set_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if rd(t + TEB_FIBER_DATA) != 0 {
        set_error(ERROR_ALREADY_FIBER);
        return 0;
    }
    let f = fiber_block(PAGE);
    wr(f + F_PARAM, param);
    wr(f + F_SEH, rd(t + super::TEB32_EXCEPTION_LIST));
    wr(f + F_BASE, rd(t + super::TEB32_STACK_BASE));
    wr(f + F_LIMIT, rd(t + super::TEB32_STACK_LIMIT));
    wr(f + F_ALLOC, f as u32);
    wr(f + F_SIZE, PAGE as u32);
    wr(f + F_FLAGS, FLAG_THREAD);
    wr(t + TEB_FIBER_DATA, f as u32);
    f as u32
}

pub extern "win64" fn convert_thread_to_fiber(param: u32) -> u32 {
    convert_thread_to_fiber_ex(param, 0)
}

/// ConvertFiberToThread : le thread n'est plus une fibre.
pub extern "win64" fn convert_fiber_to_thread() -> u32 {
    let t = teb();
    let f = if t == 0 {
        0
    } else {
        rd(t + TEB_FIBER_DATA) as usize
    };
    if f == 0 {
        set_error(ERROR_ALREADY_THREAD);
        return 0;
    }
    wr(t + TEB_FIBER_DATA, 0);
    if rd(f + F_FLAGS) & FLAG_THREAD != 0 {
        unsafe { libc::munmap(f as *mut libc::c_void, rd(f + F_SIZE) as usize) };
    }
    1
}

/// CreateFiberEx : bloc et pile sous 2 Go ; la première bascule vers la
/// fibre « revient » dans le tremplin, qui appelle la routine.
pub extern "win64" fn create_fiber_ex(
    commit: u32,
    reserve: u32,
    _flags: u32,
    start: u32,
    param: u32,
) -> u32 {
    if start == 0 {
        set_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let want = (reserve.max(commit) as usize).max(64 * 1024);
    let stack = if reserve == 0 && commit == 0 {
        DEFAULT_STACK
    } else {
        (want + 0xFFFF) & !0xFFFF
    };
    let size = stack + PAGE;
    let f = fiber_block(size);
    let top = f + size;
    // Pile de départ, du haut vers le bas : argument factice (dépilé par
    // le `ret 4` de SwitchToFiber), adresse de retour = tremplin, puis
    // ebx, esi, edi, ebp à zéro.
    let esp = top - 16 - 4 * 6;
    wr(esp, 0); // ebp
    wr(esp + 4, 0); // edi
    wr(esp + 8, 0); // esi
    wr(esp + 12, 0); // ebx
    wr(esp + 16, code().start);
    wr(esp + 20, 0);
    wr(f + F_PARAM, param);
    wr(f + F_ESP, esp as u32);
    wr(f + F_SEH, 0xFFFF_FFFF);
    wr(f + F_BASE, top as u32);
    wr(f + F_LIMIT, (f + PAGE) as u32);
    wr(f + F_START, start);
    wr(f + F_ALLOC, f as u32);
    wr(f + F_SIZE, size as u32);
    wr(f + F_FLAGS, 0);
    f as u32
}

pub extern "win64" fn create_fiber(stack: u32, start: u32, param: u32) -> u32 {
    create_fiber_ex(stack, stack, 0, start, param)
}

/// DeleteFiber : libère la pile ; supprimer la fibre courante termine le
/// thread, comme sous Windows.
pub extern "win64" fn delete_fiber(f: u32) {
    let f = f as usize;
    if f == 0 {
        return;
    }
    let t = teb();
    if t != 0 && rd(t + TEB_FIBER_DATA) as usize == f {
        crate::dlls::kernel32_misc::exit_thread(0);
    }
    unsafe {
        libc::munmap(
            rd(f + F_ALLOC) as usize as *mut libc::c_void,
            rd(f + F_SIZE) as usize,
        )
    };
}

/// Fin de la routine d'une fibre : le thread se termine.
extern "win64" fn fiber_returned(code: u32) {
    crate::dlls::kernel32_misc::exit_thread(code);
}

struct Code {
    switch: u32,
    start: u32,
    is_fiber: u32,
}

fn code() -> &'static Code {
    static CODE: OnceLock<Code> = OnceLock::new();
    CODE.get_or_init(|| {
        // SwitchToFiber(lpFiber), stdcall.
        #[rustfmt::skip]
        let switch: &[u8] = &[
            0x8B, 0x54, 0x24, 0x04,             // mov edx, [esp+4]
            0x64, 0xA1, 0x10, 0, 0, 0,          // mov eax, fs:[0x10]
            0x53, 0x56, 0x57, 0x55,             // push ebx, esi, edi, ebp
            0x64, 0x8B, 0x0D, 0, 0, 0, 0,       // mov ecx, fs:[0]
            0x89, 0x48, 0x08,                   // mov [eax+8], ecx
            0x64, 0x8B, 0x0D, 4, 0, 0, 0,       // mov ecx, fs:[4]
            0x89, 0x48, 0x0C,                   // mov [eax+12], ecx
            0x64, 0x8B, 0x0D, 8, 0, 0, 0,       // mov ecx, fs:[8]
            0x89, 0x48, 0x10,                   // mov [eax+16], ecx
            0x89, 0x60, 0x04,                   // mov [eax+4], esp
            0x64, 0x89, 0x15, 0x10, 0, 0, 0,    // mov fs:[0x10], edx
            0x8B, 0x4A, 0x08,                   // mov ecx, [edx+8]
            0x64, 0x89, 0x0D, 0, 0, 0, 0,       // mov fs:[0], ecx
            0x8B, 0x4A, 0x0C,                   // mov ecx, [edx+12]
            0x64, 0x89, 0x0D, 4, 0, 0, 0,       // mov fs:[4], ecx
            0x8B, 0x4A, 0x10,                   // mov ecx, [edx+16]
            0x64, 0x89, 0x0D, 8, 0, 0, 0,       // mov fs:[8], ecx
            0x8B, 0x62, 0x04,                   // mov esp, [edx+4]
        ];
        let mut sw = switch.to_vec();
        sw.extend_from_slice(&[
            0x5D, 0x5F, 0x5E, 0x5B, // pop ebp, edi, esi, ebx
            0xC2, 0x04, 0x00, // ret 4
        ]);
        let exit = super::native_stub(
            fiber_returned as *const () as usize,
            "i>v",
            "kernel32!<fin de fibre>",
        ) as u32;
        // Tremplin : start(param), puis fin du thread avec son code.
        let mut st = vec![
            0x64, 0xA1, 0x10, 0, 0, 0, // mov eax, fs:[0x10]
            0xFF, 0x30, // push dword [eax]
            0xFF, 0x50, 0x14, // call dword [eax+20]
            0x50, // push eax
            0xB8, // mov eax, imm32
        ];
        st.extend_from_slice(&exit.to_le_bytes());
        st.extend_from_slice(&[0xFF, 0xD0, 0xCC]); // call eax ; int3
                                                   // IsThreadAFiber.
        let is = [
            0x64, 0xA1, 0x10, 0, 0, 0, // mov eax, fs:[0x10]
            0x85, 0xC0, // test eax, eax
            0x0F, 0x95, 0xC0, // setnz al
            0x0F, 0xB6, 0xC0, // movzx eax, al
            0xC3, // ret
        ];
        Code {
            switch: super::alloc_code(&sw),
            start: super::alloc_code(&st),
            is_fiber: super::alloc_code(&is),
        }
    })
}

/// Fonctions en pur code x86 (pas de passage par la passerelle).
pub fn code_for(kernel: bool, func: &str) -> Option<usize> {
    if !kernel {
        return None;
    }
    Some(match func {
        "SwitchToFiber" => code().switch,
        "IsThreadAFiber" => code().is_fiber,
        _ => return None,
    } as usize)
}

/// Fonctions de fibre servies par la passerelle (signature build.rs).
pub fn override_for(func: &str) -> Option<(usize, &'static str)> {
    Some(match func {
        "ConvertThreadToFiber" => (convert_thread_to_fiber as *const () as usize, "i>i"),
        "ConvertThreadToFiberEx" => (convert_thread_to_fiber_ex as *const () as usize, "ii>i"),
        "ConvertFiberToThread" => (convert_fiber_to_thread as *const () as usize, ">i"),
        "CreateFiber" => (create_fiber as *const () as usize, "iii>i"),
        "CreateFiberEx" => (create_fiber_ex as *const () as usize, "iiiii>i"),
        "DeleteFiber" => (delete_fiber as *const () as usize, "i>v"),
        _ => return None,
    })
}
