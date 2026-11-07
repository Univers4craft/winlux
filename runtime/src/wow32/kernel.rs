//! Versions x86 des fonctions kernel32 qui remplissent des structures
//! contenant des pointeurs, handles ou SIZE_T (#232, étape 7) : la version
//! 64 bits écrirait une disposition plus longue et déborderait sur les
//! données voisines du programme. Chacune appelle l'implémentation 64 bits
//! sur une copie, puis la recopie à la disposition x86.

use crate::dlls::kernel32 as k32;
use std::ffi::c_void;

unsafe fn put(base: usize, off: usize, v: u32) {
    ((base + off) as *mut u32).write_unaligned(v)
}
unsafe fn put16(base: usize, off: usize, v: u16) {
    ((base + off) as *mut u16).write_unaligned(v)
}
unsafe fn get(base: usize, off: usize) -> u32 {
    ((base + off) as *const u32).read_unaligned()
}

/// Handle ou pointeur 64 bits rendu au programme : -1, -2… gardent leur
/// forme 32 bits, le reste est tronqué (tout est sous 4 Go).
fn narrow(v: usize) -> u32 {
    v as u32
}

/// Handle x86 lu dans une structure : étendu comme un argument.
fn widen(v: u32) -> usize {
    if v >= 0xffff_0000 {
        v as i32 as isize as usize
    } else {
        v as usize
    }
}

/// SYSTEM_INFO x86 (36 octets).
unsafe fn system_info32(out: u32, arch: u16) {
    if out == 0 {
        crate::process::set_last_error(87);
        return;
    }
    let mut si: k32::SystemInfo = std::mem::zeroed();
    k32::get_system_info(&mut si);
    let o = out as usize;
    let cpus = si.number_of_processors.min(32);
    put16(o, 0, arch);
    put16(o, 2, 0);
    put(o, 4, si.page_size);
    put(o, 8, 0x1_0000);
    put(o, 12, (super::limit() - 0x1_0001) as u32);
    put(
        o,
        16,
        if cpus >= 32 {
            u32::MAX
        } else {
            (1u32 << cpus) - 1
        },
    );
    put(o, 20, cpus);
    put(o, 24, if arch == 0 { 586 } else { 8664 });
    put(o, 28, si.allocation_granularity);
    put16(o, 32, si.processor_level);
    put16(o, 34, si.processor_revision);
}

/// `GetSystemInfo` : processeur x86 (PROCESSOR_ARCHITECTURE_INTEL).
pub extern "win64" fn get_system_info(out: u32) {
    unsafe { system_info32(out, 0) }
}

/// `GetNativeSystemInfo` : la vraie machine, AMD64.
pub extern "win64" fn get_native_system_info(out: u32) {
    unsafe { system_info32(out, 9) }
}

/// MEMORY_BASIC_INFORMATION x86 (28 octets).
const MBI32: usize = 28;

unsafe fn memory_info32(addr: u32, info: u32, len: u32, process: Option<u32>) -> u32 {
    if info == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    if (len as usize) < MBI32 {
        crate::process::set_last_error(24); // ERROR_BAD_LENGTH
        return 0;
    }
    let mut m: k32::MemoryBasicInformation = std::mem::zeroed();
    let size = std::mem::size_of::<k32::MemoryBasicInformation>();
    let got = match process {
        Some(h) => k32::virtual_query_ex(widen(h), addr as usize as *const c_void, &mut m, size),
        None => k32::virtual_query(addr as usize as *const c_void, &mut m, size),
    };
    if got == 0 {
        return 0;
    }
    let base = m.base_address as usize;
    // Une région ne dépasse pas ce que voit le programme.
    let end = base.saturating_add(m.region_size).min(1 << 32);
    let o = info as usize;
    put(o, 0, narrow(base));
    put(o, 4, narrow(m.allocation_base as usize));
    put(o, 8, m.allocation_protect);
    put(o, 12, end.saturating_sub(base) as u32);
    put(o, 16, m.state);
    put(o, 20, m.protect);
    put(o, 24, m.kind);
    MBI32 as u32
}

pub extern "win64" fn virtual_query(addr: u32, info: u32, len: u32) -> u32 {
    unsafe { memory_info32(addr, info, len, None) }
}

pub extern "win64" fn virtual_query_ex(process: u32, addr: u32, info: u32, len: u32) -> u32 {
    unsafe { memory_info32(addr, info, len, Some(process)) }
}

/// STARTUPINFO x86 (68 octets).
pub extern "win64" fn get_startup_info(info: u32) {
    if info == 0 {
        crate::process::set_last_error(87);
        return;
    }
    let s = k32::startup_info();
    let o = info as usize;
    unsafe {
        std::ptr::write_bytes(o as *mut u8, 0, 68);
        put(o, 0, 68);
        put(o, 44, s.flags);
        put16(o, 48, s.show_window);
        put(o, 56, narrow(s.std_input));
        put(o, 60, narrow(s.std_output));
        put(o, 64, narrow(s.std_error));
    }
}

/// MEMORYSTATUS x86 (32 octets) : valeurs plafonnées à 4 Go - 1, mémoire
/// virtuelle = espace du programme (2 ou 4 Go).
pub extern "win64" fn global_memory_status(out: u32) {
    if out == 0 {
        return;
    }
    let mut ex = [0u64; 8];
    ex[0] = 64;
    let ok = unsafe { crate::dlls::kernel32_ext::global_memory_status_ex(ex.as_mut_ptr() as _) };
    let o = out as usize;
    let cap = |v: u64| v.min(u32::MAX as u64) as u32;
    unsafe {
        std::ptr::write_bytes(o as *mut u8, 0, 32);
        put(o, 0, 32);
        if ok != 0 {
            put(o, 4, (ex[0] >> 32) as u32);
            put(o, 8, cap(ex[1]));
            put(o, 12, cap(ex[2]));
            put(o, 16, cap(ex[3]));
            put(o, 20, cap(ex[4]));
        }
        let limit = super::limit() as u64;
        put(o, 24, cap(limit - 0x1_0000));
        put(o, 28, cap(limit / 2));
    }
}

/// STARTUPINFO(EX) x86 → disposition 64 bits.
unsafe fn startup64(si: u32, flags: u32) -> Option<Vec<u8>> {
    if si == 0 {
        return None;
    }
    let s = si as usize;
    let cb = get(s, 0) as usize;
    let size = std::mem::size_of::<k32::StartupInfoA>();
    let mut out = vec![0u8; size + 8];
    let mut w = |off: usize, bytes: &[u8]| out[off..off + bytes.len()].copy_from_slice(bytes);
    let extended = flags & 0x80000 != 0 && cb >= 72;
    w(
        0,
        &((if extended { size + 8 } else { size }) as u32).to_le_bytes(),
    );
    for (i, off64) in [(4, 8), (8, 16), (12, 24)] {
        w(off64, &(get(s, i) as usize).to_le_bytes());
    }
    // dwX … dwFlags : huit DWORD, puis wShowWindow et cbReserved2.
    for i in 0..8 {
        w(32 + 4 * i, &get(s, 16 + 4 * i).to_le_bytes());
    }
    w(64, &get(s, 48).to_le_bytes());
    w(72, &(get(s, 52) as usize).to_le_bytes());
    for (i, off64) in [(56, 80), (60, 88), (64, 96)] {
        w(off64, &widen(get(s, i)).to_le_bytes());
    }
    if extended {
        // lpAttributeList : rempli par InitializeProcThreadAttributeList et
        // UpdateProcThreadAttribute de Winlux, déjà à leur disposition.
        w(size, &(get(s, 68) as usize).to_le_bytes());
    }
    Some(out)
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_process32(
    wide: bool,
    app: u32,
    cmd: u32,
    _pattr: u32,
    _tattr: u32,
    inherit: u32,
    flags: u32,
    env: u32,
    cwd: u32,
    si: u32,
    pi: u32,
) -> u32 {
    let mut si64 = startup64(si, flags);
    let si_ptr = si64
        .as_mut()
        .map_or(std::ptr::null(), |v| v.as_ptr() as *const k32::StartupInfoA);
    let mut pi64: k32::ProcessInformation = std::mem::zeroed();
    let pi_ptr = if pi == 0 {
        std::ptr::null_mut()
    } else {
        &mut pi64 as *mut _
    };
    // SECURITY_ATTRIBUTES x86 (12 octets) : seul bInheritHandle compte, et
    // l'héritage passe déjà par `inherit`.
    let r = if wide {
        k32::create_process_w(
            app as usize as *const u16,
            cmd as usize as *mut u16,
            0,
            0,
            inherit as i32,
            flags,
            env as usize as *const c_void,
            cwd as usize as *const u16,
            si_ptr,
            pi_ptr,
        )
    } else {
        k32::create_process_a(
            app as usize as *const u8,
            cmd as usize as *const u8,
            0,
            0,
            inherit as i32,
            flags,
            env as usize as *const c_void,
            cwd as usize as *const u8,
            si_ptr,
            pi_ptr,
        )
    };
    if r != 0 && pi != 0 {
        let o = pi as usize;
        put(o, 0, narrow(pi64.process));
        put(o, 4, narrow(pi64.thread));
        put(o, 8, pi64.process_id);
        put(o, 12, pi64.thread_id);
    }
    r as u32
}

#[allow(clippy::too_many_arguments)]
pub extern "win64" fn create_process_a(
    app: u32,
    cmd: u32,
    pattr: u32,
    tattr: u32,
    inherit: u32,
    flags: u32,
    env: u32,
    cwd: u32,
    si: u32,
    pi: u32,
) -> u32 {
    unsafe {
        create_process32(
            false, app, cmd, pattr, tattr, inherit, flags, env, cwd, si, pi,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub extern "win64" fn create_process_w(
    app: u32,
    cmd: u32,
    pattr: u32,
    tattr: u32,
    inherit: u32,
    flags: u32,
    env: u32,
    cwd: u32,
    si: u32,
    pi: u32,
) -> u32 {
    unsafe {
        create_process32(
            true, app, cmd, pattr, tattr, inherit, flags, env, cwd, si, pi,
        )
    }
}

/// `IsWow64Process` : vrai pour le programme x86 lui-même.
pub extern "win64" fn is_wow64_process(process: u32, out: u32) -> u32 {
    if out == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let me = k32::is_current_process_handle(widen(process));
    unsafe { put(out as usize, 0, me as u32) };
    1
}

/// `IsWow64Process2` : IMAGE_FILE_MACHINE_I386 sur une machine AMD64.
pub extern "win64" fn is_wow64_process2(process: u32, machine: u32, native: u32) -> u32 {
    if machine == 0 {
        crate::process::set_last_error(87);
        return 0;
    }
    let me = k32::is_current_process_handle(widen(process));
    unsafe {
        put16(machine as usize, 0, if me { 0x14c } else { 0 });
        if native != 0 {
            put16(native as usize, 0, 0x8664);
        }
    }
    1
}

fn atomic(p: u32) -> &'static std::sync::atomic::AtomicI32 {
    unsafe { &*(p as usize as *const std::sync::atomic::AtomicI32) }
}

/// `InterlockedIncrement` : rend la nouvelle valeur.
pub extern "win64" fn interlocked_increment(p: u32) -> i32 {
    atomic(p)
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        .wrapping_add(1)
}
pub extern "win64" fn interlocked_decrement(p: u32) -> i32 {
    atomic(p)
        .fetch_sub(1, std::sync::atomic::Ordering::SeqCst)
        .wrapping_sub(1)
}
/// `InterlockedExchange` : rend l'ancienne valeur.
pub extern "win64" fn interlocked_exchange(p: u32, v: i32) -> i32 {
    atomic(p).swap(v, std::sync::atomic::Ordering::SeqCst)
}
pub extern "win64" fn interlocked_exchange_add(p: u32, v: i32) -> i32 {
    atomic(p).fetch_add(v, std::sync::atomic::Ordering::SeqCst)
}
pub extern "win64" fn interlocked_compare_exchange(p: u32, xchg: i32, cmp: i32) -> i32 {
    match atomic(p).compare_exchange(
        cmp,
        xchg,
        std::sync::atomic::Ordering::SeqCst,
        std::sync::atomic::Ordering::SeqCst,
    ) {
        Ok(v) | Err(v) => v,
    }
}
