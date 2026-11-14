//! Programmes Windows 32 bits (x86) dans le processus 64 bits (#232).
//!
//! Comme le WOW64 de Windows, mais sans noyau à nous : le code x86 du
//! programme tourne en mode compatibilité (sélecteur de code 0x23 sous
//! Linux) et revient en 64 bits (0x33) à chaque appel d'API, pour exécuter
//! les mêmes fonctions `extern "win64"` que les programmes 64 bits.
//!
//! - **Mémoire.** Tout ce que voit le programme doit être sous 4 Go (2 Go
//!   sans `LARGEADDRESSAWARE`). Au lancement, tout l'espace libre au-dessus
//!   est réservé (PROT_NONE) : malloc, HeapAlloc, VirtualAlloc, les piles
//!   des threads et les bibliothèques chargées ensuite tombent toutes en
//!   dessous, sans toucher aux API une par une.
//! - **Passerelles 32 → 64.** Chaque import reçoit un petit morceau de code
//!   x86 qui passe en 64 bits ; `dispatch` relit les arguments sur la pile
//!   32 bits d'après la signature Rust de la fonction (relevée par
//!   `build.rs`) et rend eax, edx:eax ou ST0.
//! - **64 → 32.** `call32` appelle le point d'entrée, DllMain…
//! - **TEB 32 bits sur FS.** Le TLS de glibc utilise aussi FS : sa base est
//!   remise à chaque retour en 64 bits (wrfsbase, ou arch_prctl si le
//!   processeur n'a pas FSGSBASE), et le sélecteur du TEB 32 bits est
//!   rechargé à chaque retour en 32 bits. GS (TEB 64 bits) ne change pas.
//! - **Pile.** Le code 32 bits et les API 64 bits partagent la pile du
//!   thread, qui est sous 4 Go grâce à la réservation.

mod com;
mod com_netfw;
mod com_nlm;
mod com_taskschd;
mod comctl;
mod comdlg;
mod crt;
mod cxx;
mod fiber;
mod kernel;

/// VirtualQuery à la disposition x86 (MEMORY_BASIC_INFORMATION de 28
/// octets), pour NtQueryVirtualMemory appelé par un programme x86.
pub fn virtual_query32(addr: u32, info: u32, len: u32) -> u32 {
    kernel::virtual_query(addr, info, len)
}

/// VirtualQueryEx à la disposition x86, pour NtQueryVirtualMemory sur un
/// autre processus (#1729).
pub fn virtual_query_ex32(process: u32, addr: u32, info: u32, len: u32) -> u32 {
    kernel::virtual_query_ex(process, addr, info, len)
}
mod layout;
mod seh;
mod user;
mod vulkan;
mod wow64;

pub use wow64::{
    apply_env, fs_redirection, pointer_size, read_ptr, registry_view, set_fs_redirection, write_ptr,
};

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// Sélecteurs de segments de Linux x86-64 : code 32 bits, code 64 bits, données.
pub const CS32: u16 = 0x23;
pub const CS64: u16 = 0x33;

/// Cases du TEB 64 bits (champ WOW32Reserved et suivants de Windows,
/// inutilisés ailleurs dans Winlux) lues par le code de transition via GS.
pub(crate) const TEB64_FS_GLIBC: usize = 0x100;
pub(crate) const TEB64_FS_SELECTOR: usize = 0x108;
pub(crate) const TEB64_TEB32: usize = 0x110;
pub(crate) const TEB64_RSP_CHAIN: usize = 0x118;

/// Champs du TEB et du PEB 32 bits.
pub const TEB32_SIZE: usize = 0x2000;
/// Après le TEB 32 bits (0xFFC octets sous Windows) : cases par thread de
/// la CRT x86 (errno, struct tm de localtime…), voir `crt`.
const TEB32_SCRATCH: usize = 0x1000;
const TEB32_EXCEPTION_LIST: usize = 0x00;
const TEB32_STACK_BASE: usize = 0x04;
const TEB32_STACK_LIMIT: usize = 0x08;
const TEB32_SELF: usize = 0x18;
const TEB32_PID: usize = 0x20;
const TEB32_TID: usize = 0x24;
const TEB32_PEB: usize = 0x30;
const TEB32_TLS_POINTER: usize = 0x2c;
/// Tableau TLS statique 32 bits, dans la seconde page du TEB 32 bits.
const TEB32_TLS_ARRAY: usize = 0x1800;
const TLS32_ENTRIES: usize = 512;
const PEB32_IMAGE_BASE: usize = 0x08;
const PEB32_PROCESS_HEAP: usize = 0x18;
const PEB32_NUMBER_OF_PROCESSORS: usize = 0x64;
const PEB32_LDR: usize = 0x0c;
const PEB32_PROCESS_PARAMETERS: usize = 0x10;
const PEB32_LOADER_LOCK: usize = 0xa0;
/// Dans la page du PEB 32 bits, après lui : PEB_LDR_DATA, LoaderLock et
/// RTL_USER_PROCESS_PARAMETERS (#1440).
const PEB32_LDR_DATA_AT: usize = 0x500;
const PEB32_LOADER_LOCK_AT: usize = 0x540;
const PEB32_PARAMS_AT: usize = 0x600;
const PEB32_PARAMS_SIZE: usize = 0x2a0;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PEB32: AtomicU32 = AtomicU32::new(0);
/// Prochaine entrée de LDT libre (une par thread, pour son TEB 32 bits).
static NEXT_LDT: AtomicU32 = AtomicU32::new(1);
/// Entrées de LDT rendues par les threads terminés.
static FREE_LDT: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// Limite haute de la mémoire visible : 2 Go, ou 4 Go si LARGEADDRESSAWARE.
static LIMIT: AtomicUsize = AtomicUsize::new(0x8000_0000);

/// Un programme 32 bits est-il en cours dans ce processus ?
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// `_environ`/`_wenviron` x86 à jour après `_putenv` (#1474).
pub fn refresh_environ() {
    if active() {
        crt::refresh_environ();
    }
}

/// Adresse au-delà de laquelle le programme 32 bits ne voit rien.
pub fn limit() -> usize {
    LIMIT.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------
// Code de transition, assemblé avec le runtime puis recopié en mémoire
// basse : les sauts lointains n'acceptent que des adresses de 32 bits.
// ---------------------------------------------------------------------

std::arch::global_asm!(
    r#"
    .pushsection .rodata.winlux_wow32,"a"
    .balign 16
    .globl winlux_wow_blob_start
winlux_wow_blob_start:

    .code64
// Entrée commune des passerelles d'import, en 64 bits.
// Pile : [esp] EIP de retour de lcall, [esp+4] CS, [esp+8] descripteur,
// [esp+12] adresse de retour de l'appelant x86, [esp+16] arguments.
    .globl winlux_wow_entry64
winlux_wow_entry64:
    mov esp, esp
    push rbp
    push rbx
    push rsi
    push rdi
    mov rbp, rsp
    call winlux_wow_fs_glibc
    mov ecx, dword ptr [rbp + 40]
    lea rdx, [rbp + 48]
    sub rsp, 16
    lea r8, [rbp - 16]
    and rsp, -16
    sub rsp, 32
    call qword ptr [rip + winlux_wow_dispatch_ptr]
    mov ecx, dword ptr [rbp + 40]
    movzx ecx, byte ptr [rcx]
    cmp ecx, 0x64
    jne 1f
    fld qword ptr [rbp - 16]
    jmp 2f
1:
    cmp ecx, 0x66
    jne 2f
    fld dword ptr [rbp - 16]
2:
    mov rdx, rax
    shr rdx, 32
    movzx ecx, word ptr gs:[{fs_selector}]
    mov rsp, rbp
    pop rdi
    pop rsi
    pop rbx
    pop rbp
    // WINLUX_WOW32_STEP : pas à pas du code x86 à partir du retour (TF
    // posé par popfq agit après l'instruction suivante, le retf).
    cmp byte ptr [rip + winlux_wow_step_arm], 0
    je 8f
    mov byte ptr [rip + winlux_wow_step_arm], 0
    pushfq
    or dword ptr [rsp], 0x100
    popfq
8:
    retf

// Remet la base de FS à celle de glibc (TLS du runtime).
// Détruit rax, rcx, r11, rsi, rdi.
winlux_wow_fs_glibc:
    mov rax, qword ptr gs:[{fs_glibc}]
    cmp byte ptr [rip + winlux_wow_has_fsgsbase], 0
    je 3f
    wrfsbase rax
    ret
3:
    push rdx
    mov rsi, rax
    mov edi, 0x1002
    mov eax, 158
    syscall
    pop rdx
    ret

// Appel 64 → 32 (convention win64) : rcx = adresse x86, rdx = arguments
// (u32), r8 = nombre d'arguments ; rend edx:eax dans rax.
    .globl winlux_wow_call32
winlux_wow_call32:
    push rbp
    push rbx
    push rsi
    push rdi
    push r12
    push r13
    push r14
    push r15
    sub rsp, 40
    movdqu xmmword ptr [rsp], xmm6
    movdqu xmmword ptr [rsp + 16], xmm7
    push qword ptr gs:[{rsp_chain}]
    sub rsp, 8
    mov qword ptr gs:[{rsp_chain}], rsp
    mov r11, rcx
    mov eax, r8d
    shl eax, 2
    sub rsp, rax
    and rsp, -16
    sub rsp, 4
    xor ecx, ecx
4:
    cmp rcx, r8
    jae 5f
    mov eax, dword ptr [rdx + rcx * 4]
    mov dword ptr [rsp + rcx * 4 + 4], eax
    inc rcx
    jmp 4b
5:
    lea rax, [rip + winlux_wow_ret32]
    mov dword ptr [rsp], eax
    mov eax, r11d
    movzx ecx, word ptr gs:[{fs_selector}]
    // jmp far m16:32 [rip + winlux_wow_enter32_far], sans REX.W.
    .byte 0xff, 0x2d
    .long winlux_wow_enter32_far - (. + 4)

// Retour du code x86 (atteint par le saut lointain de winlux_wow_ret32).
    .globl winlux_wow_back64
winlux_wow_back64:
    mov esp, esp
    mov r12d, eax
    mov r13d, edx
    call winlux_wow_fs_glibc
    // Chaîne sous la pile x86 : un gestionnaire d'exception x86 a sauté
    // hors d'un appel 64 → 32 plus profond sans le terminer (Delphi,
    // C++Builder). On retrouve le maillon de cet appel-ci (#779).
    mov rax, qword ptr gs:[{rsp_chain}]
    cmp rax, rsp
    jae 6f
    mov rcx, rsp
    and rsp, -16
    sub rsp, 32
    call qword ptr [rip + winlux_wow_repair_ptr]
    mov qword ptr gs:[{rsp_chain}], rax
6:
    mov rsp, qword ptr gs:[{rsp_chain}]
    add rsp, 8
    pop qword ptr gs:[{rsp_chain}]
    movdqu xmm6, xmmword ptr [rsp]
    movdqu xmm7, xmmword ptr [rsp + 16]
    add rsp, 40
    mov eax, r12d
    shl r13, 32
    or rax, r13
    pop r15
    pop r14
    pop r13
    pop r12
    pop rdi
    pop rsi
    pop rbx
    pop rbp
    ret

    .code32
// Entrée en 32 bits : segments de données, TEB 32 bits sur FS, saut.
    .globl winlux_wow_enter32
winlux_wow_enter32:
    // push 0x2b ; pop ds ; push 0x2b ; pop es (pop sur 32 bits : sans
    // préfixe 66, que l'assembleur ajouterait).
    .byte 0x6a, 0x2b, 0x1f, 0x6a, 0x2b, 0x07
    mov fs, cx
    cld
    jmp eax

// Appel d'un filtre ou d'un bloc __finally avec l'ebp de sa fonction
// (cdecl : code, ebp).
    .globl winlux_wow_call_ebp
winlux_wow_call_ebp:
    push ebp
    push ebx
    push esi
    push edi
    mov eax, dword ptr [esp + 20]
    mov ebp, dword ptr [esp + 24]
    call eax
    pop edi
    pop esi
    pop ebx
    pop ebp
    ret

// Adresse de retour donnée au code x86 : retour en 64 bits.
    .globl winlux_wow_ret32
winlux_wow_ret32:
    .byte 0xea
    .globl winlux_wow_ret32_target
winlux_wow_ret32_target:
    .long 0
    .short {cs64}

    .code64
    .balign 8
    .globl winlux_wow_dispatch_ptr
winlux_wow_dispatch_ptr:
    .quad 0
    .globl winlux_wow_repair_ptr
winlux_wow_repair_ptr:
    .quad 0
    .globl winlux_wow_enter32_far
winlux_wow_enter32_far:
    .long 0
    .short {cs32}
    .globl winlux_wow_has_fsgsbase
winlux_wow_has_fsgsbase:
    .byte 0
    .globl winlux_wow_step_arm
winlux_wow_step_arm:
    .byte 0
    .balign 16
    .globl winlux_wow_blob_end
winlux_wow_blob_end:
    .popsection
"#,
    fs_glibc = const TEB64_FS_GLIBC,
    fs_selector = const TEB64_FS_SELECTOR,
    rsp_chain = const TEB64_RSP_CHAIN,
    cs64 = const CS64,
    cs32 = const CS32,
);

// Appel d'une fonction win64 à nombre d'arguments variable.
// rcx = fonction, rdx = cases (au moins 4), r8 = nombre de cases,
// r9 = où ranger xmm0 au retour. Chaque case va à la fois dans le registre
// entier et dans le registre XMM de sa position, comme le veut l'ABI win64.
std::arch::global_asm!(
    r#"
    .text
    .globl winlux_wow_call64
winlux_wow_call64:
    push rbp
    mov rbp, rsp
    push rsi
    push rdi
    push r12
    push r13
    mov r12, rcx
    mov r13, r9
    mov rsi, rdx
    mov rax, r8
    sub rax, 4
    jae 1f
    xor eax, eax
1:
    lea rdx, [rax * 8 + 32 + 15]
    and rdx, -16
    sub rsp, rdx
    and rsp, -16
    xor edx, edx
2:
    cmp rdx, rax
    jae 3f
    mov r10, qword ptr [rsi + rdx * 8 + 32]
    mov qword ptr [rsp + rdx * 8 + 32], r10
    inc rdx
    jmp 2b
3:
    mov rcx, qword ptr [rsi]
    mov rdx, qword ptr [rsi + 8]
    mov r8, qword ptr [rsi + 16]
    mov r9, qword ptr [rsi + 24]
    movq xmm0, rcx
    movq xmm1, rdx
    movq xmm2, r8
    movq xmm3, r9
    call r12
    movq qword ptr [r13], xmm0
    lea rsp, [rbp - 32]
    pop r13
    pop r12
    pop rdi
    pop rsi
    pop rbp
    ret
"#
);

extern "C" {
    static winlux_wow_blob_start: u8;
    static winlux_wow_blob_end: u8;
    static winlux_wow_entry64: u8;
    static winlux_wow_call32: u8;
    static winlux_wow_enter32: u8;
    static winlux_wow_back64: u8;
    static winlux_wow_ret32_target: u8;
    static winlux_wow_dispatch_ptr: u8;
    static winlux_wow_repair_ptr: u8;
    static winlux_wow_enter32_far: u8;
    static winlux_wow_has_fsgsbase: u8;
    static winlux_wow_step_arm: u8;
    static winlux_wow_call_ebp: u8;
}

extern "win64" {
    fn winlux_wow_call64(f: usize, slots: *const u64, n: usize, xmm0: *mut u64) -> u64;
}

/// Adresses (sous 4 Go) du code de transition recopié.
struct Blob {
    entry64: u32,
    /// Octet lu par la sortie des passerelles : pose TF au retour en x86.
    step_arm: usize,
    call32: usize,
    call_ebp: usize,
}

static BLOB: OnceLock<Blob> = OnceLock::new();

fn blob() -> &'static Blob {
    BLOB.get_or_init(|| unsafe {
        let start = std::ptr::addr_of!(winlux_wow_blob_start) as usize;
        let end = std::ptr::addr_of!(winlux_wow_blob_end) as usize;
        let off = |p: *const u8| p as usize - start;
        let base = alloc_low(end - start, true);
        std::ptr::copy_nonoverlapping(start as *const u8, base as *mut u8, end - start);
        let at = |p: *const u8| base + off(p);
        let patch32 = |p: *const u8, v: u32| ((at(p)) as *mut u32).write_unaligned(v);
        patch32(
            std::ptr::addr_of!(winlux_wow_ret32_target),
            at(std::ptr::addr_of!(winlux_wow_back64)) as u32,
        );
        patch32(
            std::ptr::addr_of!(winlux_wow_enter32_far),
            at(std::ptr::addr_of!(winlux_wow_enter32)) as u32,
        );
        (at(std::ptr::addr_of!(winlux_wow_dispatch_ptr)) as *mut u64)
            .write_unaligned(dispatch as *const () as usize as u64);
        (at(std::ptr::addr_of!(winlux_wow_repair_ptr)) as *mut u64)
            .write_unaligned(repair_chain as *const () as usize as u64);
        *(at(std::ptr::addr_of!(winlux_wow_has_fsgsbase)) as *mut u8) = has_fsgsbase() as u8;
        Blob {
            entry64: at(std::ptr::addr_of!(winlux_wow_entry64)) as u32,
            step_arm: at(std::ptr::addr_of!(winlux_wow_step_arm)),
            call32: at(std::ptr::addr_of!(winlux_wow_call32)),
            call_ebp: at(std::ptr::addr_of!(winlux_wow_call_ebp)),
        }
    })
}

fn has_fsgsbase() -> bool {
    const AT_HWCAP2: libc::c_ulong = 26;
    const HWCAP2_FSGSBASE: libc::c_ulong = 1 << 1;
    std::env::var_os("WINLUX_WOW32_NO_FSGSBASE").is_none()
        && unsafe { libc::getauxval(AT_HWCAP2) } & HWCAP2_FSGSBASE != 0
}

// ---------------------------------------------------------------------
// Mémoire basse
// ---------------------------------------------------------------------

struct Pool {
    cur: usize,
    end: usize,
}
static CODE_POOL: Mutex<Pool> = Mutex::new(Pool { cur: 0, end: 0 });

/// Mémoire sous 2 Go (MAP_32BIT), remise à zéro, exécutable si `exec`.
fn alloc_low(size: usize, exec: bool) -> usize {
    let prot = libc::PROT_READ | libc::PROT_WRITE | if exec { libc::PROT_EXEC } else { 0 };
    let p = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            prot,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
            -1,
            0,
        )
    };
    assert!(p != libc::MAP_FAILED, "wow32 : plus de mémoire sous 2 Go");
    p as usize
}

/// Petits morceaux de code x86 (passerelles), sous 2 Go.
pub(super) fn alloc_code(code: &[u8]) -> u32 {
    let mut p = CODE_POOL.lock().unwrap_or_else(|e| e.into_inner());
    let need = (code.len() + 15) & !15;
    if p.cur + need > p.end {
        let size = 64 * 1024;
        p.cur = alloc_low(size, true);
        p.end = p.cur + size;
    }
    let at = p.cur;
    unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), at as *mut u8, code.len()) };
    p.cur += need;
    at as u32
}

/// Lit un fichier entier dans `buf` sans allouer ; rend la longueur lue.
fn read_raw(path: &[u8], buf: &mut [u8]) -> usize {
    unsafe {
        let fd = libc::open(
            path.as_ptr() as *const libc::c_char,
            libc::O_RDONLY | libc::O_CLOEXEC,
        );
        if fd < 0 {
            return 0;
        }
        let mut n = 0usize;
        while n < buf.len() {
            let r = libc::read(
                fd,
                buf[n..].as_mut_ptr() as *mut libc::c_void,
                buf.len() - n,
            );
            if r <= 0 {
                break;
            }
            n += r as usize;
        }
        libc::close(fd);
        n
    }
}

fn hex(s: &[u8]) -> Option<usize> {
    let mut v = 0usize;
    if s.is_empty() {
        return None;
    }
    for &c in s {
        let d = (c as char).to_digit(16)? as usize;
        v = v.checked_mul(16)?.checked_add(d)?;
    }
    Some(v)
}

/// Réserve tout l'espace d'adressage libre au-dessus de `limit`, sauf la
/// marge de croissance de la pile du thread principal. N'alloue rien : elle
/// tourne aussi avant le premier malloc (voir `EARLY`).
fn reserve_high(limit: usize) {
    const TOP: usize = 0x7fff_ffff_f000;
    static MAPS: Mutex<[u8; 1 << 16]> = Mutex::new([0; 1 << 16]);
    let mut maps = MAPS.lock().unwrap_or_else(|e| e.into_inner());
    let len = read_raw(b"/proc/self/maps\0", &mut maps[..]);
    let text = &maps[..len];
    // Zones occupées, dans l'ordre croissant (c'est l'ordre de maps).
    let mut used = [(0usize, 0usize); 512];
    let mut count = 0;
    let mut stack_low = 0usize;
    for line in text.split(|&c| c == b'\n') {
        let Some(sp) = line.iter().position(|&c| c == b' ') else {
            continue;
        };
        let range = &line[..sp];
        let Some(dash) = range.iter().position(|&c| c == b'-') else {
            continue;
        };
        let (Some(a), Some(b)) = (hex(&range[..dash]), hex(&range[dash + 1..])) else {
            continue;
        };
        if line.ends_with(b"[stack]") {
            stack_low = a;
        }
        if count < used.len() {
            used[count] = (a, b);
            count += 1;
        }
    }
    let grow = {
        let mut rl = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if unsafe { libc::getrlimit(libc::RLIMIT_STACK, &mut rl) } == 0
            && rl.rlim_cur != libc::RLIM_INFINITY
        {
            (rl.rlim_cur as usize).clamp(8 << 20, 256 << 20)
        } else {
            256 << 20
        }
    };
    let reserve = |from: usize, to: usize| {
        if to <= from {
            return;
        }
        unsafe {
            libc::mmap(
                from as *mut _,
                to - from,
                libc::PROT_NONE,
                libc::MAP_PRIVATE
                    | libc::MAP_ANONYMOUS
                    | libc::MAP_NORESERVE
                    | libc::MAP_FIXED_NOREPLACE,
                -1,
                0,
            );
        }
    };
    // Comme WOW64, les 64 derniers Ko sous 4 Go restent inaccessibles : une
    // adresse x86 de 0xffff0000 à 0xffffffff se confondrait avec un entier
    // négatif (`widen` l'étend par le signe) ; wcschr de pygame32 recevait
    // ainsi 0xffffffffffffec90 (#1438).
    if limit == 0x1_0000_0000 {
        reserve(0xffff_0000, limit);
    }
    let mut cursor = limit;
    for &(a, b) in &used[..count] {
        // La pile principale grandit vers le bas : on lui laisse sa limite.
        let a = if a == stack_low {
            a.saturating_sub(grow)
        } else {
            a
        };
        if b <= cursor {
            continue;
        }
        if a > cursor {
            reserve(cursor, a.min(TOP));
        }
        cursor = cursor.max(b);
        if cursor >= TOP {
            return;
        }
    }
    reserve(cursor, TOP);
}

/// Limite choisie par `early_init`, 0 si le programme n'est pas x86.
static EARLY_LIMIT: AtomicUsize = AtomicUsize::new(0);

/// Variable d'environnement posée par `reexec` : le constructeur du
/// nouveau processus fait alors la réservation avant le premier malloc.
const REEXEC_VAR: &str = "WINLUX_WOW32_EXEC";

/// Constructeur lancé avant `main` et avant le premier malloc : si le
/// programme demandé est x86 (et que `reexec` l'a demandé), la réservation
/// au-dessus de la limite empêche le tas de glibc (brk) de s'installer
/// au-dessus ; il passe alors par mmap, en dessous. Faite plus tard, le tas
/// déjà ouvert resterait en haut. Les autres lancements (installateurs
/// déroulés par Winlux, programmes 64 bits) gardent tout l'espace.
#[used]
#[link_section = ".init_array"]
static EARLY: extern "C" fn(i32, *const *const libc::c_char, *const *const libc::c_char) =
    early_init;

extern "C" fn early_init(
    argc: i32,
    argv: *const *const libc::c_char,
    _env: *const *const libc::c_char,
) {
    if argc < 2 || argv.is_null() {
        return;
    }
    let var = b"WINLUX_WOW32_EXEC\0";
    if unsafe { libc::getenv(var.as_ptr() as *const libc::c_char) }.is_null() {
        return;
    }
    // Pas d'héritage par les processus enfants : chacun refait le test.
    unsafe { libc::unsetenv(var.as_ptr() as *const libc::c_char) };
    let mut path = unsafe { *argv.add(1) };
    // Action personnalisée DLL d'un paquet MSI (#833) : la DLL suit.
    if !path.is_null()
        && argc >= 3
        && unsafe { std::ffi::CStr::from_ptr(path) }.to_bytes() == b"--msi-dll-action"
    {
        path = unsafe { *argv.add(2) };
    }
    if path.is_null() || unsafe { *path } == b'-' as libc::c_char {
        return;
    }
    let mut head = [0u8; 4096];
    let n = unsafe {
        let fd = libc::open(path, libc::O_RDONLY | libc::O_CLOEXEC);
        if fd < 0 {
            return;
        }
        let r = libc::read(fd, head.as_mut_ptr() as *mut libc::c_void, head.len());
        libc::close(fd);
        r.max(0) as usize
    };
    if let Some(laa) = x86_header(&head[..n]) {
        let limit = if laa { 0x1_0000_0000 } else { 0x8000_0000 };
        reserve_high(limit);
        EARLY_LIMIT.store(limit, Ordering::Relaxed);
    }
}

/// Relance le runtime pour que `early_init` réserve la mémoire haute avant
/// le premier malloc. Ne revient qu'en cas d'échec d'exec.
pub fn reexec_if_needed() {
    if EARLY_LIMIT.load(Ordering::Relaxed) != 0
        || std::env::var_os("WINLUX_WOW32_NO_REEXEC").is_some()
    {
        return;
    }
    use std::os::unix::process::CommandExt;
    let mut args = std::env::args_os();
    let _ = args.next();
    let error = std::process::Command::new("/proc/self/exe")
        .arg0(std::env::args_os().next().unwrap_or_default())
        .args(args)
        .env(REEXEC_VAR, "1")
        .exec();
    crate::trace!("wow32 : relance impossible ({error}), réservation tardive");
}

/// En-tête d'un programme x86 natif (PE32, machine 0x14c, sans en-tête
/// CLR) : rend s'il est LARGEADDRESSAWARE.
fn x86_header(b: &[u8]) -> Option<bool> {
    use crate::pe::{rd16, rd32};
    if b.get(..2)? != b"MZ" {
        return None;
    }
    let pe = rd32(b, 0x3c)? as usize;
    if b.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let coff = pe + 4;
    let opt = coff + 20;
    if rd16(b, coff)? != crate::pe::IMAGE_FILE_MACHINE_I386
        || rd16(b, opt)? != crate::pe::PE32_MAGIC
    {
        return None;
    }
    let ndirs = rd32(b, opt + 92)? as usize;
    if ndirs > crate::pe::DIR_COM_DESCRIPTOR
        && rd32(b, opt + 96 + crate::pe::DIR_COM_DESCRIPTOR * 8)? != 0
    {
        return None;
    }
    Some(rd16(b, coff + 18)? & 0x0020 != 0)
}

/// Prépare le processus pour un programme x86, avant tout chargement.
pub fn init(large_address_aware: bool) {
    let limit = if large_address_aware {
        0x1_0000_0000
    } else {
        0x8000_0000
    };
    LIMIT.store(limit, Ordering::Relaxed);
    unsafe {
        // Chaque arène de glibc réserve 64 Mo : deux suffisent sous 4 Go.
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
    if EARLY_LIMIT.load(Ordering::Relaxed) != limit {
        crate::trace!("wow32 : réservation tardive, le tas de glibc peut rester au-dessus de 4 Go");
    }
    // Zones ouvertes depuis le constructeur (bibliothèques, piles) : à
    // nouveau tout ce qui est libre au-dessus.
    reserve_high(limit);
    ACTIVE.store(true, Ordering::Relaxed);
    let _ = blob();
    crate::trace!(
        "wow32 : programme x86, mémoire sous 0x{limit:x}, FSGSBASE={}",
        has_fsgsbase()
    );
}

// ---------------------------------------------------------------------
// TEB et PEB 32 bits
// ---------------------------------------------------------------------

/// PEB 32 bits, à appeler une fois l'image chargée.
pub fn init_peb32(image_base: usize) {
    let peb = alloc_low(0x1000, false);
    unsafe {
        let w = |o: usize, v: u32| *((peb + o) as *mut u32) = v;
        w(PEB32_IMAGE_BASE, image_base as u32);
        w(
            PEB32_PROCESS_HEAP,
            crate::dlls::kernel32::process_heap() as u32,
        );
        let n = std::thread::available_parallelism().map_or(1, |n| n.get());
        w(PEB32_NUMBER_OF_PROCESSORS, n as u32);
        // OSMajorVersion, OSMinorVersion, OSBuildNumber, OSPlatformId : Windows 10.
        w(0xa4, 10);
        w(0xa8, 0);
        *((peb + 0xac) as *mut u16) = 19045;
        w(0xb0, 2);
        fill_peb32_tables(peb);
    }
    PEB32.store(peb as u32, Ordering::Relaxed);
}

/// Champs du PEB 32 bits que du code x86 lit sans passer par une API :
/// PEB->Ldr (listes vides mais bouclées), PEB->LoaderLock (section libre)
/// et PEB->ProcessParameters (drapeaux, poignées standard, chemin de
/// l'image, ligne de commande), copiés du PEB 64 bits. Nuls, la CRT
/// Microsoft liée en statique plantait en lisant [0+8] à la sortie de
/// pyi32.exe et de Steam.exe (#1440). Disposition x86 de winternl.h.
unsafe fn fill_peb32_tables(peb: usize) {
    let w = |a: usize, v: u32| *(a as *mut u32) = v;
    // PEB_LDR_DATA : Length, Initialized, puis trois LIST_ENTRY (InLoadOrder
    // à 0x0c, InMemoryOrder à 0x14, InInitializationOrder à 0x1c).
    let ldr = peb + PEB32_LDR_DATA_AT;
    w(ldr, 0x30);
    w(ldr + 4, 1);
    for head in [0x0c, 0x14, 0x1c] {
        w(ldr + head, (ldr + head) as u32);
        w(ldr + head + 4, (ldr + head) as u32);
    }
    w(peb + PEB32_LDR, ldr as u32);
    // RTL_CRITICAL_SECTION x86 libre : LockCount = -1.
    let lock = peb + PEB32_LOADER_LOCK_AT;
    w(lock + 4, u32::MAX);
    w(peb + PEB32_LOADER_LOCK, lock as u32);

    let params = peb + PEB32_PARAMS_AT;
    w(params, PEB32_PARAMS_SIZE as u32);
    w(params + 4, PEB32_PARAMS_SIZE as u32);
    w(params + 8, 1); // RTL_USER_PROC_PARAMS_NORMALIZED
    w(params + 0x18, crate::dlls::kernel32::STD_INPUT_REAL as u32);
    w(params + 0x1c, crate::dlls::kernel32::STD_OUTPUT_REAL as u32);
    w(params + 0x20, crate::dlls::kernel32::STD_ERROR_REAL as u32);
    let p64 = match crate::process::peb() {
        0 => 0,
        p => *((p + crate::process::PEB_PROCESS_PARAMETERS) as *const usize),
    };
    let read64 = |off: usize| -> Vec<u16> {
        if p64 == 0 {
            return Vec::new();
        }
        let len = *((p64 + off) as *const u16) as usize / 2;
        let buf = *((p64 + off + 8) as *const usize) as *const u16;
        if buf.is_null() {
            return Vec::new();
        }
        std::slice::from_raw_parts(buf, len).to_vec()
    };
    let mut dir: Vec<u16> = std::env::current_dir()
        .map(|d| crate::paths::to_windows(&d))
        .unwrap_or_default()
        .encode_utf16()
        .collect();
    if dir.last() != Some(&(b'\\' as u16)) {
        dir.push(b'\\' as u16);
    }
    // CurrentDirectory.DosPath (0x24), DllPath (0x30), ImagePathName (0x38),
    // CommandLine (0x40) : UNICODE_STRING x86 (longueurs puis tampon).
    let strings = [
        (0x24, dir),
        (0x30, Vec::new()),
        (0x38, read64(0x60)),
        (0x40, read64(0x70)),
    ];
    let bytes: usize = strings.iter().map(|(_, s)| s.len() * 2 + 2).sum();
    let mut at = alloc_low(bytes.next_multiple_of(0x1000), false);
    for (off, s) in strings {
        std::ptr::copy_nonoverlapping(s.as_ptr(), at as *mut u16, s.len());
        *((at + s.len() * 2) as *mut u16) = 0;
        *((params + off) as *mut u16) = (s.len() * 2) as u16;
        *((params + off + 2) as *mut u16) = (s.len() * 2 + 2) as u16;
        w(params + off + 4, at as u32);
        at += s.len() * 2 + 2;
    }
    w(peb + PEB32_PROCESS_PARAMETERS, params as u32);
}

#[repr(C)]
struct UserDesc {
    entry_number: u32,
    base_addr: u32,
    limit: u32,
    flags: u32,
}

/// TEB 32 bits du thread courant, sur FS. `teb64` est déjà sur GS.
pub fn init_thread(teb64: usize) {
    if !active() {
        return;
    }
    let teb = alloc_low(TEB32_SIZE, false);
    unsafe {
        let (mut addr, mut size) = (std::ptr::null_mut(), 0usize);
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) == 0 {
            libc::pthread_attr_getstack(&attr, &mut addr, &mut size);
            libc::pthread_attr_destroy(&mut attr);
        }
        let w = |o: usize, v: u32| *((teb + o) as *mut u32) = v;
        w(TEB32_EXCEPTION_LIST, u32::MAX);
        w(TEB32_STACK_BASE, (addr as usize + size) as u32);
        w(TEB32_STACK_LIMIT, addr as u32);
        w(TEB32_SELF, teb as u32);
        w(TEB32_PID, libc::getpid() as u32);
        w(TEB32_TID, libc::gettid() as u32);
        w(TEB32_PEB, PEB32.load(Ordering::Relaxed));

        // Entrée de LDT propre au thread, dont la base est le TEB 32 bits.
        // (set_thread_area n'existe pas en appel système 64 bits.)
        let entry = FREE_LDT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop()
            .unwrap_or_else(|| NEXT_LDT.fetch_add(1, Ordering::Relaxed));
        assert!(entry < 8192, "wow32 : plus d'entrée de LDT libre");
        let desc = UserDesc {
            entry_number: entry,
            base_addr: teb as u32,
            limit: (TEB32_SIZE - 1) as u32,
            // seg_32bit | useable
            flags: 1 | (1 << 6),
        };
        let rc = libc::syscall(
            libc::SYS_modify_ldt,
            1,
            &desc as *const UserDesc,
            std::mem::size_of::<UserDesc>(),
        );
        assert!(
            rc == 0,
            "wow32 : modify_ldt a échoué ({})",
            std::io::Error::last_os_error()
        );
        // Sélecteur : index, table locale (TI=1), niveau 3.
        let selector = (entry << 3) | 4 | 3;
        let mut fs_base = 0usize;
        libc::syscall(libc::SYS_arch_prctl, 0x1003, &mut fs_base as *mut usize);
        *((teb64 + TEB64_FS_GLIBC) as *mut usize) = fs_base;
        *((teb64 + TEB64_FS_SELECTOR) as *mut usize) = selector as usize;
        *((teb64 + TEB64_TEB32) as *mut usize) = teb;
        TEB32_ADDR.with(|c| c.set(teb));
        // TLS statique (__declspec(thread)) : fs:[0x2c] pointe sur un tableau
        // de pointeurs 32 bits, copie de celui du TEB 64 bits.
        let arr64 = *((teb64 + crate::process::TEB_TLS_POINTER) as *const usize);
        if arr64 != 0 {
            for i in 0..TLS32_ENTRIES {
                let block = *((arr64 + i * 8) as *const usize);
                *((teb + TEB32_TLS_ARRAY + i * 4) as *mut u32) = block as u32;
            }
        }
        w(TEB32_TLS_POINTER, (teb + TEB32_TLS_ARRAY) as u32);
        *((teb64 + TEB64_RSP_CHAIN) as *mut usize) = 0;
    }
}

// ---------------------------------------------------------------------
// Appels 64 → 32
// ---------------------------------------------------------------------

thread_local! {
    /// Appels 64 → 32 en cours : (repère dans la pile de `call32`, chaîne
    /// en place avant l'appel). Le maillon de chaque appel vit dans la pile
    /// partagée ; quand un gestionnaire d'exception x86 saute hors d'un
    /// appel sans le terminer, ce maillon est abandonné et la pile le
    /// réécrit. Cette copie permet de retrouver le bon (#779).
    static CALLS32: std::cell::RefCell<Vec<(u64, u64)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Appelle du code x86 (stdcall ou cdecl) et rend edx:eax.
pub fn call32(target: usize, args: &[u32]) -> u64 {
    assert!(target < limit(), "wow32 : appel 32 bits vers 0x{target:x}");
    let f: extern "win64" fn(usize, *const u32, usize) -> u64 =
        unsafe { std::mem::transmute(blob().call32) };
    if history_on() {
        let mut a = [0u32; 4];
        for (d, s) in a.iter_mut().zip(args) {
            *d = *s;
        }
        note(Passage {
            desc: 0,
            target: target as u32,
            esp: 0,
            ret: 0,
            args: a,
            result: None,
        });
    }
    let marker = 0u8;
    let here = std::ptr::addr_of!(marker) as u64;
    let depth = CALLS32.with(|c| {
        let mut c = c.borrow_mut();
        c.push((here, seh::chain_now()));
        c.len() - 1
    });
    let r = f(target, args.as_ptr(), args.len());
    // Les appels plus profonds encore notés ont été abandonnés.
    CALLS32.with(|c| c.borrow_mut().truncate(depth));
    std::hint::black_box(&marker);
    r
}

/// Retour d'un appel 64 → 32 alors que la chaîne pointe sous la pile x86
/// (`esp`) : les appels notés sous `esp` sont morts ; la chaîne d'avant le
/// plus ancien d'entre eux est celle de l'appel qui revient.
extern "win64" fn repair_chain(esp: u64) -> u64 {
    let stale = seh::chain_now();
    let mut found = None;
    CALLS32.with(|c| {
        let mut c = c.borrow_mut();
        while let Some(&(at, before)) = c.last() {
            if at >= esp {
                break;
            }
            c.pop();
            found = Some(before);
        }
    });
    crate::trace!(
        "wow32 : retour 32 → 64 après un saut hors d'un appel imbriqué (chaîne 0x{stale:x} → {:x?})",
        found
    );
    found.unwrap_or(stale)
}

/// Assistant x86 « appel avec ebp » (filtres et blocs __finally).
fn call_ebp_helper() -> usize {
    blob().call_ebp
}

/// TEB 32 bits du thread courant (0 hors programme x86).
pub(crate) fn teb32_addr() -> usize {
    TEB32_ADDR.with(Cell::get)
}

/// Cases par thread de la CRT x86, sous 4 Go.
fn scratch() -> usize {
    let t = teb32_addr();
    assert!(t != 0, "wow32 : thread sans TEB 32 bits");
    t + TEB32_SCRATCH
}

/// `errno` (ou `_doserrno`) du thread, lisible par le code x86 ; `None`
/// hors programme x86 ou dans un thread du runtime.
pub fn errno_slot(dos: bool) -> Option<usize> {
    if !active() {
        return None;
    }
    let t = teb32_addr();
    (t != 0).then(|| t + TEB32_SCRATCH + if dos { crt::S_DOSERRNO } else { crt::S_ERRNO })
}

/// Nouveau bloc TLS statique d'une DLL x86 chargée par LoadLibrary, pour
/// le thread dont le TEB 64 bits est `teb64`.
pub fn mirror_tls_slot(teb64: usize, index: usize, block: usize) {
    if !active() || index >= TLS32_ENTRIES {
        return;
    }
    let teb = unsafe { *((teb64 + TEB64_TEB32) as *const usize) };
    if teb != 0 {
        unsafe { *((teb + TEB32_TLS_ARRAY + index * 4) as *mut u32) = block as u32 };
    }
}

/// Fin d'un thread : son entrée de LDT redevient libre, son TEB 32 bits
/// est rendu. À appeler sur le thread lui-même, avant de détacher GS.
pub fn free_thread(teb64: usize) {
    let teb = teb32_addr();
    if !active() || teb == 0 {
        return;
    }
    unsafe {
        // FS : sélecteur nul, base de glibc (ARCH_SET_FS remet le sélecteur
        // à zéro) avant d'effacer l'entrée qu'il désigne.
        let mut base = 0usize;
        libc::syscall(libc::SYS_arch_prctl, 0x1003, &mut base as *mut usize);
        libc::syscall(libc::SYS_arch_prctl, 0x1002, base);
        let selector = *((teb64 + TEB64_FS_SELECTOR) as *const usize) as u32;
        let entry = selector >> 3;
        let empty = UserDesc {
            entry_number: entry,
            base_addr: 0,
            limit: 0,
            // read_exec_only | seg_not_present : entrée vide.
            flags: (1 << 3) | (1 << 5),
        };
        let rc = libc::syscall(
            libc::SYS_modify_ldt,
            1,
            &empty as *const UserDesc,
            std::mem::size_of::<UserDesc>(),
        );
        if rc == 0 && entry != 0 {
            FREE_LDT
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(entry);
        }
        *((teb64 + TEB64_TEB32) as *mut usize) = 0;
        *((teb64 + TEB64_FS_SELECTOR) as *mut usize) = 0;
        libc::munmap(teb as *mut libc::c_void, TEB32_SIZE);
    }
    TEB32_ADDR.with(|c| c.set(0));
}

/// Données de la CRT x86 (argv, environnement…), une fois le processus décrit.
pub fn fill_crt_data() {
    if active() {
        crt::fill_process_data();
    }
}

/// Lecture et écriture d'un mot dans la mémoire du programme x86.
pub(crate) fn rd(addr: u32) -> u32 {
    unsafe { (addr as usize as *const u32).read_unaligned() }
}

pub(crate) fn wr(addr: u32, v: u32) {
    unsafe { (addr as usize as *mut u32).write_unaligned(v) }
}

thread_local! {
    /// TEB 32 bits du thread (0 si le thread n'en a pas).
    static TEB32_ADDR: Cell<usize> = const { Cell::new(0) };
    /// Arguments x86 de l'appel d'API en cours (voir `caller_regs`).
    static CURRENT_ARGS: Cell<u32> = const { Cell::new(0) };
    /// EIP d'une faute à ne pas répartir une seconde fois (rapport).
    static REFIRE: Cell<u32> = const { Cell::new(0) };
}

fn set_refire(eip: u32) {
    REFIRE.with(|r| r.set(eip));
}

/// Registres x86 de l'appelant de l'API en cours, relus dans la trame de
/// `winlux_wow_entry64` (rdi, rsi, rbx, rbp empilés juste sous l'EIP de
/// retour de lcall).
pub(crate) struct CallerRegs {
    pub eip: u32,
    pub esp: u32,
    pub ebp: u32,
    pub ebx: u32,
    pub esi: u32,
    pub edi: u32,
}

pub(crate) fn caller_regs() -> CallerRegs {
    let args = CURRENT_ARGS.with(Cell::get);
    CallerRegs {
        eip: rd(args - 4),
        esp: args,
        ebp: rd(args - 24),
        ebx: rd(args - 32),
        esi: rd(args - 40),
        edi: rd(args - 48),
    }
}

// ---------------------------------------------------------------------
// Appels 32 → 64 : passerelles d'import
// ---------------------------------------------------------------------

/// Descripteur d'une passerelle, lu par `winlux_wow_entry64` (le code de
/// retour doit rester au décalage 0) puis par `dispatch`.
#[repr(C)]
struct Desc {
    ret: u8,
    supported: bool,
    target: usize,
    kinds: Vec<u8>,
    label: String,
    /// Correction du pointeur rendu (copie sous 4 Go).
    fix: crt::Fix,
    /// `Some(aux)` : `target(args_x86, aux)` lit lui-même la pile x86.
    raw: Option<usize>,
    /// Arguments qui sont des fonctions x86 à rappeler (bit n = argument n).
    callbacks: u32,
}

include!(concat!(env!("OUT_DIR"), "/win64_sigs.rs"));

/// Signatures des méthodes d'une table écrite comme une structure
/// (`module` sans le nom du crate, par exemple `dlls::d3d11`), relevées par
/// `build.rs`, une par méthode et `this` compris. Une sous-structure
/// (`ext: ext::DeviceExt`) est remplacée par ses propres champs (#1090).
pub fn vtable_sigs(module: &str, name: &str) -> Option<Vec<&'static str>> {
    fn expand(module: &str, name: &str, depth: u32, out: &mut Vec<&'static str>) -> bool {
        let Some((_, _, sigs)) = VTABLE_SIGS
            .iter()
            .find(|(m, n, _)| *m == module && *n == name)
            .or_else(|| {
                VTABLE_SIGS
                    .iter()
                    .find(|(m, n, _)| m.starts_with(module) && *n == name)
            })
            .or_else(|| VTABLE_SIGS.iter().find(|(_, n, _)| *n == name))
        else {
            return false;
        };
        for s in sigs.iter() {
            match s.strip_prefix('@') {
                Some(inner) if depth < 4 => {
                    if !expand(module, inner, depth + 1, out) {
                        return false;
                    }
                }
                Some(_) => return false,
                None => out.push(s),
            }
        }
        true
    }
    let mut out = Vec::new();
    expand(module, name, 0, &mut out).then_some(out)
}

/// Fonctions exportées par les DLL intégrées : adresse, module Rust de la
/// table d'exports et chemin écrit dans `exports!`.
static NOTES: Mutex<Vec<(usize, &'static str, &'static str)>> = Mutex::new(Vec::new());

/// Appelé par la macro `exports!` pour chaque fonction.
pub fn note_export(addr: usize, module: &'static str, path: &'static str) {
    NOTES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((addr, module, path));
}

struct SigIndex {
    by_path: HashMap<String, &'static str>,
    /// Nom de fonction → signature, s'il n'y en a qu'une pour ce nom.
    by_name: HashMap<&'static str, Option<&'static str>>,
}

fn sig_index() -> &'static SigIndex {
    static I: OnceLock<SigIndex> = OnceLock::new();
    I.get_or_init(|| {
        let mut by_path = HashMap::new();
        let mut by_name: HashMap<&'static str, Option<&'static str>> = HashMap::new();
        for &(module, name, sig) in SIGS {
            let key = if module.is_empty() {
                name.to_string()
            } else {
                format!("{module}::{name}")
            };
            by_path.insert(key, sig);
            by_name
                .entry(name)
                .and_modify(|s| {
                    if *s != Some(sig) {
                        *s = None
                    }
                })
                .or_insert(Some(sig));
        }
        SigIndex { by_path, by_name }
    })
}

fn parent(module: &str) -> &str {
    module.rsplit_once("::").map_or("", |(p, _)| p)
}

fn join(a: &str, b: &str) -> String {
    if a.is_empty() {
        b.to_string()
    } else {
        format!("{a}::{b}")
    }
}

/// Signature d'un chemin Rust écrit dans le module `module`.
fn sig_of_path(module: &str, path: &str) -> Option<&'static str> {
    let idx = sig_index();
    // module_path!() commence par le nom du crate.
    let module = module.split_once("::").map_or("", |(_, rest)| rest);
    let path: String = path.chars().filter(|c| !c.is_whitespace()).collect();
    let path = path.trim_start_matches('&');
    let mut candidates = Vec::new();
    if let Some(rest) = path.strip_prefix("crate::") {
        candidates.push(rest.to_string());
    } else if let Some(rest) = path.strip_prefix("self::") {
        candidates.push(join(module, rest));
    } else if let Some(rest) = path.strip_prefix("super::") {
        candidates.push(join(parent(module), rest));
    } else {
        candidates.push(join(module, path));
        candidates.push(join(parent(module), path));
        candidates.push(join("dlls", path));
    }
    for c in &candidates {
        if let Some(s) = idx.by_path.get(c) {
            return Some(s);
        }
    }
    if path.contains("::") {
        let tail = format!("::{path}");
        let found: Vec<&&str> = idx
            .by_path
            .iter()
            .filter(|(k, _)| k.ends_with(&tail))
            .map(|(_, v)| v)
            .collect();
        if found.len() == 1 {
            return Some(found[0]);
        }
    }
    let name = path.rsplit("::").next().unwrap_or(path);
    idx.by_name.get(name).copied().flatten()
}

/// Signature (« arguments>retour ») de la fonction intégrée à `addr`.
fn sig_of_addr(addr: usize) -> Option<&'static str> {
    let notes = NOTES.lock().unwrap_or_else(|e| e.into_inner());
    notes
        .iter()
        .filter(|(a, _, _)| *a == addr)
        .find_map(|(_, module, path)| sig_of_path(module, path))
}

/// Convention d'appel x86 d'un export : la CRT est en cdecl (l'appelant
/// dépile), le reste de l'API Win32 en stdcall.
fn is_cdecl(dll: &str, func: &str) -> bool {
    let n = crate::dlls::normalize(dll);
    n == "msvcrt"
        || n == "ucrtbase"
        || n.starts_with("msvcr")
        || n.starts_with("msvcp")
        // operator new / delete de MFC 4.2 (signtool.exe, #1394).
        || (n == "mfc42" && (func == "#823" || func == "#825"))
        || (n == "ntdll" && func.starts_with(|c: char| c == '_' || c.is_ascii_lowercase()))
        // LDAPAPI de winldap.h est __cdecl (#1765).
        || n == "wldap32"
        // U_EXPORT2 d'ICU est __cdecl (#1792).
        || n == "icuuc"
        // FCI et FDI de cabinet.dll sont DIAMONDAPI, c'est-à-dire __cdecl
        // (fci.h / fdi.h de mingw-w64), par nom ou par ordinal (#1758).
        || (n == "cabinet"
            && (func.starts_with("FCI")
                || func.starts_with("FDI")
                || matches!(func, "#10" | "#11" | "#12" | "#13" | "#14" | "#20" | "#21" | "#22" | "#23")))
}

/// Octets d'arguments qu'une fonction stdcall x86 dépile (`ret N`), d'après
/// les noms décorés des bibliothèques d'import i686 de MinGW-w64
/// (stdcall.tsv, scripts/gen-x86-stdcall.py). Un bouchon Rust peut avoir
/// moins d'arguments que l'API qu'il sert : c'est la table qui dit ce que
/// le code x86 attend.
fn stdcall_bytes(dll: &str, func: &str) -> Option<u16> {
    static TABLE: OnceLock<HashMap<String, HashMap<&'static str, u16>>> = OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t: HashMap<String, HashMap<&'static str, u16>> = HashMap::new();
        let mut cur = String::new();
        for line in include_str!("stdcall.tsv").lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some(d) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                cur = d.to_string();
                continue;
            }
            if let Some((f, n)) = line.split_once(' ') {
                if let Ok(n) = n.parse() {
                    t.entry(cur.clone()).or_default().insert(f, n);
                }
            }
        }
        t
    });
    let n = crate::dlls::normalize(dll);
    let n = n.strip_suffix(".drv").unwrap_or(&n);
    t.get(n)?.get(func).copied()
}

/// `ret N` d'une passerelle : rien pour cdecl, sinon la table, à défaut la
/// signature Rust.
fn stub_pop(dll: &str, func: &str, slots: usize, label: &str) -> Option<u16> {
    if is_cdecl(dll, func) {
        return None;
    }
    let sig = (slots * 4) as u16;
    match stdcall_bytes(dll, func) {
        Some(n) => {
            if n != sig && slots != 0 {
                crate::trace!(
                    "wow32 : {label} : {sig} octets d'après la signature, {n} d'après la table"
                );
            }
            Some(n)
        }
        None => Some(sig),
    }
}

fn stub_code(desc: usize, pop: Option<u16>) -> Vec<u8> {
    let mut c = Vec::with_capacity(24);
    c.push(0x68); // push desc
    c.extend_from_slice(&(desc as u32).to_le_bytes());
    c.push(0x9a); // lcall 0x33:entry64
    c.extend_from_slice(&blob().entry64.to_le_bytes());
    c.extend_from_slice(&CS64.to_le_bytes());
    c.extend_from_slice(&[0x8e, 0xe1]); // mov fs, cx
    c.extend_from_slice(&[0x83, 0xc4, 0x04]); // add esp, 4
    match pop {
        Some(n) if n > 0 => {
            c.push(0xc2);
            c.extend_from_slice(&n.to_le_bytes());
        }
        _ => c.push(0xc3),
    }
    c
}

fn make_stub(desc: Desc, pop: Option<u16>) -> usize {
    // Descripteur sous 2 Go, comme les passerelles : son adresse est
    // poussée par le code x86 sur 32 bits.
    let at = {
        let mut p = CODE_POOL.lock().unwrap_or_else(|e| e.into_inner());
        let need = (std::mem::size_of::<Desc>() + 15) & !15;
        if p.cur + need > p.end {
            let size = 64 * 1024;
            p.cur = alloc_low(size, true);
            p.end = p.cur + size;
        }
        let at = p.cur;
        p.cur += need;
        at
    };
    unsafe { std::ptr::write(at as *mut Desc, desc) };
    let desc = at;
    alloc_code(&stub_code(desc, pop)) as usize
}

/// Fonctions dont un `*mut usize` n'est pas une case unique (tableau,
/// structure) : l'appelé reçoit l'adresse x86 telle quelle.
const POINTER_ARRAYS: [&str; 11] = [
    "GetKeyboardLayoutList",
    "EnumProcessModules",
    "EnumProcessModulesEx",
    "K32EnumProcessModules",
    "K32EnumProcessModulesEx",
    "ExtractIconExA",
    "ExtractIconExW",
    "RtlCaptureStackBackTrace",
    "CaptureStackBackTrace",
    "__std_exception_copy",
    "__std_exception_destroy",
];

/// Passerelle x86 vers la fonction intégrée `orig` (appelée via `target`,
/// qui peut être son traceur WINLUX_APITRACE).
pub fn import_stub(dll: &str, func: &str, orig: usize, target: usize) -> usize {
    let label = format!("{}!{func}", dll.to_ascii_lowercase());
    let sig = sig_of_addr(orig);
    let (mut args, ret) = match sig.and_then(|s| s.split_once('>')) {
        Some((a, r)) => (a.as_bytes().to_vec(), r.as_bytes()[0]),
        None => (b"?".to_vec(), b'?'),
    };
    // Tableaux de handles en sortie : une seule case de 8 octets prêtée ne
    // suffirait pas, l'appelé écrit directement (disposition 64 bits).
    if POINTER_ARRAYS.contains(&func) {
        for k in args.iter_mut().filter(|k| **k == b'p') {
            *k = b'i';
        }
    }
    let supported = sig.is_some() && !args.contains(&b'?') && ret != b'?';
    // Rappels typés `fn(…)` dans la signature Rust : passerelle 64 → 32.
    let callbacks = args
        .iter()
        .enumerate()
        .filter(|(_, k)| **k == b'c')
        .fold(callback_args(dll, func), |m, (i, _)| m | (1 << i.min(31)));
    if !supported {
        crate::trace!("wow32 : {label} pas encore disponible en 32 bits (signature {sig:?})");
    }
    let slots: usize = args
        .iter()
        .map(|k| if matches!(k, b'q' | b'd') { 2 } else { 1 })
        .sum();
    let pop = stub_pop(dll, func, slots, &label);
    make_stub(
        Desc {
            ret,
            supported,
            target,
            kinds: args,
            label,
            fix: crt::Fix::None,
            raw: None,
            callbacks,
        },
        pop,
    )
}

/// Fonctions dont la version 64 bits ne convient pas aux programmes x86
/// (exceptions par chaîne de cadres…) : signature écrite ici.
fn override_for(dll: &str, func: &str) -> Option<(usize, &'static str)> {
    let n = crate::dlls::normalize(dll);
    let kernel = n == "kernel32" || n == "ntdll";
    let crt = n == "msvcrt" || n == "ucrtbase" || n.starts_with("msvcr");
    if kernel {
        if let Some(f) = fiber::override_for(func) {
            return Some(f);
        }
    }
    Some(match func {
        "RaiseException" if kernel => (seh::raise_exception as *const () as usize, "iiii>v"),
        "RtlUnwind" if kernel => (seh::rtl_unwind as *const () as usize, "iiii>i"),
        "SetUnhandledExceptionFilter" if kernel => (
            seh::set_unhandled_exception_filter as *const () as usize,
            "i>i",
        ),
        "UnhandledExceptionFilter" if kernel => {
            (seh::unhandled_exception_filter as *const () as usize, "i>i")
        }
        "GetProcAddress" if kernel => (get_proc_address32 as *const () as usize, "ii>i"),
        // Structures à pointeurs ou handles remplies par kernel32 : versions
        // à la disposition x86 (étape 7).
        "GetSystemInfo" if kernel => (kernel::get_system_info as *const () as usize, "i>v"),
        "GetNativeSystemInfo" if kernel => {
            (kernel::get_native_system_info as *const () as usize, "i>v")
        }
        "VirtualQuery" if kernel => (kernel::virtual_query as *const () as usize, "iii>i"),
        "VirtualQueryEx" if kernel => (kernel::virtual_query_ex as *const () as usize, "iiii>i"),
        "GetStartupInfoA" | "GetStartupInfoW" if kernel => {
            (kernel::get_startup_info as *const () as usize, "i>v")
        }
        "GlobalMemoryStatus" if kernel => {
            (kernel::global_memory_status as *const () as usize, "i>v")
        }
        "CreateProcessA" if kernel => (
            kernel::create_process_a as *const () as usize,
            "iiiiiiiiii>i",
        ),
        "CreateProcessW" if kernel => (
            kernel::create_process_w as *const () as usize,
            "iiiiiiiiii>i",
        ),
        // Fonctions Interlocked* : exportées par le kernel32 x86 seulement
        // (intrinsèques en 64 bits).
        "InterlockedIncrement" if kernel => {
            (kernel::interlocked_increment as *const () as usize, "i>i")
        }
        "InterlockedDecrement" if kernel => {
            (kernel::interlocked_decrement as *const () as usize, "i>i")
        }
        "InterlockedExchange" if kernel => {
            (kernel::interlocked_exchange as *const () as usize, "ii>i")
        }
        "InterlockedExchangeAdd" if kernel => (
            kernel::interlocked_exchange_add as *const () as usize,
            "ii>i",
        ),
        "InterlockedCompareExchange" if kernel => (
            kernel::interlocked_compare_exchange as *const () as usize,
            "iii>i",
        ),
        "IsWow64Process" if kernel => (kernel::is_wow64_process as *const () as usize, "ii>i"),
        "IsWow64Process2" if kernel => (kernel::is_wow64_process2 as *const () as usize, "iii>i"),
        "_except_handler3" if crt => (seh::except_handler3 as *const () as usize, "iiii>i"),
        "_except_handler4_common" if crt => (
            seh::except_handler4_common as *const () as usize,
            "iiiiii>i",
        ),
        "_CxxThrowException" if crt => (cxx::cxx_throw_exception as *const () as usize, "ii>v"),
        "?terminate@@YAXXZ" if crt => (cxx::terminate as *const () as usize, ">v"),
        "_XcptFilter" if crt => (cxx::xcpt_filter as *const () as usize, "ii>i"),
        "__setusermatherr" if crt => (cxx::set_user_matherr as *const () as usize, "i>v"),
        "_local_unwind2" if crt => (seh::local_unwind2 as *const () as usize, "ii>v"),
        "_global_unwind2" if crt => (seh::global_unwind2 as *const () as usize, "i>v"),
        "_local_unwind4" if crt => (seh::local_unwind4 as *const () as usize, "iii>v"),
        _ => return None,
    })
}

/// Noms que seuls les programmes x86 importent et que wow32 sert par ses
/// passerelles, sans export 64 bits du même nom : `winlux --exports` les
/// ajoute (« x86:dll ») pour que scripts/compat-imports.py ne les compte
/// pas comme manquants dans les binaires 32 bits (#1508).
pub fn x86_only_candidates() -> Vec<(&'static str, String)> {
    let mut v: Vec<(&'static str, String)> = Vec::new();
    for crt in ["msvcrt", "ucrtbase"] {
        for m in [
            "acos", "asin", "atan", "atan2", "cos", "cosh", "exp", "fmod", "log", "log10", "pow",
            "sin", "sinh", "sqrt", "tan", "tanh",
        ] {
            v.push((crt, format!("_CI{m}")));
            v.push((crt, format!("_libm_sse2_{m}_precise")));
        }
        for f in [
            "_except1",
            "_ftol",
            "_ftol2",
            "_ftol2_sse",
            "_adjust_fdiv",
            "_setjmp3",
            "_except_handler3",
            "_except_handler4_common",
            "_local_unwind2",
            "_local_unwind4",
            "_global_unwind2",
            "??1type_info@@UAE@XZ",
            "??0exception@@QAE@XZ",
            "??0exception@@QAE@ABQBD@Z",
            "??0exception@@QAE@ABV0@@Z",
            "??1exception@@UAE@XZ",
            "?what@exception@@UBEPBDXZ",
            "??0bad_cast@@QAE@ABV0@@Z",
            "??1bad_cast@@UAE@XZ",
            "__p___initenv",
            "__p___winitenv",
            "__CxxFrameHandler",
            "__CxxFrameHandler2",
        ] {
            v.push((crt, f.to_string()));
        }
    }
    for f in [
        "InterlockedIncrement",
        "InterlockedDecrement",
        "InterlockedExchange",
        "InterlockedExchangeAdd",
        "InterlockedCompareExchange",
    ] {
        v.push(("kernel32", f.to_string()));
    }
    v.retain(|(dll, f)| override_stub(dll, f).is_some());
    v
}

/// Passerelle x86 vers la version 32 bits propre d'une fonction, s'il y en
/// a une ; pour une donnée exportée, son adresse sous 4 Go.
pub fn override_stub(dll: &str, func: &str) -> Option<usize> {
    let n = crate::dlls::normalize(dll);
    if let Some(code) = fiber::code_for(n == "kernel32" || n == "ntdll", func) {
        return Some(code);
    }
    let label = format!("{}!{func}", dll.to_ascii_lowercase());
    let (target, sig, fix) = match override_for(dll, func) {
        Some((target, sig)) => (target, sig, crt::Fix::None),
        None => match {
            let n = crate::dlls::normalize(dll);
            user::override_for(&n, func)
                .or_else(|| crt::override_for(&n, func))
                .or_else(|| com::override_for(&n, func))
                .or_else(|| comdlg::override_for(&n, func))
                .or_else(|| comctl::override_for(&n, func))
                .or_else(|| vulkan::override_for(&n, func))
        }? {
            crt::Override::Data(addr) => return Some(addr),
            crt::Override::Native { target, sig, fix } => (target, sig, fix),
            crt::Override::Raw { target, aux, pop } => {
                return Some(make_stub(
                    Desc {
                        ret: b'i',
                        supported: true,
                        target,
                        kinds: Vec::new(),
                        label,
                        fix: crt::Fix::None,
                        raw: Some(aux),
                        callbacks: 0,
                    },
                    pop,
                ))
            }
        },
    };
    let (args, ret) = sig.split_once('>')?;
    let slots: usize = args
        .bytes()
        .map(|k| if matches!(k, b'q' | b'd') { 2 } else { 1 })
        .sum();
    let pop = (!is_cdecl(dll, func)).then_some((slots * 4) as u16);
    Some(make_stub(
        Desc {
            ret: ret.as_bytes()[0],
            supported: true,
            target,
            kinds: args.as_bytes().to_vec(),
            label,
            fix,
            raw: None,
            callbacks: 0,
        },
        pop,
    ))
}

/// Passerelle x86 de `kernel32!func`, comme à l'import (#2367).
pub(super) fn kernel32_stub(func: &str) -> Option<usize> {
    override_stub("kernel32.dll", func).or_else(|| {
        let a = crate::dlls::lookup("kernel32", func)?;
        Some(import_stub("kernel32.dll", func, a, a))
    })
}

/// `GetProcAddress` d'un programme x86 : pour une DLL intégrée, la
/// passerelle x86 de la fonction (une par fonction), jamais son adresse 64 bits.
extern "win64" fn get_proc_address32(module: u32, name: u32) -> u32 {
    let found = unsafe {
        crate::dlls::kernel32::get_proc_address(module as usize, name as usize as *const u8)
    };
    if found == 0 {
        // Fonction propre au CRT x86 (_CIfmod, _libm_sse2_*…) : pas
        // d'export 64 bits, mais un remplacement x86, comme à l'import (#1333).
        if let Some(dll) = crate::dlls::builtin_by_handle(module as usize) {
            if name > 0xffff {
                let symbol = unsafe { crate::dlls::cstr(name as usize as *const u8) };
                if let Some(stub) = override_stub(&format!("{dll}.dll"), &symbol) {
                    return stub as u32;
                }
            }
        }
        return 0;
    }
    let Some(dll) = crate::dlls::builtin_by_handle(module as usize) else {
        // Image x86 chargée : ses exports sont déjà du code x86.
        return found as u32;
    };
    let symbol = if name <= 0xffff {
        format!("#{name}")
    } else {
        unsafe { crate::dlls::cstr(name as usize as *const u8) }
    };
    static STUBS: Mutex<Option<HashMap<(String, String), usize>>> = Mutex::new(None);
    let key = (dll.to_string(), symbol.clone());
    if let Some(&s) = STUBS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(&key)
    {
        return s as u32;
    }
    let file = format!("{dll}.dll");
    let known = dll != "kernel32"
        || crate::dlls::kernel32_proc(&symbol, module as usize == crate::dlls::kernelbase_handle())
            .is_some();
    let stub = if !known || (dll != "kernel32" && crate::dlls::lookup(dll, &symbol).is_none()) {
        // Bouchon de WINLUX_GETPROC_STUBS : fonction absente.
        missing_stub(dll, &symbol, found)
    } else {
        override_stub(&file, &symbol).unwrap_or_else(|| {
            let orig = crate::dlls::lookup(dll, &symbol).unwrap_or(found);
            import_stub(&file, &symbol, orig, found)
        })
    };
    STUBS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, stub);
    stub as u32
}

/// Passerelle x86 cdecl vers une fonction win64 de signature `sig`.
pub(super) fn native_stub(target: usize, sig: &str, label: &str) -> usize {
    let (args, ret) = sig.split_once('>').unwrap_or(("", "i"));
    make_stub(
        Desc {
            ret: ret.as_bytes()[0],
            supported: true,
            target,
            kinds: args.as_bytes().to_vec(),
            label: label.to_string(),
            fix: crt::Fix::None,
            raw: None,
            callbacks: 0,
        },
        None,
    )
}

/// Passerelle x86 vers le bouchon 64 bits d'une fonction absente.
pub fn missing_stub(dll: &str, func: &str, stub64: usize) -> usize {
    let label = format!("{}!{func}", dll.to_ascii_lowercase());
    // Même absente, une fonction stdcall doit dépiler ses arguments si le
    // programme continue après l'échec qu'elle rend.
    let pop = stub_pop(dll, func, 0, &label);
    make_stub(
        Desc {
            ret: b'i',
            supported: true,
            target: stub64,
            kinds: Vec::new(),
            label,
            fix: crt::Fix::None,
            raw: None,
            callbacks: 0,
        },
        pop,
    )
}

/// Argument x86 de 32 bits vers sa case 64 bits. Pseudo-handles (-1,
/// -2…) et constantes négatives gardent leur signe ; adresses et entiers
/// positifs sont complétés par des zéros. Une fonction qui attend un u32
/// n'en lit de toute façon que les 32 bits bas.
fn widen(v: u32) -> u64 {
    if v >= 0xffff_0000 {
        v as i32 as i64 as u64
    } else {
        v as u64
    }
}

// ---------------------------------------------------------------------
// Historique des passages (WINLUX_WOW32_HISTORY)
// ---------------------------------------------------------------------

/// Passage noté : appel x86 d'une passerelle (`desc`) ou rappel 64 → 32
/// d'une fonction x86 (`desc` = 0, `target`).
#[derive(Clone, Copy)]
struct Passage {
    desc: u32,
    target: u32,
    /// esp x86 à l'appel (adresse de retour), et cette adresse de retour.
    esp: u32,
    ret: u32,
    args: [u32; 4],
    /// edx:eax rendu par la passerelle, une fois revenue (#1440) : la
    /// valeur nulle qui fait fauter le code x86 vient souvent de là.
    result: Option<u64>,
}

const HISTORY_LEN: usize = 64;

type Ring = std::sync::Arc<Mutex<std::collections::VecDeque<Passage>>>;

/// Historiques de tous les fils x86, pour les écrire à la fin du processus
/// quel que soit le fil qui la décide (#2368). (tid, historique)
static RINGS: Mutex<Vec<(i64, Ring)>> = Mutex::new(Vec::new());

thread_local! {
    static HISTORY: Ring = {
        let ring: Ring = std::sync::Arc::new(Mutex::new(
            std::collections::VecDeque::with_capacity(HISTORY_LEN),
        ));
        let tid = unsafe { libc::syscall(libc::SYS_gettid) };
        RINGS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((tid, std::sync::Arc::clone(&ring)));
        ring
    };
}

fn ring_lock(r: &Ring) -> std::sync::MutexGuard<'_, std::collections::VecDeque<Passage>> {
    r.lock().unwrap_or_else(|e| e.into_inner())
}

/// `WINLUX_WOW32_HISTORY=1` : chaque fil garde ses 64 derniers passages
/// entre le code x86 et le runtime (passerelles, y compris SendMessage et
/// CallWindowProc que WINLUX_APITRACE ne voit pas, et rappels de fonctions
/// x86) ; une faute x86 les écrit dans la trace. Sert à trouver l'appel
/// qui a laissé la pile x86 décalée (#1129).
fn history_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("WINLUX_WOW32_HISTORY").is_some_and(|v| !v.is_empty() && v != "0")
    })
}

/// `WINLUX_WOW32_HISTORY=flux` (ou `flux:motif`) : chaque passage est en
/// plus écrit tout de suite sur la sortie d'erreur, puis une seconde fois
/// avec la valeur rendue (« = 0x… ») au retour, filtré sur le motif
/// (« SetWindowPos », « user32 »…). Sert à suivre ce qu'un programme x86
/// demande sans attendre une faute (champ du port de WinSCP, #1251).
/// Plusieurs motifs se séparent par des virgules : `flux:filemapping,mapview`.
fn history_stream() -> Option<&'static str> {
    static FILTER: OnceLock<Option<String>> = OnceLock::new();
    FILTER
        .get_or_init(|| {
            let v = std::env::var("WINLUX_WOW32_HISTORY").ok()?;
            let rest = v.strip_prefix("flux")?;
            Some(rest.strip_prefix(':').unwrap_or("").to_ascii_lowercase())
        })
        .as_deref()
}

/// La ligne passe-t-elle le filtre du mode flux (motif vide : tout passe ;
/// sinon l'un des motifs séparés par des virgules) ?
fn stream_match(filter: &str, line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    filter.is_empty()
        || filter
            .split(',')
            .map(str::trim)
            .any(|m| !m.is_empty() && line.contains(m))
}

fn passage_line(p: &Passage) -> String {
    let what = if p.desc != 0 {
        let d = unsafe { &*(p.desc as usize as *const Desc) };
        d.label.clone()
    } else {
        format!("rappel x86 0x{:x}", p.target)
    };
    let rendu = match p.result {
        Some(r) => format!(" = 0x{:x}", r as u32),
        None => String::new(),
    };
    format!(
        "{what} (esp 0x{:x}, retour 0x{:x}) {:x?}{rendu}",
        p.esp, p.ret, p.args
    )
}

/// Note la valeur rendue par la passerelle `desc` appelée à `esp` : le
/// passage le plus récent qui lui correspond (des rappels x86 ont pu être
/// notés entre-temps).
fn note_result(desc: u32, esp: u32, r: u64) {
    let _ = HISTORY.try_with(|h| {
        let mut h = ring_lock(h);
        if let Some(p) = h
            .iter_mut()
            .rev()
            .find(|p| p.desc == desc && p.esp == esp && p.result.is_none())
        {
            p.result = Some(r);
            // Mode flux : la valeur rendue suit aussitôt (#1616), sinon
            // seule une faute la montrait.
            if let Some(filter) = history_stream() {
                let line = passage_line(p);
                if stream_match(filter, &line) {
                    eprintln!("winlux[wow32] {line}");
                }
            }
        }
    });
}

fn note(p: Passage) {
    if let Some(filter) = history_stream() {
        let line = passage_line(&p);
        if stream_match(filter, &line) {
            eprintln!("winlux[wow32] {line}");
        }
    }
    let _ = HISTORY.try_with(|h| {
        let mut h = ring_lock(h);
        if h.len() == HISTORY_LEN {
            h.pop_front();
        }
        h.push_back(p);
    });
}

/// Passages notés, du plus ancien au plus récent, en texte.
fn history_lines() -> Vec<String> {
    HISTORY
        .try_with(|h| ring_lock(h).iter().map(passage_line).collect())
        .unwrap_or_default()
}

/// Écrit l'historique au niveau Warn : sortie d'erreur et journal, sans
/// exiger WINLUX_LOG=trace (#2368).
fn history_emit(line: &std::fmt::Arguments<'_>) {
    crate::log::emit(crate::log::Level::Warn, "wow32", line);
}

/// Fin du processus (ExitProcess, TerminateProcess sur soi, fail-fast,
/// exception non gérée) : avec WINLUX_WOW32_HISTORY, l'historique de chaque
/// fil x86 est écrit, une seule fois (#2368).
pub fn dump_history_at_exit(why: &str) {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !history_on() || DONE.swap(true, Ordering::Relaxed) {
        return;
    }
    let rings: Vec<(i64, Ring)> = RINGS.try_lock().map(|r| r.clone()).unwrap_or_default();
    let me = unsafe { libc::syscall(libc::SYS_gettid) };
    history_emit(&format_args!(
        "{why} : historique des passages x86 ↔ runtime de {} fil(s)",
        rings.len()
    ));
    for (tid, ring) in rings {
        // try_lock : un fil arrêté en plein passage garde son verrou.
        let Ok(h) = ring.try_lock() else {
            history_emit(&format_args!("fil {tid} : historique occupé"));
            continue;
        };
        let lines: Vec<String> = h.iter().map(passage_line).collect();
        drop(h);
        let mark = if tid == me { " (fil qui termine)" } else { "" };
        history_emit(&format_args!("fil {tid}{mark} : {} passages", lines.len()));
        for (i, l) in lines.iter().enumerate() {
            history_emit(&format_args!("fil {tid}   {i:2} {l}"));
        }
    }
}

#[cfg(test)]
#[test]
fn pas_a_pas_cible() {
    assert_eq!(
        parse_step("SendMessageW:133c"),
        Some(("SendMessageW".to_string(), Some(0x133c)))
    );
    assert_eq!(
        parse_step("SendMessageW:0x30"),
        Some(("SendMessageW".into(), Some(0x30)))
    );
    assert_eq!(parse_step("GetObjectW"), Some(("GetObjectW".into(), None)));
    assert_eq!(parse_step(":12"), None);
    assert_eq!(parse_step("F:zz"), None);
}

/// Variable qui fait de `historique_enfant` le processus enfant (#2368) :
/// `sortie` finit par ExitProcess, `failfast` par RaiseFailFastException.
#[cfg(test)]
const HISTORIQUE_ENFANT: &str = "WINLUX_TEST_HISTORIQUE_ENFANT";

#[cfg(test)]
#[test]
fn historique_enfant() {
    let Some(mode) = std::env::var_os(HISTORIQUE_ENFANT) else {
        return;
    };
    let passage = |t: u32| Passage {
        desc: 0,
        target: t,
        esp: 0,
        ret: 0,
        args: [0; 4],
        result: None,
    };
    note(passage(0x40_1111));
    // Un second fil : son historique doit aussi sortir, écrit par le fil
    // qui termine.
    std::thread::spawn(move || note(passage(0x40_2222)))
        .join()
        .unwrap();
    if mode == "failfast" {
        crate::dlls::seh::raise_fail_fast_exception(std::ptr::null(), std::ptr::null(), 0);
    }
    crate::dlls::kernel32::exit_process(3);
}

#[cfg(test)]
fn historique_lancer(mode: &str, history: bool) -> (Option<i32>, String) {
    let dir = std::env::temp_dir().join(format!("winlux-hist-{mode}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "wow32::historique_enfant", "--test-threads=1"])
        .env(HISTORIQUE_ENFANT, mode)
        .env("WINLUX_JOURNAL_DIR", &dir)
        .env_remove("WINLUX_LOG")
        .env_remove("WINLUX_TRACE")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    if history {
        cmd.env("WINLUX_WOW32_HISTORY", "1");
    } else {
        cmd.env_remove("WINLUX_WOW32_HISTORY");
    }
    let mut child = cmd.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let status = loop {
        if let Some(st) = child.try_wait().unwrap() {
            break st;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("processus enfant bloqué plus de 60 s");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let mut err = String::new();
    use std::io::Read;
    let _ = child.stderr.take().unwrap().read_to_string(&mut err);
    let _ = std::fs::remove_dir_all(&dir);
    (status.code(), err)
}

/// #2368 : avec WINLUX_WOW32_HISTORY seule, l'historique de chaque fil sort
/// sur ExitProcess et sur un fail-fast ; sans elle, rien.
#[cfg(test)]
#[test]
fn historique_ecrit_a_la_fin_du_processus() {
    for mode in ["sortie", "failfast"] {
        let (code, err) = historique_lancer(mode, true);
        let want = if mode == "sortie" { 3 } else { 0x09 };
        assert_eq!(code, Some(want), "{mode} : {err}");
        assert!(
            err.contains("historique des passages x86"),
            "{mode} : {err}"
        );
        assert!(err.contains("rappel x86 0x401111"), "{mode} : {err}");
        assert!(err.contains("rappel x86 0x402222"), "{mode} : {err}");
        assert!(err.contains("(fil qui termine)"), "{mode} : {err}");
    }
    let (_, err) = historique_lancer("sortie", false);
    assert!(!err.contains("historique des passages"), "{err}");
}

#[cfg(test)]
#[test]
fn historique_borne() {
    for i in 0..(HISTORY_LEN as u32 + 5) {
        note(Passage {
            desc: 0,
            target: 0x40_0000 + i,
            esp: 0,
            ret: 0,
            args: [i, 0, 0, 0],
            result: None,
        });
    }
    let lines = history_lines();
    assert_eq!(lines.len(), HISTORY_LEN);
    note(Passage {
        desc: 0,
        target: 0x41_0000,
        esp: 0x1000,
        ret: 0,
        args: [0; 4],
        result: None,
    });
    note_result(0, 0x1000, 0x1234);
    assert!(history_lines().last().unwrap().ends_with("= 0x1234"));
    assert!(lines[0].starts_with("rappel x86 0x400005"), "{}", lines[0]);
    assert!(lines[HISTORY_LEN - 1].contains("0x400044"));
}

// ---------------------------------------------------------------------
// Pas à pas (WINLUX_WOW32_STEP)
// ---------------------------------------------------------------------

/// Instructions x86 encore à suivre pas à pas.
static STEP_LEFT: AtomicUsize = AtomicUsize::new(0);
const STEP_RING: usize = 1024;
const STEP_MAX: usize = 200_000;
/// Derniers pas : (eip << 32) | esp.
static STEPS: [AtomicU64; STEP_RING] = [const { AtomicU64::new(0) }; STEP_RING];
static STEP_AT: AtomicUsize = AtomicUsize::new(0);

/// `WINLUX_WOW32_STEP=Fonction[:message]` (avec `WINLUX_TRACE=1`) : au
/// retour de chaque appel x86 de la passerelle dont le libellé contient
/// `Fonction` (et dont le deuxième argument vaut `message`, en hexadécimal),
/// le code x86 est suivi pas à pas sur 200 000 instructions (indicateur TF
/// du processeur) ; une faute x86 écrit les 1 024 derniers EIP et ESP.
/// Sert quand la pile est abîmée sans appel au runtime entre le dernier
/// passage et la faute (#1129).
fn step_target() -> Option<&'static (String, Option<u32>)> {
    static T: OnceLock<Option<(String, Option<u32>)>> = OnceLock::new();
    T.get_or_init(|| {
        let v = std::env::var("WINLUX_WOW32_STEP").ok()?;
        parse_step(&v)
    })
    .as_ref()
}

fn parse_step(v: &str) -> Option<(String, Option<u32>)> {
    let (f, m) = match v.split_once(':') {
        Some((f, m)) => {
            let m = m.trim().trim_start_matches("0x");
            (f, Some(u32::from_str_radix(m, 16).ok()?))
        }
        None => (v, None),
    };
    let f = f.trim();
    (!f.is_empty()).then(|| (f.to_string(), m))
}

/// L'appel qui commence doit-il armer le pas à pas à son retour ?
fn step_wanted(d: &Desc, args: u32) -> bool {
    let Some((f, m)) = step_target() else {
        return false;
    };
    d.label.contains(f.as_str()) && m.is_none_or(|m| rd(args + 4) == m)
}

fn step_arm() {
    STEP_LEFT.store(STEP_MAX, Ordering::Relaxed);
    unsafe { *(blob().step_arm as *mut u8) = 1 };
}

/// SIGTRAP du pas à pas. En x86 : le pas est noté et TF reste posé. En
/// 64 bits (passerelle appelée pendant le pas à pas) : TF est retiré et
/// sera reposé au retour de la passerelle. Rend faux si le signal n'est
/// pas un pas.
unsafe fn step_trap(info: *mut libc::siginfo_t, uc: *mut libc::ucontext_t) -> bool {
    const TRAP_TRACE: i32 = 2;
    if STEP_LEFT.load(Ordering::Relaxed) == 0 || info.is_null() || uc.is_null() {
        return false;
    }
    if (*info).si_code != TRAP_TRACE {
        return false;
    }
    let g = &mut (*uc).uc_mcontext.gregs;
    let cs = g[libc::REG_CSGSFS as usize] as u64 & 0xffff;
    if cs == CS64 as u64 {
        g[libc::REG_EFL as usize] &= !0x100;
        *(blob().step_arm as *mut u8) = 1;
        return true;
    }
    let eip = g[libc::REG_RIP as usize] as u64 & 0xffff_ffff;
    let esp = g[libc::REG_RSP as usize] as u64 & 0xffff_ffff;
    let i = STEP_AT.fetch_add(1, Ordering::Relaxed) % STEP_RING;
    STEPS[i].store(eip << 32 | esp, Ordering::Relaxed);
    if STEP_LEFT.fetch_sub(1, Ordering::Relaxed) > 1 {
        g[libc::REG_EFL as usize] |= 0x100;
    } else {
        g[libc::REG_EFL as usize] &= !0x100;
    }
    true
}

/// Derniers pas notés, du plus ancien au plus récent.
fn step_lines() -> Vec<String> {
    let n = STEP_AT.load(Ordering::Relaxed);
    let first = n.saturating_sub(STEP_RING);
    let all: Vec<u64> = (first..n)
        .map(|i| STEPS[i % STEP_RING].load(Ordering::Relaxed))
        .collect();
    all.chunks(8)
        .map(|c| {
            c.iter()
                .map(|v| format!("{:x}/{:x}", v >> 32, v & 0xffff_ffff))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Écrit l'historique du fil dans la trace (faute x86).
pub(super) fn trace_history() {
    if step_target().is_some() {
        // Plus de pas à pas pendant la répartition de la faute.
        STEP_LEFT.store(0, Ordering::Relaxed);
        let lines = step_lines();
        crate::trace!(
            "wow32 : {} derniers pas x86 (eip/esp) :",
            STEP_AT.load(Ordering::Relaxed).min(STEP_RING)
        );
        for l in lines {
            crate::trace!("wow32 :   {l}");
        }
    }
    if !history_on() {
        return;
    }
    // Niveau Warn : la variable suffit, sans WINLUX_LOG=trace (#2368).
    let lines = history_lines();
    history_emit(&format_args!(
        "faute x86 : {} derniers passages x86 ↔ runtime du fil",
        lines.len()
    ));
    for (i, l) in lines.iter().enumerate() {
        history_emit(&format_args!("  {i:2} {l}"));
    }
}

/// Relit les arguments x86 et appelle la fonction win64.
extern "win64" fn dispatch(desc: u32, args: u32, xmm0: *mut u64) -> u64 {
    let d = unsafe { &*(desc as usize as *const Desc) };
    let step = step_target().is_some() && step_wanted(d, args);
    let r = dispatch_inner(d, desc, args, xmm0);
    if history_on() {
        note_result(desc, args - 4, r);
    }
    if step {
        step_arm();
    }
    r
}

fn dispatch_inner(d: &Desc, desc: u32, args: u32, xmm0: *mut u64) -> u64 {
    if history_on() {
        let rd = |a: u32| unsafe { (a as usize as *const u32).read_unaligned() };
        note(Passage {
            desc,
            target: 0,
            esp: args - 4,
            ret: rd(args - 4),
            args: [rd(args), rd(args + 4), rd(args + 8), rd(args + 12)],
            result: None,
        });
    }
    if !d.supported {
        eprintln!(
            "\nwinlux: le programme 32 bits a appelé {}, pas encore disponible pour les programmes 32 bits (#232).",
            d.label
        );
        crate::thunks::log_missing(&format!("{} (32 bits)", d.label), true);
        crate::crash::record_failure(
            "ENTRYPOINT_NOT_FOUND",
            &format!("fonction={} (32 bits)", d.label),
        );
        crate::dlls::kernel32::flush_and_exit(0xC000_0139u32 as i32)
    }
    CURRENT_ARGS.with(|c| c.set(args));
    if let Some(aux) = d.raw {
        let f: extern "win64" fn(u32, usize) -> u64 = unsafe { std::mem::transmute(d.target) };
        return f(args, aux);
    }
    let mut slots: Vec<u64> = Vec::with_capacity(d.kinds.len().max(4));
    // Cases de 8 octets prêtées à l'appelé pour les arguments `p` : (case
    // x86 de 4 octets, valeur 64 bits). Taille fixée d'avance : les adresses
    // passées à l'appelé ne bougent pas.
    let mut wide: Vec<(usize, [u64; 2])> = Vec::with_capacity(d.kinds.len());
    let mut p = args as usize;
    let rd = |p: usize| unsafe { (p as *const u32).read_unaligned() };
    for k in &d.kinds {
        match k {
            b'p' => {
                let at = rd(p) as usize;
                p += 4;
                if at == 0 {
                    slots.push(0);
                    continue;
                }
                // Valeur d'entrée (tailles en entrée-sortie, handle à
                // remplacer…) : étendue comme un argument ordinaire.
                let v = unsafe { (at as *const u32).read_unaligned() };
                // 16 octets par case : un appelé qui écrirait un peu plus
                // loin qu'un pointeur ne déborde pas sur le tas.
                wide.push((at, [widen(v), 0]));
                let cell = wide.last_mut().map(|w| w.1.as_mut_ptr());
                slots.push(cell.map_or(0, |c| c as u64));
            }
            b'q' | b'd' => {
                slots.push(rd(p) as u64 | (rd(p + 4) as u64) << 32);
                p += 8;
            }
            b'f' => {
                slots.push(rd(p) as u64);
                p += 4;
            }
            _ => {
                slots.push(widen(rd(p)));
                p += 4;
            }
        }
    }
    if d.callbacks != 0 {
        // Une case par argument, quel que soit son type : rang = argument.
        for (i, s) in slots.iter_mut().enumerate().take(32) {
            if d.callbacks & (1 << i) != 0 && *s != 0 {
                *s = callback64(*s as u32) as u64;
            }
        }
    }
    let n = slots.len();
    while slots.len() < 4 {
        slots.push(0);
    }
    let r = unsafe { winlux_wow_call64(d.target, slots.as_ptr(), n, xmm0) };
    for &(at, [v, _]) in &wide {
        if (v >> 32) != 0 && (v >> 32) != 0xffff_ffff {
            crate::trace!(
                "wow32 : {} a écrit 0x{v:x} dans une case de 4 octets (tronqué)",
                d.label
            );
        }
        unsafe { (at as *mut u32).write_unaligned(v as u32) };
    }
    if d.ret == b'i' {
        let r = crt::apply_fix(d.fix, r);
        if (r >> 32) != 0 && (r >> 32) != 0xffff_ffff && r as usize >= limit() {
            crate::trace!(
                "wow32 : {} a rendu 0x{r:x}, au-dessus de 4 Go (pointeur tronqué ?)",
                d.label
            );
        }
        return r;
    }
    r
}

// ---------------------------------------------------------------------
// Rappels : fonctions x86 appelées par le code 64 bits
// ---------------------------------------------------------------------

/// Arguments « fonction de rappel » des API qui en prennent (bit n =
/// argument n). Le code 64 bits les appelle en convention win64 : chacune
/// reçoit une petite passerelle 64 → 32 (`callback64`).
fn callback_args(dll: &str, func: &str) -> u32 {
    let n = crate::dlls::normalize(dll);
    let bit = |i: u32| 1u32 << i;
    let crt = n == "msvcrt" || n == "ucrtbase" || n.starts_with("msvcr");
    match func {
        "_beginthreadex" if crt => bit(2),
        "_beginthread" if crt => bit(0),
        "CreateThread" | "CreateTimerQueueTimer" | "RegisterWaitForSingleObject" => bit(2),
        "CreateRemoteThread" | "CreateRemoteThreadEx" => bit(3),
        "QueueUserWorkItem"
        | "FlsAlloc"
        | "SetConsoleCtrlHandler"
        | "QueueUserAPC"
        | "TrySubmitThreadpoolCallback"
        | "CreateThreadpoolWork"
        | "CreateThreadpoolTimer"
        | "CreateThreadpoolWait"
        | "EnumSystemLocalesA"
        | "EnumSystemLocalesW"
        | "EnumSystemCodePagesA"
        | "EnumSystemCodePagesW"
        | "EnumUILanguagesA"
        | "EnumUILanguagesW"
        | "EnumCalendarInfoW"
        | "EnumCalendarInfoExW"
        | "EnumWindows" => bit(0),
        "InitOnceExecuteOnce"
        | "RegisterServiceCtrlHandlerA"
        | "RegisterServiceCtrlHandlerW"
        | "RegisterServiceCtrlHandlerExA"
        | "RegisterServiceCtrlHandlerExW"
        | "SetTimerQueueTimer"
        | "CreateFiber"
        | "CreateThreadpoolIo"
        | "EnumResourceTypesA"
        | "EnumResourceTypesW"
        | "EnumChildWindows"
        | "EnumThreadWindows"
        | "EnumDesktopWindows" => bit(1),
        "EnumResourceNamesA"
        | "EnumResourceNamesW"
        | "CreateFiberEx"
        | "EnumDisplayMonitors"
        | "EnumFontFamiliesA"
        | "EnumFontFamiliesW"
        | "EnumFontFamiliesExA"
        | "EnumFontFamiliesExW"
        | "EnumFontsA"
        | "EnumFontsW"
        | "GrayStringA"
        | "GrayStringW" => bit(2),
        "EnumResourceLanguagesA" | "EnumResourceLanguagesW" | "SetWaitableTimer" | "SetTimer" => {
            bit(3)
        }
        "ReadFileEx" | "WriteFileEx" => bit(4),
        _ => 0,
    }
}

// Entrée commune des passerelles 64 → 32 : r10d = fonction x86. Recopie
// rcx, rdx, r8, r9 et quatre cases de pile, puis appelle `callback_dispatch`.
std::arch::global_asm!(
    r#"
    .text
    .globl winlux_wow_cb64
winlux_wow_cb64:
    push rbp
    mov rbp, rsp
    sub rsp, 96
    mov qword ptr [rsp + 32], rcx
    mov qword ptr [rsp + 40], rdx
    mov qword ptr [rsp + 48], r8
    mov qword ptr [rsp + 56], r9
    mov rax, qword ptr [rbp + 48]
    mov qword ptr [rsp + 64], rax
    mov rax, qword ptr [rbp + 56]
    mov qword ptr [rsp + 72], rax
    mov rax, qword ptr [rbp + 64]
    mov qword ptr [rsp + 80], rax
    mov rax, qword ptr [rbp + 72]
    mov qword ptr [rsp + 88], rax
    mov ecx, r10d
    lea rdx, [rsp + 32]
    call {dispatch}
    leave
    ret
"#,
    dispatch = sym callback_dispatch,
);

extern "C" {
    fn winlux_wow_cb64();
}

/// Passerelles 64 → 32 déjà créées : fonction x86 → passerelle, et l'inverse.
static CALLBACKS: Mutex<Option<(HashMap<u32, usize>, HashMap<usize, u32>)>> = Mutex::new(None);

/// Adresse appelable en win64 pour la fonction x86 `target` (une
/// passerelle par fonction). Une passerelle déjà donnée reste telle quelle.
pub fn callback64(target: u32) -> usize {
    let mut g = CALLBACKS.lock().unwrap_or_else(|e| e.into_inner());
    let (fwd, back) = g.get_or_insert_with(|| (HashMap::new(), HashMap::new()));
    if back.contains_key(&(target as usize)) {
        return target as usize;
    }
    if let Some(&t) = fwd.get(&target) {
        return t;
    }
    let mut code = Vec::with_capacity(20);
    code.extend_from_slice(&[0x41, 0xba]); // mov r10d, imm32
    code.extend_from_slice(&target.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xb8]); // mov rax, imm64
    code.extend_from_slice(&(winlux_wow_cb64 as *const () as usize as u64).to_le_bytes());
    code.extend_from_slice(&[0xff, 0xe0]); // jmp rax
    let at = alloc_code(&code) as usize;
    fwd.insert(target, at);
    back.insert(at, target);
    at
}

extern "win64" fn callback_dispatch(target: u32, args: *const u64) -> u64 {
    // Thread du runtime (minuterie, réserve de threads) sans TEB : il en
    // reçoit un, avec son TEB 32 bits.
    if teb32_addr() == 0 {
        crate::process::init_thread();
    }
    let a: Vec<u32> = (0..8).map(|i| unsafe { *args.add(i) } as u32).collect();
    let r = call32(target as usize, &a) as u32;
    if r >= 0xffff_0000 {
        r as i32 as i64 as u64
    } else {
        r as u64
    }
}

// ---------------------------------------------------------------------
// Signaux
// ---------------------------------------------------------------------

/// Décalage de uc_mcontext.gregs[REG_CSGSFS] dans ucontext_t.
const UC_CSGSFS: usize = std::mem::offset_of!(libc::ucontext_t, uc_mcontext)
    + std::mem::offset_of!(libc::mcontext_t, gregs)
    + libc::REG_CSGSFS as usize * 8;

static PREVIOUS: [AtomicUsize; 32] = [const { AtomicUsize::new(0) }; 32];

// Entrée des gestionnaires de fautes : si la faute vient du code x86, FS
// pointe sur le TEB 32 bits et le TLS de glibc est inaccessible ; on le
// rétablit avant tout code Rust.
std::arch::global_asm!(
    r#"
    .text
    .globl winlux_wow_signal
winlux_wow_signal:
    movzx eax, word ptr [rdx + {csgsfs}]
    cmp eax, {cs32}
    jne 1f
    // Pas à pas (WINLUX_WOW32_STEP) : traité sans toucher à FS, que le
    // retour du signal ne rétablirait pas pour le code x86.
    cmp edi, 5
    jne 2f
    push rdi
    push rsi
    push rdx
    call {step_signal}
    pop rdx
    pop rsi
    pop rdi
    test al, al
    jz 2f
    ret
2:
    push rdi
    push rsi
    push rdx
    mov rax, qword ptr gs:[{fs_glibc}]
    mov rsi, rax
    mov edi, 0x1002
    mov eax, 158
    syscall
    pop rdx
    pop rsi
    pop rdi
    jmp {fault32}
1:
    jmp {chain}
"#,
    csgsfs = const UC_CSGSFS,
    cs32 = const CS32,
    fs_glibc = const TEB64_FS_GLIBC,
    fault32 = sym fault32,
    chain = sym chain,
    step_signal = sym step_signal,
);

/// SIGTRAP reçu en x86, FS encore celui du programme : sans TLS.
extern "C" fn step_signal(_sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) -> bool {
    unsafe { step_trap(info, ctx as *mut libc::ucontext_t) }
}

extern "C" {
    fn winlux_wow_signal();
}

/// Faute dans le code x86 : rapport de plantage. Le SEH 32 bits (chaîne
/// fs:[0]) viendra avec la deuxième étape de #232.
extern "C" fn fault32(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    unsafe {
        if !info.is_null()
            && !ctx.is_null()
            && redirect_fault(sig, info, ctx as *mut libc::ucontext_t)
        {
            return;
        }
        let code = crate::crash::report(sig, info, ctx);
        crate::dlls::kernel32::record_exit_code(code);
        libc::_exit(code as i32);
    }
}

/// Pose EXCEPTION_RECORD et CONTEXT x86 sous la pile du programme et fait
/// revenir le signal dans `seh::hw_dispatch`, en 64 bits sur la même pile.
unsafe fn redirect_fault(sig: i32, info: *mut libc::siginfo_t, uc: *mut libc::ucontext_t) -> bool {
    if !crate::dlls::seh_hw::enabled() {
        return false;
    }
    let g = &mut (*uc).uc_mcontext.gregs;
    let eip = g[libc::REG_RIP as usize] as u32;
    // Faute rejouée après une exception non traitée (int3 : EIP est déjà
    // après l'instruction).
    let refire = REFIRE.with(|r| r.replace(0));
    if refire != 0 && (refire == eip || refire.wrapping_add(1) == eip) {
        return false;
    }
    let Some(fault) = crate::dlls::seh_hw::classify(sig, info, uc) else {
        return false;
    };
    let esp = g[libc::REG_RSP as usize] as u32;
    let teb = *((crate::process::teb() + TEB64_TEB32) as *const usize) as u32;
    let limit = rd(teb + 8);
    let size = std::mem::size_of::<seh::HwFrame>() as u32;
    // Assez de place pour la trame et la répartition (gestionnaires x86 et
    // code Rust) ; sinon, rapport de plantage.
    let Some(frame) = esp.checked_sub(256 + size).map(|f| f & !63) else {
        return false;
    };
    if fault.overflow || frame < limit || frame - limit < 64 * 1024 {
        return false;
    }
    let f = &mut *(frame as usize as *mut seh::HwFrame);
    seh::context_from_ucontext(uc, &mut f.ctx, &mut f.fx);
    let mut ctx_eip = fault.ctx_rip as u32;
    if ctx_eip == 0 {
        ctx_eip = eip;
    }
    f.ctx[seh::CTX_EIP..seh::CTX_EIP + 4].copy_from_slice(&ctx_eip.to_le_bytes());
    f.orig = f.ctx;
    let params: Vec<u32> = fault.params[..(fault.nparams as usize).min(2)]
        .iter()
        .map(|&p| p as u32)
        .collect();
    f.rec.fill(0);
    f.rec[0..4].copy_from_slice(&fault.code.to_le_bytes());
    f.rec[12..16].copy_from_slice(&(fault.address as u32).to_le_bytes());
    f.rec[16..20].copy_from_slice(&fault.nparams.to_le_bytes());
    for (i, p) in params.iter().enumerate() {
        f.rec[20 + i * 4..24 + i * 4].copy_from_slice(&p.to_le_bytes());
    }
    // Retour du signal en 64 bits (CS 0x33), FS laissé à glibc.
    let csgsfs = g[libc::REG_CSGSFS as usize] as u64;
    g[libc::REG_CSGSFS as usize] = ((csgsfs & !0xffff) | CS64 as u64) as i64;
    g[libc::REG_RSP as usize] = frame as i64;
    g[libc::REG_RDI as usize] = frame as i64;
    g[libc::REG_RIP as usize] = seh::winlux_wow_fault64 as *const () as usize as i64;
    g[libc::REG_EFL as usize] &= !(0x400 | 0x100);
    true
}

/// Faute du code 64 bits : gestionnaire installé avant nous.
extern "C" fn chain(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    // Pas à pas entré dans une passerelle : sans TLS (FS peut encore être
    // celui du programme x86).
    if sig == libc::SIGTRAP && unsafe { step_trap(info, ctx as *mut libc::ucontext_t) } {
        return;
    }
    let prev = PREVIOUS
        .get(sig as usize)
        .map_or(0, |p| p.load(Ordering::Relaxed));
    if prev > 1 {
        let f: extern "C" fn(i32, *mut libc::siginfo_t, *mut libc::c_void) =
            unsafe { std::mem::transmute(prev) };
        f(sig, info, ctx);
    } else {
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }
}

/// Place `winlux_wow_signal` devant les gestionnaires de fautes installés
/// (plantage, qualité).
pub fn wrap_fault_handlers() {
    if !active() {
        return;
    }
    for sig in [
        libc::SIGSEGV,
        libc::SIGBUS,
        libc::SIGILL,
        libc::SIGFPE,
        libc::SIGTRAP,
        libc::SIGABRT,
    ] {
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(sig, std::ptr::null(), &mut old) != 0 {
                continue;
            }
            if old.sa_sigaction == winlux_wow_signal as *const () as usize {
                continue;
            }
            PREVIOUS[sig as usize].store(old.sa_sigaction, Ordering::Relaxed);
            let mut sa = old;
            sa.sa_sigaction = winlux_wow_signal as *const () as usize;
            sa.sa_flags |= libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        }
    }
}

/// Table de méthodes x86 (stdcall) d'un objet COM du runtime qui lit
/// lui-même la disposition x86 de ses arguments (DirectDraw, Direct3D 8,
/// #237). `methods[i]` = (fonction win64, nombre d'arguments après `this`) ;
/// chaque argument x86 de 4 octets est passé étendu à 64 bits (un `float`
/// garde ses bits dans les 32 bits bas). Rend l'adresse x86 de la table.
/// Comme `x86_vtable`, avec la signature de chaque méthode : une lettre par
/// argument après `this` (`i` : 4 octets étendus, `p` : pointeur vers une
/// case de la taille d'un pointeur, remplie par la méthode), suivie de
/// `>f` si la méthode rend un `float` (#778, Direct3D 9).
pub fn x86_vtable_sig(name: &str, methods: &[(usize, &str)]) -> u32 {
    let table: Vec<u32> = methods
        .iter()
        .enumerate()
        .map(|(i, &(target, sig))| {
            let (args, ret) = sig.split_once('>').unwrap_or((sig, "i"));
            let mut kinds = vec![b'i'];
            kinds.extend(args.bytes());
            let n = kinds.len() as u16;
            make_stub(
                Desc {
                    ret: ret.bytes().next().unwrap_or(b'i'),
                    supported: true,
                    target,
                    kinds,
                    label: format!("{name}::méthode n° {i}"),
                    fix: crt::Fix::None,
                    raw: None,
                    callbacks: 0,
                },
                Some(n * 4),
            ) as u32
        })
        .collect();
    Box::leak(table.into_boxed_slice()).as_ptr() as usize as u32
}

/// Passerelle x86 (stdcall) vers une fonction win64 du runtime qui n'est
/// pas exportée par nom, comme les fonctions d'extension de Winsock rendues
/// par WSAIoctl (#1408) : `n` arguments de 4 octets, étendus à 64 bits.
pub fn x86_stdcall(name: &str, target: usize, n: u8) -> u32 {
    make_stub(
        Desc {
            ret: b'i',
            supported: true,
            target,
            kinds: vec![b'i'; n as usize],
            label: name.to_string(),
            fix: crt::Fix::None,
            raw: None,
            callbacks: 0,
        },
        Some(n as u16 * 4),
    ) as u32
}

pub fn x86_vtable(name: &str, methods: &[(usize, u8)]) -> u32 {
    let table: Vec<u32> = methods
        .iter()
        .enumerate()
        .map(|(i, &(target, n))| {
            make_stub(
                Desc {
                    ret: b'i',
                    supported: true,
                    target,
                    kinds: vec![b'i'; n as usize + 1],
                    label: format!("{name}::méthode n° {i}"),
                    fix: crt::Fix::None,
                    raw: None,
                    callbacks: 0,
                },
                Some(((n as u16) + 1) * 4),
            ) as u32
        })
        .collect();
    Box::leak(table.into_boxed_slice()).as_ptr() as usize as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtre_du_flux_a_plusieurs_motifs() {
        let l = "KERNEL32.dll!MapViewOfFile (esp 0x1, retour 0x2) [0, 0, 0, 0] = 0x0";
        assert!(stream_match("", l));
        assert!(stream_match("mapview", l));
        assert!(stream_match("filemapping, mapview", l));
        assert!(!stream_match("filemapping,virtualquery", l));
        assert!(!stream_match(",", l));
    }

    #[test]
    fn table_stdcall() {
        assert_eq!(stdcall_bytes("ADVAPI32.dll", "RegSetKeySecurity"), Some(12));
        assert_eq!(stdcall_bytes("kernel32", "SetLastError"), Some(4));
        assert_eq!(stdcall_bytes("winspool.drv", "ClosePrinter"), Some(4));
        assert_eq!(stdcall_bytes("kernel32", "Inexistante"), None);
        assert_eq!(stub_pop("msvcrt.dll", "printf", 1, "t"), None);
        assert_eq!(stub_pop("user32.dll", "GetActiveWindow", 0, "t"), Some(0));
    }

    #[test]
    fn signatures_relevees_a_la_compilation() {
        assert_eq!(
            sig_of_path("winlux::dlls::kernel32", "get_std_handle"),
            Some("i>i")
        );
        assert_eq!(
            sig_of_path("winlux::dlls::kernel32", "write_file"),
            Some("iiiii>i")
        );
        assert_eq!(
            sig_of_path("winlux::dlls::kernel32", "get_tick_count64"),
            Some(">q")
        );
        assert_eq!(
            sig_of_path("winlux::dlls::kernel32", "exit_process"),
            Some("i>v")
        );
    }

    /// Exports de la CRT servis en x64 sans passerelle x86, chacun justifié
    /// par la bibliothèque d'import x86 de mingw-w64 (ucrtbase-common.def.in,
    /// lib32/msvcr120.def.in) : ils n'existent pas dans la DLL x86.
    const CRT_X64_SEULEMENT: &[&str] = &[
        // F64(__intrinsic_setjmpex) : x64 seulement.
        "__intrinsic_setjmpex",
        // Absent de ucrtbase et de msvcr120 x86 (_setjmp et _setjmp3 seuls).
        "_setjmpex",
        // F_NON_ARM64 : exporté en x86, mais aucune documentation publique
        // ne décrit sa forme x86, et MSVC x86 appelle _setjmp3. Laissé sans
        // passerelle plutôt que deviné (#2367).
        "__intrinsic_setjmp",
    ];

    /// #2367 : toute fonction que ucrtbase ou msvcrt sert en x64 a une
    /// passerelle x86 (signature relevée par build.rs ou version x86 écrite
    /// à la main). Une fonction engendrée par macro_rules! n'a pas de
    /// signature : l'outil de déploiement d'Office mourait sur iswalpha.
    #[test]
    fn crt_x64_toujours_servie_en_x86() {
        let notes: std::collections::HashSet<usize> = NOTES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(a, _, _)| *a)
            .collect();
        let mut manquants = Vec::new();
        for (dll, names) in crate::dlls::builtin_exports() {
            if dll != "ucrtbase" && dll != "msvcrt" {
                continue;
            }
            for n in names {
                let Some(a) = crate::dlls::lookup(dll, n) else {
                    continue;
                };
                // Données exportées : servies à part, sans signature.
                if !notes.contains(&a) || CRT_X64_SEULEMENT.contains(&n) {
                    continue;
                }
                if sig_of_addr(a).is_none() && override_stub(dll, n).is_none() {
                    manquants.push(format!("{dll}!{n}"));
                }
            }
        }
        manquants.sort();
        assert!(manquants.is_empty(), "sans passerelle x86 : {manquants:?}");
        for n in ["iswalpha", "_iswalpha_l", "_findfirst64i32", "_wfindnext32"] {
            let a = crate::dlls::lookup("ucrtbase", n).unwrap();
            assert!(sig_of_addr(a).is_some(), "{n}");
        }
    }

    /// Adaptateur cdecl → stdcall des `__crt*` : chaque push relit le même
    /// déplacement, la passerelle dépile la copie (#2367).
    #[test]
    fn adaptateur_cdecl_vers_stdcall() {
        assert_eq!(
            super::crt::cdecl_to_stdcall_for_test(0x1234_5678, 8),
            [
                0xff, 0x74, 0x24, 8, 0xff, 0x74, 0x24, 8, 0xb8, 0x78, 0x56, 0x34, 0x12, 0xff, 0xd0,
                0xc3
            ]
        );
        assert_eq!(
            super::crt::cdecl_to_stdcall_for_test(0x10, 0),
            [0xb8, 0x10, 0, 0, 0, 0xff, 0xd0, 0xc3]
        );
        for (c, k) in super::crt::CRT_KERNEL32 {
            assert!(stdcall_bytes("kernel32", k).is_some(), "{c} → {k}");
        }
    }

    #[test]
    fn conventions_d_appel() {
        assert!(is_cdecl("msvcrt.dll", "printf"));
        assert!(is_cdecl("api-ms-win-crt-stdio-l1-1-0.dll", "fwrite"));
        assert!(!is_cdecl("KERNEL32.dll", "WriteFile"));
        assert!(is_cdecl("ntdll.dll", "memcpy"));
        assert!(!is_cdecl("ntdll.dll", "RtlGetVersion"));
        assert!(is_cdecl("CABINET.dll", "#20"));
        assert!(is_cdecl("cabinet.dll", "FDICopy"));
        assert!(is_cdecl("cabinet.dll", "#10"));
        assert!(!is_cdecl("cabinet.dll", "CreateCompressor"));
        assert!(is_cdecl("WLDAP32.dll", "#144"));
    }
}
