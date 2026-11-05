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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// Sélecteurs de segments de Linux x86-64 : code 32 bits, code 64 bits, données.
pub const CS32: u16 = 0x23;
pub const CS64: u16 = 0x33;

/// Cases du TEB 64 bits (champ WOW32Reserved et suivants de Windows,
/// inutilisés ailleurs dans Winlux) lues par le code de transition via GS.
const TEB64_FS_GLIBC: usize = 0x100;
const TEB64_FS_SELECTOR: usize = 0x108;
const TEB64_TEB32: usize = 0x110;
const TEB64_RSP_CHAIN: usize = 0x118;

/// Champs du TEB et du PEB 32 bits.
pub const TEB32_SIZE: usize = 0x2000;
const TEB32_EXCEPTION_LIST: usize = 0x00;
const TEB32_STACK_BASE: usize = 0x04;
const TEB32_STACK_LIMIT: usize = 0x08;
const TEB32_SELF: usize = 0x18;
const TEB32_PID: usize = 0x20;
const TEB32_TID: usize = 0x24;
const TEB32_PEB: usize = 0x30;
const PEB32_IMAGE_BASE: usize = 0x08;
const PEB32_PROCESS_HEAP: usize = 0x18;
const PEB32_NUMBER_OF_PROCESSORS: usize = 0x64;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PEB32: AtomicU32 = AtomicU32::new(0);
/// Prochaine entrée de LDT libre (une par thread, pour son TEB 32 bits).
static NEXT_LDT: AtomicU32 = AtomicU32::new(1);
/// Limite haute de la mémoire visible : 2 Go, ou 4 Go si LARGEADDRESSAWARE.
static LIMIT: AtomicUsize = AtomicUsize::new(0x8000_0000);

/// Un programme 32 bits est-il en cours dans ce processus ?
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
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
    .globl winlux_wow_enter32_far
winlux_wow_enter32_far:
    .long 0
    .short {cs32}
    .globl winlux_wow_has_fsgsbase
winlux_wow_has_fsgsbase:
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
    static winlux_wow_enter32_far: u8;
    static winlux_wow_has_fsgsbase: u8;
}

extern "win64" {
    fn winlux_wow_call64(f: usize, slots: *const u64, n: usize, xmm0: *mut u64) -> u64;
}

/// Adresses (sous 4 Go) du code de transition recopié.
struct Blob {
    entry64: u32,
    call32: usize,
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
        *(at(std::ptr::addr_of!(winlux_wow_has_fsgsbase)) as *mut u8) = has_fsgsbase() as u8;
        Blob {
            entry64: at(std::ptr::addr_of!(winlux_wow_entry64)) as u32,
            call32: at(std::ptr::addr_of!(winlux_wow_call32)),
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
fn alloc_code(code: &[u8]) -> u32 {
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
    let path = unsafe { *argv.add(1) };
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
    }
    PEB32.store(peb as u32, Ordering::Relaxed);
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
        let entry = NEXT_LDT.fetch_add(1, Ordering::Relaxed);
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
        *((teb64 + TEB64_RSP_CHAIN) as *mut usize) = 0;
    }
}

// ---------------------------------------------------------------------
// Appels 64 → 32
// ---------------------------------------------------------------------

/// Appelle du code x86 (stdcall ou cdecl) et rend edx:eax.
pub fn call32(target: usize, args: &[u32]) -> u64 {
    assert!(target < limit(), "wow32 : appel 32 bits vers 0x{target:x}");
    let f: extern "win64" fn(usize, *const u32, usize) -> u64 =
        unsafe { std::mem::transmute(blob().call32) };
    f(target, args.as_ptr(), args.len())
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
}

include!(concat!(env!("OUT_DIR"), "/win64_sigs.rs"));

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
        || (n == "ntdll" && func.starts_with(|c: char| c == '_' || c.is_ascii_lowercase()))
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

/// Passerelle x86 vers la fonction intégrée `orig` (appelée via `target`,
/// qui peut être son traceur WINLUX_APITRACE).
pub fn import_stub(dll: &str, func: &str, orig: usize, target: usize) -> usize {
    let label = format!("{}!{func}", dll.to_ascii_lowercase());
    let sig = sig_of_addr(orig);
    let (args, ret) = match sig.and_then(|s| s.split_once('>')) {
        Some((a, r)) => (a.as_bytes().to_vec(), r.as_bytes()[0]),
        None => (b"?".to_vec(), b'?'),
    };
    let supported = sig.is_some() && !args.contains(&b'?') && !args.contains(&b'c') && ret != b'?';
    if !supported {
        crate::trace!("wow32 : {label} pas encore disponible en 32 bits (signature {sig:?})");
    }
    let slots: usize = args
        .iter()
        .map(|k| if matches!(k, b'q' | b'd') { 2 } else { 1 })
        .sum();
    let pop = (!is_cdecl(dll, func)).then_some((slots * 4) as u16);
    make_stub(
        Desc {
            ret,
            supported,
            target,
            kinds: args,
            label,
        },
        pop,
    )
}

/// Passerelle x86 vers le bouchon 64 bits d'une fonction absente.
pub fn missing_stub(dll: &str, func: &str, stub64: usize) -> usize {
    make_stub(
        Desc {
            ret: b'i',
            supported: true,
            target: stub64,
            kinds: Vec::new(),
            label: format!("{}!{func}", dll.to_ascii_lowercase()),
        },
        None,
    )
}

/// Relit les arguments x86 et appelle la fonction win64.
extern "win64" fn dispatch(desc: u32, args: u32, xmm0: *mut u64) -> u64 {
    let d = unsafe { &*(desc as usize as *const Desc) };
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
    let mut slots: Vec<u64> = Vec::with_capacity(d.kinds.len().max(4));
    let mut p = args as usize;
    let rd = |p: usize| unsafe { (p as *const u32).read_unaligned() };
    for k in &d.kinds {
        match k {
            b'q' | b'd' => {
                slots.push(rd(p) as u64 | (rd(p + 4) as u64) << 32);
                p += 8;
            }
            b'f' => {
                slots.push(rd(p) as u64);
                p += 4;
            }
            _ => {
                let v = rd(p);
                // Pseudo-handles (-1, -2…) et constantes négatives gardent
                // leur signe ; adresses et entiers positifs sont complétés
                // par des zéros. Une fonction qui attend un u32 n'en lit
                // de toute façon que les 32 bits bas.
                slots.push(if v >= 0xffff_0000 {
                    v as i32 as i64 as u64
                } else {
                    v as u64
                });
                p += 4;
            }
        }
    }
    let n = slots.len();
    while slots.len() < 4 {
        slots.push(0);
    }
    unsafe { winlux_wow_call64(d.target, slots.as_ptr(), n, xmm0) }
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
);

extern "C" {
    fn winlux_wow_signal();
}

/// Faute dans le code x86 : rapport de plantage. Le SEH 32 bits (chaîne
/// fs:[0]) viendra avec la deuxième étape de #232.
extern "C" fn fault32(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
    unsafe {
        let code = crate::crash::report(sig, info, ctx);
        libc::_exit(code as i32);
    }
}

/// Faute du code 64 bits : gestionnaire installé avant nous.
extern "C" fn chain(sig: i32, info: *mut libc::siginfo_t, ctx: *mut libc::c_void) {
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn conventions_d_appel() {
        assert!(is_cdecl("msvcrt.dll", "printf"));
        assert!(is_cdecl("api-ms-win-crt-stdio-l1-1-0.dll", "fwrite"));
        assert!(!is_cdecl("KERNEL32.dll", "WriteFile"));
        assert!(is_cdecl("ntdll.dll", "memcpy"));
        assert!(!is_cdecl("ntdll.dll", "RtlGetVersion"));
    }
}
