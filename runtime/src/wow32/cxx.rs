//! Exceptions C++ MSVC des programmes x86 (#232, étape 7) : 7-Zip,
//! installeurs Burn, tout programme MSVC 32 bits lié à msvcrt.dll ou à
//! vcruntime140.dll.
//!
//! En x86, chaque fonction qui contient des objets à détruire ou des blocs
//! `try` pose un cadre dans la chaîne `fs:[0]` : `[ebp-0x10]` pile
//! sauvegardée, `[ebp-0xC]` cadre suivant, `[ebp-8]` gestionnaire (un
//! petit morceau de code qui charge l'adresse de la FuncInfo dans eax puis
//! saute à `__CxxFrameHandler`), `[ebp-4]` état courant. Les tables
//! (FuncInfo, table des états, blocs `try`, gestionnaires `catch`,
//! ThrowInfo, types capturables) ont la même forme qu'en x64, avec des
//! adresses au lieu de déplacements ; elles sont décrites publiquement
//! (article « Reversing Microsoft Visual C++ Part I: Exception Handling »,
//! OpenRCE) et lues ici comme le fait `dlls/cxx_eh.rs` pour le x64.
//!
//! Un `catch` s'exécute comme sous Windows : déroulement global des cadres
//! au-dessus (`RtlUnwind`), destructeurs du cadre jusqu'au bloc `try`,
//! appel du bloc `catch` (qui rend l'adresse de reprise), destruction de
//! l'objet lancé, puis reprise dans la fonction avec sa pile.

use super::seh;
use super::{call32, rd, wr};
use std::cell::RefCell;
use std::sync::OnceLock;

const CXX_EXCEPTION: u32 = 0xE06D_7363;
const MAGIC: u32 = 0x1993_0520;
const EXCEPTION_UNWINDING: u32 = 2;
const EXCEPTION_EXIT_UNWIND: u32 = 4;
const HT_REFERENCE: u32 = 8;
const HT_ALL: u32 = 0x40;
const CT_SIMPLE: u32 = 1;
const CT_BY_REF_ONLY: u32 = 2;
const CT_VIRTUAL_BASE: u32 = 4;
/// EHFlags : compilé avec /EHs, catch(...) ne prend pas les exceptions SEH.
const FI_EHS: u32 = 1;

fn rdi(p: u32) -> i32 {
    rd(p) as i32
}

struct FuncInfo {
    magic: u32,
    unwind_map: u32,
    try_count: u32,
    try_map: u32,
    flags: u32,
}

impl FuncInfo {
    fn read(p: u32) -> Option<FuncInfo> {
        if p == 0 {
            return None;
        }
        let magic = rd(p) & 0x1FFF_FFFF;
        if !(MAGIC..=MAGIC + 2).contains(&magic) {
            return None;
        }
        Some(FuncInfo {
            magic,
            unwind_map: rd(p + 8),
            try_count: rd(p + 12),
            try_map: rd(p + 16),
            flags: if magic >= MAGIC + 2 { rd(p + 32) } else { 0 },
        })
    }
}

/// Le cadre `fs:[0]` d'une fonction C++ : ebp = cadre + 12.
fn state(frame: u32) -> i32 {
    rdi(frame + 8)
}

/// Destructeurs des états de `from` jusqu'à `to` (exclu).
fn local_unwind(fi: &FuncInfo, frame: u32, to: i32) {
    let ebp = frame + 12;
    loop {
        let s = state(frame);
        if s <= to || s < 0 {
            break;
        }
        let e = fi.unwind_map + s as u32 * 8;
        let next = rdi(e);
        let action = rd(e + 4);
        // État mis à jour avant l'appel : un destructeur qui lève ne
        // relance pas le même déroulement.
        wr(frame + 8, next as u32);
        if action != 0 {
            seh::call_with_ebp(action, ebp);
        }
    }
}

fn type_name(td: u32) -> &'static [u8] {
    unsafe { std::ffi::CStr::from_ptr((td as usize + 8) as *const std::ffi::c_char).to_bytes() }
}

/// Gestionnaire `catch` : adjectifs, type, objet capturé (déplacement
/// depuis ebp), code du bloc.
struct Handler {
    adjectives: u32,
    type_desc: u32,
    catch_obj: i32,
    code: u32,
}

/// Type capturable du ThrowInfo `ti` qui correspond à `h`.
fn find_catchable(ti: u32, h: &Handler) -> Option<u32> {
    let cta = rd(ti + 12);
    let n = rdi(cta).clamp(0, 256) as u32;
    (0..n).map(|i| rd(cta + 4 + i * 4)).find(|&ct| {
        let props = rd(ct);
        let td = rd(ct + 4);
        if props & CT_BY_REF_ONLY != 0 && h.adjectives & HT_REFERENCE == 0 {
            return false;
        }
        td == h.type_desc || type_name(td) == type_name(h.type_desc)
    })
}

/// Déplacement `this` (PMD) d'un type capturable.
fn adjust(obj: u32, ct: u32) -> u32 {
    let mdisp = rdi(ct + 8);
    let pdisp = rdi(ct + 12);
    let vdisp = rdi(ct + 16);
    let mut p = obj as i32;
    if pdisp >= 0 {
        let vbtable = rdi((p + pdisp) as u32);
        p += pdisp + rdi((vbtable + vdisp) as u32);
    }
    (p + mdisp) as u32
}

/// Assistants x86 pour les appels `thiscall` (this dans ecx) avec 0, 1 ou
/// 2 arguments sur la pile : `helper(cible, this, arg…)`.
fn thiscall_helper(args: usize) -> usize {
    static CODE: OnceLock<[usize; 3]> = OnceLock::new();
    CODE.get_or_init(|| {
        let head = [0x8B, 0x44, 0x24, 0x04, 0x8B, 0x4C, 0x24, 0x08]; // eax = cible, ecx = this
        let tail = [0xFF, 0xD0, 0xC3]; // call eax ; ret
        let make = |pushes: &[u8]| {
            let mut c = head.to_vec();
            c.extend_from_slice(pushes);
            c.extend_from_slice(&tail);
            super::alloc_code(&c) as usize
        };
        [
            make(&[]),
            make(&[0xFF, 0x74, 0x24, 0x0C]), // push [esp+12]
            make(&[0xFF, 0x74, 0x24, 0x10, 0xFF, 0x74, 0x24, 0x10]), // push [esp+16] ×2
        ]
    })[args]
}

fn thiscall(target: u32, this: u32, args: &[u32]) {
    let mut a = vec![target, this];
    a.extend_from_slice(args);
    call32(thiscall_helper(args.len()), &a);
}

/// Copie l'objet lancé dans la variable du `catch`.
fn copy_object(obj: u32, ct: u32, h: &Handler, ebp: u32) {
    if h.catch_obj == 0 || h.type_desc == 0 {
        return;
    }
    let dest = (ebp as i32 + h.catch_obj) as u32;
    let src = adjust(obj, ct);
    if h.adjectives & HT_REFERENCE != 0 {
        wr(dest, src);
        return;
    }
    let props = rd(ct);
    let size = rdi(ct + 20).max(0) as usize;
    let copy = rd(ct + 24);
    if props & CT_SIMPLE != 0 || copy == 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(src as usize as *const u8, dest as usize as *mut u8, size)
        };
    } else if props & CT_VIRTUAL_BASE != 0 {
        thiscall(copy, dest, &[src, 1]);
    } else {
        thiscall(copy, dest, &[src]);
    }
}

fn destroy_object(obj: u32, ti: u32) {
    if obj != 0 && ti != 0 {
        let dtor = rd(ti + 4);
        if dtor != 0 {
            thiscall(dtor, obj, &[]);
        }
    }
}

/// Exceptions dont un `catch` est en cours (par thread) : (cadre, objet,
/// ThrowInfo), pour `throw;`.
struct Active {
    frame: u32,
    obj: u32,
    ti: u32,
}

thread_local! {
    static ACTIVE: RefCell<Vec<Active>> = const { RefCell::new(Vec::new()) };
}

/// `_CxxThrowException(objet, ThrowInfo)` ; (0, 0) relance l'exception du
/// `catch` en cours.
pub extern "win64" fn cxx_throw_exception(obj: u32, ti: u32) {
    let (obj, ti) = if ti == 0 {
        match ACTIVE.with(|a| a.borrow().last().map(|e| (e.obj, e.ti))) {
            Some(p) => p,
            None => terminate(),
        }
    } else {
        (obj, ti)
    };
    // Sur la pile du thread, donc sous 4 Go.
    let params = [MAGIC, obj, ti];
    seh::raise_exception(CXX_EXCEPTION, 1, 3, params.as_ptr() as usize as u32);
}

/// `__CxxFrameHandler(rec, cadre, ctx, dc)` avec la FuncInfo dans eax,
/// reçue ici en cinquième argument par le petit code de `frame_handler_code`.
extern "win64" fn frame_handler(rec: u32, frame: u32, ctx: u32, _dc: u32, fi: u32) -> u32 {
    let Some(fi) = FuncInfo::read(fi) else {
        return 1;
    };
    let flags = rd(rec + 4);
    if flags & (EXCEPTION_UNWINDING | EXCEPTION_EXIT_UNWIND) != 0 {
        local_unwind(&fi, frame, -1);
        return 1;
    }
    if fi.try_count == 0 {
        return 1;
    }
    let code = rd(rec);
    let cxx =
        code == CXX_EXCEPTION && rd(rec + 16) >= 3 && (MAGIC..=MAGIC + 2).contains(&rd(rec + 20));
    let (obj, ti) = if cxx {
        (rd(rec + 24), rd(rec + 28))
    } else {
        (0, 0)
    };
    // Exception SEH : seul catch(...) la prend, et pas avec /EHs.
    let seh_catchable = !cxx && (fi.magic == MAGIC || fi.flags & FI_EHS == 0);
    if !cxx && !seh_catchable {
        return 1;
    }
    let ebp = frame + 12;
    let level = state(frame);
    for i in 0..fi.try_count {
        let t = fi.try_map + i * 20;
        let (low, high) = (rdi(t), rdi(t + 4));
        if level < low || level > high {
            continue;
        }
        let count = rd(t + 12);
        let handlers = rd(t + 16);
        for j in 0..count {
            let p = handlers + j * 16;
            let h = Handler {
                adjectives: rd(p),
                type_desc: rd(p + 4),
                catch_obj: rdi(p + 8),
                code: rd(p + 12),
            };
            let all = h.type_desc == 0 || h.adjectives & HT_ALL != 0;
            let ct = if all {
                0
            } else if cxx {
                match find_catchable(ti, &h) {
                    Some(ct) => ct,
                    None => continue,
                }
            } else {
                continue;
            };
            crate::trace!("wow32 : catch C++ trouvé (cadre 0x{frame:x}, état {level})");
            if ct != 0 {
                copy_object(obj, ct, &h, ebp);
            }
            // Cadres posés au-dessus : déroulés et retirés de la chaîne.
            seh::rtl_unwind(frame, 0, rec, 0);
            local_unwind(&fi, frame, low - 1);
            wr(frame + 8, (low - 1) as u32);
            ACTIVE.with(|a| {
                let mut a = a.borrow_mut();
                // Les `catch` des cadres plus bas sur la pile sont abandonnés.
                a.retain(|e| e.frame >= frame);
                a.push(Active { frame, obj, ti });
            });
            // Pile de reprise : celle que le cadre a enregistrée avant le
            // bloc `catch` (le code d'un `catch` peut y mettre la sienne,
            // pour un `try` écrit dans ce `catch`).
            let saved_esp = rd(frame - 4);
            let resume_at = seh::call_with_ebp(h.code, ebp);
            wr(frame - 4, saved_esp);
            ACTIVE.with(|a| {
                a.borrow_mut().pop();
            });
            if cxx {
                destroy_object(obj, ti);
            }
            let mut target = [0u8; seh::CONTEXT32_SIZE];
            let put = |buf: &mut [u8], off: usize, v: u32| {
                buf[off..off + 4].copy_from_slice(&v.to_le_bytes())
            };
            put(&mut target, seh::CTX_FLAGS, 0x0001_0007);
            put(&mut target, seh::CTX_EIP, resume_at);
            put(&mut target, seh::CTX_EBP, ebp);
            put(&mut target, seh::CTX_ESP, saved_esp);
            put(&mut target, seh::CTX_EBX, rd(ctx + seh::CTX_EBX as u32));
            put(&mut target, seh::CTX_ESI, rd(ctx + seh::CTX_ESI as u32));
            put(&mut target, seh::CTX_EDI, rd(ctx + seh::CTX_EDI as u32));
            put(&mut target, seh::CTX_EFLAGS, 0x202);
            seh::transfer(target.as_ptr() as usize as u32)
        }
    }
    1
}

/// Point d'entrée x86 de `__CxxFrameHandler` : pousse eax (FuncInfo)
/// comme cinquième argument et appelle `frame_handler` en cdecl.
pub fn frame_handler_code() -> usize {
    static CODE: OnceLock<usize> = OnceLock::new();
    *CODE.get_or_init(|| {
        let stub = super::native_stub(
            frame_handler as *const () as usize,
            "iiiii>i",
            "msvcrt!__CxxFrameHandler",
        ) as u32;
        let mut c = vec![0x50]; // push eax
        for _ in 0..4 {
            c.extend_from_slice(&[0xFF, 0x74, 0x24, 0x14]); // push [esp+20]
        }
        c.push(0xB9); // mov ecx, stub
        c.extend_from_slice(&stub.to_le_bytes());
        c.extend_from_slice(&[0xFF, 0xD1]); // call ecx
        c.extend_from_slice(&[0x83, 0xC4, 0x14]); // add esp, 20
        c.push(0xC3); // ret
        super::alloc_code(&c) as usize
    })
}

/// `std::terminate` : fin du programme, comme abort().
pub extern "win64" fn terminate() -> ! {
    eprintln!("winlux: exception C++ non rattrapée dans le programme 32 bits (terminate).");
    crate::dlls::kernel32::flush_and_exit(3)
}

/// `_XcptFilter(code, pointeurs)` : filtre du `main` de la CRT msvcrt, qui
/// passe la main au filtre de dernier recours.
pub extern "win64" fn xcpt_filter(_code: u32, pointers: u32) -> i32 {
    seh::unhandled_exception_filter(pointers)
}

/// `__setusermatherr` : gestionnaire des erreurs mathématiques, jamais appelé.
pub extern "win64" fn set_user_matherr(_f: u32) {}
