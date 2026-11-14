//! msvcp140.dll maison, suite : `std::exception_ptr` (`<exception>`, #2307).
//!
//! Le compilateur ne voit `std::exception_ptr` qu'à travers dix fonctions
//! exportées par msvcp140 (`__ExceptionPtrCreate`, `…Destroy`, `…Copy`,
//! `…Assign`, `…Compare`, `…ToBool`, `…Swap`, `…CurrentException`,
//! `…Rethrow`, `…CopyException`), plus `std::_Rethrow_future_exception`
//! pour `std::future`. Sans elles, un programme qui construit un seul
//! `exception_ptr` (PPL, `std::promise`, `std::async`) s'arrête sur
//! ENTRYPOINT_NOT_FOUND.
//!
//! Écrit d'après la bibliothèque standard publique de Microsoft (MS STL,
//! licence Apache 2.0 avec exception LLVM) : `stl/src/excptptr.cpp` et
//! `stl/inc/exception`. Ce qu'on en reprend :
//!
//! - `exception_ptr` fait 16 octets, la disposition d'un
//!   `shared_ptr<const EXCEPTION_RECORD>` : `_Ptr` (le record) à +0,
//!   `_Rep` (le bloc qui compte les références) à +8. Vide = deux zéros ;
//!   `ToBool` et `Compare` ne regardent que `_Ptr`.
//! - Capturer une exception C++ copie son objet, avec le constructeur de
//!   copie du premier type capturable de son ThrowInfo, dans le même bloc
//!   que le record ; le record capturé désigne cette copie. Le dernier
//!   `exception_ptr` qui la lâche appelle le destructeur du ThrowInfo.
//! - Une exception non C++ (SEH) est capturée par copie de son record.
//! - `Rethrow` lève une **nouvelle copie** de l'objet, que le `catch` qui la
//!   reçoit détruit à sa fin, comme une exception lancée par `throw` :
//!   l'objet tenu par l'`exception_ptr` reste intact.
//!
//! Nos choix, dits :
//!
//! - Le bloc compteur est le nôtre (`Bloc`), pas un `_Ref_count_base` de la
//!   MS STL : les programmes n'y touchent qu'à travers ces fonctions.
//! - La copie levée par `Rethrow` vit sur le tas (la MS STL la place sur la
//!   pile par `alloca`) ; `cxx_eh` la rend dès que le `catch` a détruit
//!   l'objet (`liberer_copie_relancee`).
//! - `Rethrow` d'un `exception_ptr` vide, que la MS STL signale par
//!   `throw bad_exception()`, arrête le programme avec un message, comme
//!   nos autres fonctions `_X*` (aucun ThrowInfo de `std::bad_exception`
//!   n'existe chez nous).
//! - Un constructeur de copie qui lève pendant la capture n'est pas
//!   rattrapé (la MS STL capture alors cette seconde exception).
//! - `_Rethrow_future_exception` reçoit son `exception_ptr` par adresse
//!   (objet non trivial passé par valeur, ABI x64) ; la référence de ce
//!   temporaire n'est pas rendue quand l'exception remonte.

use super::cxx_eh;
use super::seh::{self, ExceptionRecord, EXCEPTION_NONCONTINUABLE};
use super::Exports;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Bloc partagé par les copies d'un même `exception_ptr`.
#[repr(C)]
struct Bloc {
    /// `_Ptr` d'un `exception_ptr` désigne ce champ.
    rec: ExceptionRecord,
    refs: AtomicUsize,
    /// Copie de l'objet C++ (alignée sur 16 octets, comme `malloc`).
    objet: Box<[u128]>,
    /// ThrowInfo et base de son image, pour le destructeur.
    ti: usize,
    base: usize,
}

/// Les deux mots d'un `exception_ptr`.
#[repr(C)]
struct Eptr {
    ptr: usize,
    rep: usize,
}

unsafe fn eptr<'a>(p: *mut u8) -> &'a mut Eptr {
    &mut *(p as *mut Eptr)
}

fn retenir(rep: usize) {
    if rep != 0 {
        unsafe { (*(rep as *const Bloc)).refs.fetch_add(1, Ordering::Relaxed) };
    }
}

unsafe fn lacher(rep: usize) {
    if rep == 0 {
        return;
    }
    let b = rep as *mut Bloc;
    if (*b).refs.fetch_sub(1, Ordering::AcqRel) != 1 {
        return;
    }
    let b = Box::from_raw(b);
    let obj = b.rec.info[1];
    if b.ti != 0 && obj != 0 {
        cxx_eh::destroy_thrown(obj, b.ti, b.base);
    }
    drop(b);
}

fn emballer(b: Bloc) -> Eptr {
    let b = Box::into_raw(Box::new(b));
    let rec = unsafe { std::ptr::addr_of!((*b).rec) as usize };
    Eptr {
        ptr: rec,
        rep: b as usize,
    }
}

/// Paramètres d'une exception C++ (objet, ThrowInfo, base), comme
/// `PER_IS_MSVC_PURE_OR_NATIVE_EH` de la MS STL.
fn params_cxx(rec: &ExceptionRecord) -> Option<(usize, usize, usize)> {
    if rec.code != cxx_eh::CXX_EXCEPTION
        || rec.nparams < 3
        || !(0x1993_0520..=0x1993_0522).contains(&rec.info[0])
        || rec.info[2] == 0
    {
        return None;
    }
    let base = if rec.nparams >= 4 && rec.info[3] != 0 {
        rec.info[3]
    } else {
        seh::module_base(rec.info[2]).unwrap_or(0)
    };
    Some((rec.info[1], rec.info[2], base))
}

/// Copie d'un record (`_CopyExceptionRecord`) : non continuable, sans
/// chaînage, paramètres au-delà de `nparams` à zéro.
fn copie_record(src: &ExceptionRecord) -> ExceptionRecord {
    let mut r = ExceptionRecord::new(src.code, src.flags | EXCEPTION_NONCONTINUABLE, 0);
    let n = (src.nparams as usize).min(15);
    r.nparams = n as u32;
    r.info[..n].copy_from_slice(&src.info[..n]);
    r
}

/// Copie l'objet d'une exception C++ dans un tampon neuf aligné sur 16.
unsafe fn copier_objet(obj: usize, ti: usize, base: usize) -> Box<[u128]> {
    let taille = cxx_eh::thrown_size(ti, base);
    let mut buf = vec![0u128; taille.div_ceil(16).max(1)].into_boxed_slice();
    if obj != 0 {
        cxx_eh::copy_thrown(buf.as_mut_ptr() as usize, obj, ti, base);
    }
    buf
}

/// `_Assign_cpp_exception_ptr_from_record` / `_Assign_seh_…` : un
/// `exception_ptr` neuf qui tient une copie de l'exception `rec`.
unsafe fn capturer(rec: &ExceptionRecord) -> Eptr {
    let mut copie = copie_record(rec);
    match params_cxx(rec) {
        Some((obj, ti, base)) => {
            let objet = copier_objet(obj, ti, base);
            copie.info[1] = if obj != 0 { objet.as_ptr() as usize } else { 0 };
            if copie.nparams >= 4 {
                copie.info[3] = base;
            }
            emballer(Bloc {
                rec: copie,
                refs: AtomicUsize::new(1),
                objet,
                ti,
                base,
            })
        }
        None => emballer(Bloc {
            rec: copie,
            refs: AtomicUsize::new(1),
            objet: Box::new([]),
            ti: 0,
            base: 0,
        }),
    }
}

unsafe extern "win64" fn eptr_create(p: *mut u8) {
    *eptr(p) = Eptr { ptr: 0, rep: 0 };
}

unsafe extern "win64" fn eptr_destroy(p: *mut u8) {
    let e = eptr(p);
    let rep = e.rep;
    *e = Eptr { ptr: 0, rep: 0 };
    lacher(rep);
}

unsafe extern "win64" fn eptr_copy(dest: *mut u8, src: *const u8) {
    let s = eptr(src as *mut u8);
    retenir(s.rep);
    *eptr(dest) = Eptr {
        ptr: s.ptr,
        rep: s.rep,
    };
}

unsafe extern "win64" fn eptr_assign(dest: *mut u8, src: *const u8) {
    let s = eptr(src as *mut u8);
    let (ptr, rep) = (s.ptr, s.rep);
    retenir(rep);
    let d = eptr(dest);
    let ancien = d.rep;
    *d = Eptr { ptr, rep };
    lacher(ancien);
}

unsafe extern "win64" fn eptr_compare(a: *const u8, b: *const u8) -> bool {
    eptr(a as *mut u8).ptr == eptr(b as *mut u8).ptr
}

unsafe extern "win64" fn eptr_to_bool(p: *const u8) -> bool {
    eptr(p as *mut u8).ptr != 0
}

unsafe extern "win64" fn eptr_swap(a: *mut u8, b: *mut u8) {
    std::ptr::swap(a as *mut Eptr, b as *mut Eptr);
}

/// `std::current_exception()` : sans exception en cours de traitement,
/// l'`exception_ptr` (déjà construit vide par l'appelant) n'est pas touché.
unsafe extern "win64" fn eptr_current_exception(p: *mut u8) {
    let rec = *cxx_eh::current_exception() as *const ExceptionRecord;
    if rec.is_null() {
        return;
    }
    *eptr(p) = capturer(&*rec);
}

/// `std::make_exception_ptr(e)` : `__ExceptionPtrCopyException(&p, &e,
/// ThrowInfo de e)`.
unsafe extern "win64" fn eptr_copy_exception(p: *mut u8, obj: *const u8, ti: *const u8) {
    let mut rec = ExceptionRecord::new(cxx_eh::CXX_EXCEPTION, EXCEPTION_NONCONTINUABLE, 0);
    rec.nparams = 4;
    rec.info[0] = 0x1993_0520;
    rec.info[1] = obj as usize;
    rec.info[2] = ti as usize;
    rec.info[3] = seh::module_base(ti as usize).unwrap_or(0);
    *eptr(p) = capturer(&rec);
}

// ---------- Relancer ----------

/// Copies d'objets levées par `Rethrow`, rendues quand le `catch` qui les
/// a reçues les détruit.
static RELANCEES: Mutex<Option<HashMap<usize, Box<[u128]>>>> = Mutex::new(None);
static NB_RELANCEES: AtomicUsize = AtomicUsize::new(0);

/// Appelé par `cxx_eh` après la destruction d'un objet lancé : rend le
/// tampon s'il vient de `Rethrow`.
pub(crate) fn liberer_copie_relancee(obj: usize) {
    if NB_RELANCEES.load(Ordering::Acquire) == 0 {
        return;
    }
    let mut g = RELANCEES.lock().unwrap_or_else(|e| e.into_inner());
    if g.as_mut().and_then(|m| m.remove(&obj)).is_some() {
        NB_RELANCEES.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Arguments de RaiseException préparés pour `Rethrow` : code, drapeaux,
/// nombre de paramètres puis les paramètres.
#[repr(C)]
struct Relance {
    code: usize,
    flags: usize,
    nparams: usize,
    info: [usize; 15],
}

thread_local! {
    static RELANCE: RefCell<Relance> = const {
        RefCell::new(Relance { code: 0, flags: 0, nparams: 0, info: [0; 15] })
    };
}

unsafe extern "win64" fn preparer_relance(p: *const u8) -> *const Relance {
    let e = eptr(p as *mut u8);
    if e.ptr == 0 {
        super::msvcp140::fatal(
            "bad_exception",
            c"rethrow_exception(nullptr)".as_ptr().cast(),
        );
    }
    let mut rec = copie_record(&*(e.ptr as *const ExceptionRecord));
    if let Some((obj, ti, base)) = params_cxx(&rec) {
        if obj != 0 {
            let copie = copier_objet(obj, ti, base);
            let adr = copie.as_ptr() as usize;
            rec.info[1] = adr;
            let mut g = RELANCEES.lock().unwrap_or_else(|e| e.into_inner());
            g.get_or_insert_with(HashMap::new).insert(adr, copie);
            NB_RELANCEES.fetch_add(1, Ordering::AcqRel);
        }
    }
    RELANCE.with(|r| {
        let mut r = r.borrow_mut();
        r.code = rec.code as usize;
        r.flags = rec.flags as usize;
        r.nparams = rec.nparams as usize;
        r.info = rec.info;
        &*r as *const Relance
    })
}

/// `std::rethrow_exception(p)` : lève, depuis le cadre de l'appelant, une
/// copie de l'exception tenue par `p`.
#[unsafe(naked)]
unsafe extern "win64" fn eptr_rethrow(_p: *const u8) {
    core::arch::naked_asm!(
        "sub rsp, 0x28",
        "call {prepare}",
        "add rsp, 0x28",
        "mov ecx, [rax]",
        "mov edx, [rax + 8]",
        "mov r8d, [rax + 16]",
        "lea r9, [rax + 24]",
        "jmp {raise}",
        prepare = sym preparer_relance,
        raise = sym seh::raise_exception,
    )
}

pub fn exports() -> Exports {
    crate::exports! {
        "?__ExceptionPtrCreate@@YAXPEAX@Z" => fn eptr_create,
        "?__ExceptionPtrDestroy@@YAXPEAX@Z" => fn eptr_destroy,
        "?__ExceptionPtrCopy@@YAXPEAXPEBX@Z" => fn eptr_copy,
        "?__ExceptionPtrAssign@@YAXPEAXPEBX@Z" => fn eptr_assign,
        "?__ExceptionPtrCompare@@YA_NPEBX0@Z" => fn eptr_compare,
        "?__ExceptionPtrToBool@@YA_NPEBX@Z" => fn eptr_to_bool,
        "?__ExceptionPtrSwap@@YAXPEAX0@Z" => fn eptr_swap,
        "?__ExceptionPtrCurrentException@@YAXPEAX@Z" => fn eptr_current_exception,
        "?__ExceptionPtrRethrow@@YAXPEBX@Z" => fn eptr_rethrow,
        "?__ExceptionPtrCopyException@@YAXPEAXPEBX1@Z" => fn eptr_copy_exception,
        "?_Rethrow_future_exception@std@@YAXVexception_ptr@1@@Z" => fn eptr_rethrow,
    }
}

#[cfg(test)]
mod tests;
