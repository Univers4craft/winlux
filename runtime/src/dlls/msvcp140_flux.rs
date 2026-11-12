//! msvcp140.dll maison, suite (#2195) : les flux C++ (`<istream>`,
//! `<ostream>`, `<streambuf>`, `std::cout`, `std::cerr`) et les locales
//! (`std::locale`, facettes `ctype`, `codecvt`, `time_put`) que les
//! programmes compilés avec Visual Studio importent de msvcp140.dll.
//!
//! Écrit d'après la bibliothèque standard publique de Microsoft (MS STL,
//! licence Apache 2.0) : `stl/inc/xiosbase`, `ios`, `streambuf`, `istream`,
//! `__msvc_ostream.hpp`, `xlocale`, `xlocinfo`, `xloctime`, `iomanip` et
//! `stl/src/locale0.cpp`, `locale.cpp`, `ios.cpp`, `fiopen.cpp`,
//! `xstrcoll.cpp`, `xstrxfrm.cpp`, `cout.cpp`, `cerr.cpp`. Les dispositions
//! en mémoire, l'ordre des tables virtuelles (les surcharges virtuelles y
//! sont rangées à l'envers de leur déclaration) et l'ABI des classes à base
//! virtuelle ont été vérifiés en compilant ces déclarations avec clang en
//! mode MSVC (`-fdump-record-layouts`, `-fdump-vtable-layouts`) :
//!
//! - `ios_base` (72 octets) : vfptr, `_Stdstr` +8, `_Mystate` +16,
//!   `_Except` +20, `_Fmtfl` +24, `_Prec` +32, `_Wide` +40, `_Arr` +48,
//!   `_Calls` +56, `_Ploc` +64 ; `basic_ios` ajoute `_Mystrbuf` +72,
//!   `_Tiestr` +80, `_Fillch` +88 (96 octets).
//! - `basic_istream` : vbptr, `_Chcount` +8, vtordisp +20, `basic_ios` +24 ;
//!   `basic_ostream` : vbptr, vtordisp +12, `basic_ios` +16 ;
//!   `basic_iostream` : les deux vbptr (+0, +16), vtordisp +28,
//!   `basic_ios` +32. Leurs constructeurs reçoivent un dernier argument
//!   caché (« objet le plus dérivé ») ; leur destructeur reçoit `this`
//!   placé sur la base virtuelle à son décalage statique.
//! - `basic_streambuf` (104 octets) : 15 entrées virtuelles dans l'ordre
//!   de déclaration ; les accès passent par les pointeurs indirects
//!   (`_IGfirst`…) comme dans l'en-tête.
//! - `locale` = un pointeur vers `_Locimp` (56 octets : facette, vecteur de
//!   facettes +16, nombre +24, catégories +32, transparente +36, nom +40).
//!
//! Limites, dites : les facettes se comportent comme la locale « C » (la
//! seule que notre CRT connaît) ; une exception que la MS STL lèverait
//! (`ios_base::failure` sur un masque d'exceptions, nom de locale nul)
//! arrête le programme avec un message, comme les autres fonctions `_X*`
//! de notre msvcp140.

use super::Exports;
use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};
use std::sync::OnceLock;

type P = *mut u8;

// ---------- mémoire ----------

unsafe fn rd<T: Copy>(p: *const u8, off: usize) -> T {
    (p.add(off) as *const T).read_unaligned()
}

unsafe fn wr<T>(p: *mut u8, off: usize, v: T) {
    (p.add(off) as *mut T).write_unaligned(v)
}

/// Adresse de l'entrée `i` de la table virtuelle de `obj`.
unsafe fn slot(obj: *const u8, i: usize) -> usize {
    *rd::<*const usize>(obj, 0).add(i)
}

fn alloc_zeroed(n: usize) -> P {
    let p = unsafe { libc::calloc(1, n.max(1)) } as P;
    if p.is_null() {
        super::msvcp140::bad_alloc_fatal();
    }
    p
}

unsafe fn free<T>(p: *mut T) {
    libc::free(p as *mut c_void)
}

fn fatal(what: &str, msg: &[u8]) -> ! {
    super::msvcp140::fatal(what, msg.as_ptr())
}

// ---------- verrous (_Lockit) ----------

/// Verrou réentrant, un par type (`_LOCK_LOCALE` = 0, `_LOCK_STREAM` = 2…).
struct RLock {
    word: AtomicU32,
    owner: AtomicI32,
    depth: AtomicU32,
}

#[allow(clippy::declare_interior_mutable_const)]
const RL: RLock = RLock {
    word: AtomicU32::new(0),
    owner: AtomicI32::new(0),
    depth: AtomicU32::new(0),
};
static LOCKS: [RLock; 8] = [RL; 8];
const LOCK_LOCALE: i32 = 0;
const LOCK_STREAM: i32 = 2;

fn tid() -> i32 {
    unsafe { libc::syscall(libc::SYS_gettid) as i32 }
}

fn lock(kind: i32) {
    let Some(l) = usize::try_from(kind).ok().and_then(|k| LOCKS.get(k)) else {
        return;
    };
    let me = tid();
    if l.owner.load(Ordering::Relaxed) == me {
        l.depth.fetch_add(1, Ordering::Relaxed);
        return;
    }
    if l.word
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        while l.word.swap(2, Ordering::Acquire) != 0 {
            unsafe { super::msvcp140::futex_wait(&l.word, 2, None) };
        }
    }
    l.owner.store(me, Ordering::Relaxed);
    l.depth.store(1, Ordering::Relaxed);
}

fn unlock(kind: i32) {
    let Some(l) = usize::try_from(kind).ok().and_then(|k| LOCKS.get(k)) else {
        return;
    };
    if l.owner.load(Ordering::Relaxed) != tid() {
        return;
    }
    if l.depth.fetch_sub(1, Ordering::Relaxed) == 1 {
        l.owner.store(0, Ordering::Relaxed);
        if l.word.swap(0, Ordering::Release) == 2 {
            unsafe { super::msvcp140::futex_wake(&l.word, 1) };
        }
    }
}

struct Held(i32);
impl Held {
    fn new(kind: i32) -> Held {
        lock(kind);
        Held(kind)
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        unlock(self.0)
    }
}

unsafe extern "win64" fn lockit_ctor(this: P, kind: i32) -> P {
    wr(this, 0, kind);
    lock(kind);
    this
}

unsafe extern "win64" fn lockit_dtor(this: P) {
    unlock(rd::<i32>(this, 0));
}

// ---------- _Yarn<char> : { _Myptr, _Nul } ----------

unsafe fn yarn_set(y: P, s: *const u8) {
    let cur: P = rd(y, 0);
    if std::ptr::eq(cur, s) {
        return;
    }
    free(cur);
    wr(y, 0, std::ptr::null_mut::<u8>());
    if !s.is_null() {
        let n = libc::strlen(s as *const libc::c_char) + 1;
        let p = alloc_zeroed(n);
        std::ptr::copy_nonoverlapping(s, p, n);
        wr(y, 0, p);
    }
}

unsafe fn yarn_cstr(y: *const u8) -> *const u8 {
    let p: *const u8 = rd(y, 0);
    if p.is_null() {
        y.add(8)
    } else {
        p
    }
}

unsafe fn yarn_free(y: P) {
    free(rd::<P>(y, 0));
    wr(y, 0, std::ptr::null_mut::<u8>());
}

unsafe extern "win64" fn yarn_assign(this: P, s: *const u8) -> P {
    yarn_set(this, s);
    this
}

unsafe extern "win64" fn yarn_c_str(this: P) -> *const u8 {
    yarn_cstr(this)
}

// ---------- facettes ----------

const REFS: usize = 8;

unsafe fn refs(f: P) -> &'static AtomicU32 {
    &*(f.add(REFS) as *const AtomicU32)
}

unsafe extern "win64" fn facet_incref(this: P) {
    refs(this).fetch_add(1, Ordering::AcqRel);
}

unsafe extern "win64" fn facet_decref(this: P) -> P {
    if refs(this).fetch_sub(1, Ordering::AcqRel) == 1 {
        this
    } else {
        std::ptr::null_mut()
    }
}

unsafe extern "win64" fn facet_ctor(this: P, initrefs: usize) -> P {
    wr(this, 0, tables().facet.as_ptr());
    wr(this, REFS, initrefs as u32);
    this
}

unsafe extern "win64" fn facet_dtor(_this: P) {}

unsafe extern "win64" fn facet_deleting(this: P, flags: u32) -> P {
    if flags & 1 != 0 {
        free(this);
    }
    this
}

/// `_Ptr->_Incref()` par la table virtuelle.
unsafe fn incref(f: P) {
    let g: unsafe extern "win64" fn(P) = std::mem::transmute(slot(f, 1));
    g(f)
}

/// `delete f->_Decref()` par la table virtuelle.
unsafe fn release(f: P) {
    if f.is_null() {
        return;
    }
    let dec: unsafe extern "win64" fn(P) -> P = std::mem::transmute(slot(f, 2));
    let dead = dec(f);
    if !dead.is_null() {
        let del: unsafe extern "win64" fn(P, u32) -> P = std::mem::transmute(slot(dead, 0));
        del(dead, 1);
    }
}

// ---------- locale::id ----------

static ID_CNT: AtomicI32 = AtomicI32::new(0);
static ID_CTYPE_C: AtomicUsize = AtomicUsize::new(0);
static ID_CTYPE_W: AtomicUsize = AtomicUsize::new(0);
static ID_CODECVT_C: AtomicUsize = AtomicUsize::new(0);
static ID_CODECVT_W: AtomicUsize = AtomicUsize::new(0);
static ID_COLLATE_C: AtomicUsize = AtomicUsize::new(0);
static ID_TIME_PUT_C: AtomicUsize = AtomicUsize::new(0);

/// `locale::id::_Get_index` : numéro attribué au premier usage.
fn id_index(id: &AtomicUsize) -> usize {
    if id.load(Ordering::Acquire) == 0 {
        let _g = Held::new(LOCK_LOCALE);
        if id.load(Ordering::Acquire) == 0 {
            let n = ID_CNT.fetch_add(1, Ordering::AcqRel) + 1;
            id.store(n as usize, Ordering::Release);
        }
    }
    id.load(Ordering::Acquire)
}

// ---------- locale::_Locimp ----------

const LI_VEC: usize = 16;
const LI_COUNT: usize = 24;
const LI_CATMASK: usize = 32;
const LI_XPARENT: usize = 36;
const LI_NAME: usize = 40;
const LI_SIZE: usize = 56;
const CAT_CTYPE: i32 = 2;
const CAT_TIME: i32 = 16;
const CAT_ALL: i32 = 63;

static GLOBAL: AtomicUsize = AtomicUsize::new(0);
/// L'objet `locale` « C » rendu par `locale::classic()` (un pointeur).
static CLASSIC: AtomicUsize = AtomicUsize::new(0);

unsafe fn locimp_alloc() -> P {
    let p = alloc_zeroed(LI_SIZE);
    wr(p, 0, tables().locimp.as_ptr());
    wr(p, REFS, 1u32);
    p
}

unsafe extern "win64" fn new_locimp(transparent: bool) -> P {
    let p = locimp_alloc();
    wr(p, LI_XPARENT, transparent);
    yarn_set(p.add(LI_NAME), c"*".as_ptr() as *const u8);
    p
}

unsafe extern "win64" fn new_locimp_copy(right: P) -> P {
    let p = locimp_alloc();
    wr(p, LI_COUNT, rd::<usize>(right, LI_COUNT));
    wr(p, LI_CATMASK, rd::<i32>(right, LI_CATMASK));
    wr(p, LI_XPARENT, rd::<bool>(right, LI_XPARENT));
    yarn_set(p.add(LI_NAME), yarn_cstr(right.add(LI_NAME)));
    if right as usize == CLASSIC.load(Ordering::Acquire) {
        // Copie de la locale « C » : ses facettes standard.
        wr(p, LI_COUNT, 0usize);
        let mut info = [0u8; LOCINFO_SIZE];
        locinfo_ctor(info.as_mut_ptr(), c"C".as_ptr() as *const u8);
        makeloc(info.as_mut_ptr(), CAT_ALL, p, std::ptr::null());
        locinfo_dtor(info.as_mut_ptr());
    } else {
        let _g = Held::new(LOCK_LOCALE);
        let n: usize = rd(p, LI_COUNT);
        if n > 0 {
            let vec = alloc_zeroed(n * 8) as *mut P;
            let src: *const P = rd(right, LI_VEC);
            for i in 0..n {
                let f = *src.add(i);
                *vec.add(i) = f;
                if !f.is_null() {
                    incref(f);
                }
            }
            wr(p, LI_VEC, vec);
        }
    }
    p
}

unsafe extern "win64" fn locimp_deleting(this: P, flags: u32) -> P {
    {
        let _g = Held::new(LOCK_LOCALE);
        let n: usize = rd(this, LI_COUNT);
        let vec: *mut P = rd(this, LI_VEC);
        if !vec.is_null() {
            for i in (0..n).rev() {
                release(*vec.add(i));
            }
        }
        free(vec);
    }
    yarn_free(this.add(LI_NAME));
    if flags & 1 != 0 {
        free(this);
    }
    this
}

unsafe extern "win64" fn locimp_addfac(this: P, fac: P, id: usize) {
    let _g = Held::new(LOCK_LOCALE);
    let count: usize = rd(this, LI_COUNT);
    let mut vec: *mut P = rd(this, LI_VEC);
    if vec.is_null() {
        wr(this, LI_COUNT, 0usize);
    }
    let count = if vec.is_null() { 0 } else { count };
    if count <= id {
        let n = (id + 1).max(40);
        vec = libc::realloc(vec as *mut c_void, n * 8) as *mut P;
        if vec.is_null() {
            super::msvcp140::bad_alloc_fatal();
        }
        for i in count..n {
            *vec.add(i) = std::ptr::null_mut();
        }
        wr(this, LI_VEC, vec);
        wr(this, LI_COUNT, n);
    }
    incref(fac);
    release(*vec.add(id));
    *vec.add(id) = fac;
}

/// `locale::_Getfacet`, comme l'en-tête : la locale, sinon la globale si
/// elle est transparente.
unsafe fn getfacet(imp: P, id: usize) -> P {
    let n: usize = rd(imp, LI_COUNT);
    let vec: *const P = rd(imp, LI_VEC);
    let f = if id < n && !vec.is_null() {
        *vec.add(id)
    } else {
        std::ptr::null_mut()
    };
    if !f.is_null() || !rd::<bool>(imp, LI_XPARENT) {
        return f;
    }
    let g = GLOBAL.load(Ordering::Acquire) as P;
    if g.is_null() {
        return std::ptr::null_mut();
    }
    let n: usize = rd(g, LI_COUNT);
    let vec: *const P = rd(g, LI_VEC);
    if id < n && !vec.is_null() {
        *vec.add(id)
    } else {
        std::ptr::null_mut()
    }
}

type Maker = unsafe fn() -> P;

/// Facettes que notre msvcp140 sait construire, par catégorie.
fn standard_facets() -> [(i32, &'static AtomicUsize, Maker); 5] {
    [
        (CAT_CTYPE, &ID_CTYPE_C, new_ctype_c as Maker),
        (CAT_CTYPE, &ID_CTYPE_W, new_ctype_w as Maker),
        (CAT_CTYPE, &ID_CODECVT_C, new_codecvt_c as Maker),
        (CAT_CTYPE, &ID_CODECVT_W, new_codecvt_w as Maker),
        (CAT_TIME, &ID_TIME_PUT_C, new_time_put_c as Maker),
    ]
}

/// `use_facet` de nos propres facettes : celle de la locale, sinon une
/// facette paresseuse partagée (jamais libérée, comme `_Facet_Register`).
unsafe fn use_own_facet(imp: P, id: &AtomicUsize, make: Maker) -> P {
    static LAZY: OnceLock<Vec<AtomicUsize>> = OnceLock::new();
    let idx = id_index(id);
    let f = getfacet(imp, idx);
    if !f.is_null() {
        return f;
    }
    let lazy = LAZY.get_or_init(|| (0..5).map(|_| AtomicUsize::new(0)).collect());
    let k = standard_facets()
        .iter()
        .position(|(_, i, _)| std::ptr::eq(*i, id))
        .unwrap_or(0);
    let _g = Held::new(LOCK_LOCALE);
    let mut p = lazy[k].load(Ordering::Acquire) as P;
    if p.is_null() {
        p = make();
        incref(p);
        lazy[k].store(p as usize, Ordering::Release);
    }
    p
}

unsafe extern "win64" fn makeloc(lobj: P, cat: i32, imp: P, ploc: *const usize) -> P {
    for (c, id, make) in standard_facets() {
        if cat & c == 0 {
            continue;
        }
        let idx = id_index(id);
        if ploc.is_null() {
            locimp_addfac(imp, make(), idx);
        } else {
            let f = use_own_facet(*ploc as P, id, make);
            locimp_addfac(imp, f, idx);
        }
    }
    wr(imp, LI_CATMASK, rd::<i32>(imp, LI_CATMASK) | cat);
    yarn_set(imp.add(LI_NAME), locinfo_getname(lobj));
    imp
}

/// `locale::_Init` : crée la locale globale et la locale « C » au premier
/// appel (sans facette : elles sont créées à la demande, comme MSVC).
unsafe extern "win64" fn locale_init(do_incref: bool) -> P {
    let _g = Held::new(LOCK_LOCALE);
    let mut p = GLOBAL.load(Ordering::Acquire) as P;
    if p.is_null() {
        p = new_locimp(false);
        wr(p, LI_CATMASK, CAT_ALL);
        yarn_set(p.add(LI_NAME), c"C".as_ptr() as *const u8);
        incref(p);
        CLASSIC.store(p as usize, Ordering::Release);
        GLOBAL.store(p as usize, Ordering::Release);
    }
    if do_incref {
        incref(p);
    }
    p
}

unsafe extern "win64" fn getgloballocale() -> P {
    GLOBAL.load(Ordering::Acquire) as P
}

unsafe extern "win64" fn locale_classic() -> *const AtomicUsize {
    if CLASSIC.load(Ordering::Acquire) == 0 {
        locale_init(false);
    }
    &CLASSIC
}

/// `new locale` : un objet `locale` sur le tas, copie de la globale.
unsafe fn new_locale() -> *mut usize {
    let l = alloc_zeroed(8) as *mut usize;
    *l = locale_init(true) as usize;
    l
}

unsafe fn locale_copy(dst: *mut usize, src: *const usize) {
    *dst = *src;
    if *dst != 0 {
        incref(*dst as P);
    }
}

unsafe fn locale_assign(dst: *mut usize, src: *const usize) {
    if *dst != *src {
        release(*dst as P);
        *dst = *src;
        incref(*dst as P);
    }
}

unsafe fn delete_locale(l: *mut usize) {
    if !l.is_null() {
        release(*l as P);
        free(l);
    }
}

// ---------- _Locinfo ----------

const LOCINFO_SIZE: usize = 104;
const LINFO_OLD: usize = 72;
const LINFO_NEW: usize = 88;

/// `_Locinfo_ctor` / `_Locinfo_Addcats` : passe la CRT sur la locale
/// demandée, retient l'ancienne pour la remettre à la destruction.
unsafe fn locinfo_start(this: P, cat: i32, name: *const u8) {
    lockit_ctor(this, LOCK_LOCALE);
    std::ptr::write_bytes(this.add(8), 0, LOCINFO_SIZE - 8);
    // Ancien nom (large), rangé tel quel dans le _Yarn<char> (ABI MSVC).
    let old = super::crt_runtime::wsetlocale(0, std::ptr::null());
    if !old.is_null() {
        let mut n = 0;
        while *old.add(n) != 0 {
            n += 1;
        }
        let p = alloc_zeroed((n + 1) * 2);
        std::ptr::copy_nonoverlapping(old as *const u8, p, n * 2);
        wr(this, LINFO_OLD, p);
    }
    if name.is_null() {
        fatal("runtime_error", b"bad locale name\0");
    }
    let star = *name == b'*' && *name.add(1) == 0;
    let mut newname: *const u8 = std::ptr::null();
    if !star {
        newname = if cat == 0 {
            super::crt_runtime::setlocale(0, std::ptr::null())
        } else {
            super::crt_runtime::setlocale(0, name)
        };
    }
    if newname.is_null() {
        yarn_set(this.add(LINFO_NEW), c"*".as_ptr() as *const u8);
    } else {
        yarn_set(this.add(LINFO_NEW), newname);
    }
}

unsafe extern "win64" fn locinfo_ctor(this: P, name: *const u8) -> P {
    locinfo_start(this, CAT_ALL, name);
    this
}

unsafe extern "win64" fn locinfo_ctor_cat(this: P, cat: i32, name: *const u8) -> P {
    locinfo_start(this, cat, name);
    this
}

unsafe extern "win64" fn locinfo_dtor(this: P) {
    let old: *const u16 = rd(this, LINFO_OLD);
    if old.is_null() {
        super::crt_runtime::wsetlocale(0, [0u16].as_ptr());
    } else {
        super::crt_runtime::wsetlocale(0, old);
    }
    for off in [8, 24, 40, 56, LINFO_OLD, LINFO_NEW] {
        yarn_free(this.add(off));
    }
    lockit_dtor(this);
}

unsafe extern "win64" fn locinfo_getname(this: P) -> *const u8 {
    yarn_cstr(this.add(LINFO_NEW))
}

/// `_Collvec` de la locale « C » : page 0 (CP_ACP), pas de nom.
unsafe extern "win64" fn locinfo_getcoll(_this: P, out: P) -> P {
    wr(out, 0, 0u32);
    wr(out, 8, std::ptr::null_mut::<u16>());
    out
}

// ---------- ctype<char> ----------

const CT_TABLE: usize = 24;
const CT_DELFL: usize = 32;
const CT_NAME: usize = 40;

fn classic_table() -> *const u16 {
    super::crt_runtime::pctype_func()
}

unsafe fn new_ctype_c() -> P {
    let p = alloc_zeroed(48);
    wr(p, 0, tables().ctype_c.as_ptr());
    wr(p, CT_TABLE, classic_table());
    p
}

unsafe fn ctype_tidy(this: P, wide: bool) {
    let del: i32 = rd(this, CT_DELFL);
    if del != 0 && (wide || del > 0 || del < 0) {
        free(rd::<P>(this, CT_TABLE));
    }
    free(rd::<P>(this, CT_NAME));
}

unsafe extern "win64" fn ctype_c_deleting(this: P, flags: u32) -> P {
    ctype_tidy(this, false);
    facet_deleting(this, flags)
}

unsafe extern "win64" fn ctype_c_do_tolower(_this: P, c: u8) -> u8 {
    c.to_ascii_lowercase()
}
unsafe extern "win64" fn ctype_c_do_tolower_r(_this: P, first: P, last: *const u8) -> *const u8 {
    let mut p = first;
    while (p as *const u8) < last {
        *p = (*p).to_ascii_lowercase();
        p = p.add(1);
    }
    last
}
unsafe extern "win64" fn ctype_c_do_toupper(_this: P, c: u8) -> u8 {
    c.to_ascii_uppercase()
}
unsafe extern "win64" fn ctype_c_do_toupper_r(_this: P, first: P, last: *const u8) -> *const u8 {
    let mut p = first;
    while (p as *const u8) < last {
        *p = (*p).to_ascii_uppercase();
        p = p.add(1);
    }
    last
}
unsafe extern "win64" fn ctype_c_do_widen(_this: P, c: u8) -> u8 {
    c
}
unsafe extern "win64" fn ctype_c_do_widen_r(
    _this: P,
    first: *const u8,
    last: *const u8,
    dest: P,
) -> *const u8 {
    std::ptr::copy(first, dest, last.offset_from(first).max(0) as usize);
    last
}
unsafe extern "win64" fn ctype_c_do_narrow(_this: P, c: u8, _dflt: u8) -> u8 {
    c
}
unsafe extern "win64" fn ctype_c_do_narrow_r(
    _this: P,
    first: *const u8,
    last: *const u8,
    _dflt: u8,
    dest: P,
) -> *const u8 {
    std::ptr::copy(first, dest, last.offset_from(first).max(0) as usize);
    last
}

// Fonctions publiques (non virtuelles) : appel de la virtuelle `do_*`.
unsafe extern "win64" fn ctype_c_tolower(this: P, c: u8) -> u8 {
    let f: unsafe extern "win64" fn(P, u8) -> u8 = std::mem::transmute(slot(this, 4));
    f(this, c)
}
unsafe extern "win64" fn ctype_c_tolower_r(this: P, first: P, last: *const u8) -> *const u8 {
    let f: unsafe extern "win64" fn(P, P, *const u8) -> *const u8 =
        std::mem::transmute(slot(this, 3));
    f(this, first, last)
}
unsafe extern "win64" fn ctype_c_toupper(this: P, c: u8) -> u8 {
    let f: unsafe extern "win64" fn(P, u8) -> u8 = std::mem::transmute(slot(this, 6));
    f(this, c)
}
unsafe extern "win64" fn ctype_c_widen(this: P, c: u8) -> u8 {
    let f: unsafe extern "win64" fn(P, u8) -> u8 = std::mem::transmute(slot(this, 8));
    f(this, c)
}

unsafe extern "win64" fn getcat_ctype_c(ppf: *mut P, _ploc: *const usize) -> usize {
    if !ppf.is_null() && (*ppf).is_null() {
        *ppf = new_ctype_c();
    }
    2
}

// ---------- ctype<wchar_t> ----------

unsafe fn new_ctype_w() -> P {
    let p = alloc_zeroed(96);
    wr(p, 0, tables().ctype_w.as_ptr());
    wr(p, CT_TABLE, classic_table());
    // _Cvtvec de la locale « C » : page 0, un octet par caractère.
    wr(p, 52, 1u32);
    wr(p, 56, 1i32);
    p
}

fn wlower(c: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&c) {
        c + 32
    } else {
        c
    }
}
fn wupper(c: u16) -> u16 {
    if (b'a' as u16..=b'z' as u16).contains(&c) {
        c - 32
    } else {
        c
    }
}
fn wmask(c: u16) -> i16 {
    if c < 256 {
        unsafe { *classic_table().add(c as usize) as i16 }
    } else {
        0
    }
}

unsafe extern "win64" fn ctype_w_deleting(this: P, flags: u32) -> P {
    ctype_tidy(this, true);
    facet_deleting(this, flags)
}
unsafe extern "win64" fn ctype_w_do_is_r(
    _this: P,
    first: *const u16,
    last: *const u16,
    dest: *mut i16,
) -> *const u16 {
    let mut p = first;
    let mut d = dest;
    while p < last {
        *d = wmask(*p);
        p = p.add(1);
        d = d.add(1);
    }
    p
}
unsafe extern "win64" fn ctype_w_do_is(_this: P, mask: i16, c: u16) -> bool {
    wmask(c) & mask != 0
}
unsafe extern "win64" fn ctype_w_do_scan_is(
    _this: P,
    mask: i16,
    first: *const u16,
    last: *const u16,
) -> *const u16 {
    let mut p = first;
    while p < last && wmask(*p) & mask == 0 {
        p = p.add(1);
    }
    p
}
unsafe extern "win64" fn ctype_w_do_scan_not(
    _this: P,
    mask: i16,
    first: *const u16,
    last: *const u16,
) -> *const u16 {
    let mut p = first;
    while p < last && wmask(*p) & mask != 0 {
        p = p.add(1);
    }
    p
}
unsafe extern "win64" fn ctype_w_do_tolower_r(
    _this: P,
    first: *mut u16,
    last: *const u16,
) -> *const u16 {
    let mut p = first;
    while (p as *const u16) < last {
        *p = wlower(*p);
        p = p.add(1);
    }
    last
}
unsafe extern "win64" fn ctype_w_do_tolower(_this: P, c: u16) -> u16 {
    wlower(c)
}
unsafe extern "win64" fn ctype_w_do_toupper_r(
    _this: P,
    first: *mut u16,
    last: *const u16,
) -> *const u16 {
    let mut p = first;
    while (p as *const u16) < last {
        *p = wupper(*p);
        p = p.add(1);
    }
    last
}
unsafe extern "win64" fn ctype_w_do_toupper(_this: P, c: u16) -> u16 {
    wupper(c)
}
unsafe extern "win64" fn ctype_w_do_widen_r(
    _this: P,
    first: *const u8,
    last: *const u8,
    dest: *mut u16,
) -> *const u8 {
    let mut p = first;
    let mut d = dest;
    while p < last {
        *d = *p as u16;
        p = p.add(1);
        d = d.add(1);
    }
    last
}
unsafe extern "win64" fn ctype_w_do_widen(_this: P, c: u8) -> u16 {
    c as u16
}
unsafe extern "win64" fn ctype_w_do_narrow_r(
    _this: P,
    first: *const u16,
    last: *const u16,
    dflt: u8,
    dest: P,
) -> *const u16 {
    let mut p = first;
    let mut d = dest;
    while p < last {
        *d = if *p < 256 { *p as u8 } else { dflt };
        p = p.add(1);
        d = d.add(1);
    }
    last
}
unsafe extern "win64" fn ctype_w_do_narrow(_this: P, c: u16, dflt: u8) -> u8 {
    if c < 256 {
        c as u8
    } else {
        dflt
    }
}

unsafe extern "win64" fn ctype_w_tolower_r(
    this: P,
    first: *mut u16,
    last: *const u16,
) -> *const u16 {
    let f: unsafe extern "win64" fn(P, *mut u16, *const u16) -> *const u16 =
        std::mem::transmute(slot(this, 7));
    f(this, first, last)
}
unsafe extern "win64" fn ctype_w_tolower(this: P, c: u16) -> u16 {
    let f: unsafe extern "win64" fn(P, u16) -> u16 = std::mem::transmute(slot(this, 8));
    f(this, c)
}
unsafe extern "win64" fn ctype_w_toupper_r(
    this: P,
    first: *mut u16,
    last: *const u16,
) -> *const u16 {
    let f: unsafe extern "win64" fn(P, *mut u16, *const u16) -> *const u16 =
        std::mem::transmute(slot(this, 9));
    f(this, first, last)
}
unsafe extern "win64" fn ctype_w_toupper(this: P, c: u16) -> u16 {
    let f: unsafe extern "win64" fn(P, u16) -> u16 = std::mem::transmute(slot(this, 10));
    f(this, c)
}
unsafe extern "win64" fn ctype_w_widen(this: P, c: u8) -> u16 {
    let f: unsafe extern "win64" fn(P, u8) -> u16 = std::mem::transmute(slot(this, 12));
    f(this, c)
}

unsafe extern "win64" fn getcat_ctype_w(ppf: *mut P, _ploc: *const usize) -> usize {
    if !ppf.is_null() && (*ppf).is_null() {
        *ppf = new_ctype_w();
    }
    2
}

// ---------- codecvt<char, char, mbstate_t> ----------

const CV_OK: i32 = 0;
const CV_PARTIAL: i32 = 1;
const CV_ERROR: i32 = 2;
const CV_NOCONV: i32 = 3;

unsafe fn new_codecvt_c() -> P {
    let p = alloc_zeroed(16);
    wr(p, 0, tables().codecvt_c.as_ptr());
    p
}

unsafe extern "win64" fn cvt_true(_this: P) -> bool {
    true
}
unsafe extern "win64" fn cvt_false(_this: P) -> bool {
    false
}
unsafe extern "win64" fn cvt_one(_this: P) -> i32 {
    1
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn cvt_c_noconv(
    _this: P,
    _state: P,
    first1: *const u8,
    _last1: *const u8,
    mid1: *mut *const u8,
    first2: P,
    _last2: P,
    mid2: *mut P,
) -> i32 {
    *mid1 = first1;
    *mid2 = first2;
    CV_NOCONV
}
unsafe extern "win64" fn cvt_c_unshift(
    _this: P,
    _state: P,
    first2: P,
    _last2: P,
    mid2: *mut P,
) -> i32 {
    *mid2 = first2;
    CV_NOCONV
}
unsafe extern "win64" fn cvt_length(
    _this: P,
    _state: P,
    first1: *const u8,
    last1: *const u8,
    count: usize,
) -> i32 {
    let dist = (last1.offset_from(first1).max(0) as usize).min(i32::MAX as usize);
    count.min(dist) as i32
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn cvt_c_in(
    this: P,
    state: P,
    first1: *const u8,
    last1: *const u8,
    mid1: *mut *const u8,
    first2: P,
    last2: P,
    mid2: *mut P,
) -> i32 {
    type F =
        unsafe extern "win64" fn(P, P, *const u8, *const u8, *mut *const u8, P, P, *mut P) -> i32;
    let f: F = std::mem::transmute(slot(this, 6));
    f(this, state, first1, last1, mid1, first2, last2, mid2)
}
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn cvt_c_out(
    this: P,
    state: P,
    first1: *const u8,
    last1: *const u8,
    mid1: *mut *const u8,
    first2: P,
    last2: P,
    mid2: *mut P,
) -> i32 {
    type F =
        unsafe extern "win64" fn(P, P, *const u8, *const u8, *mut *const u8, P, P, *mut P) -> i32;
    let f: F = std::mem::transmute(slot(this, 7));
    f(this, state, first1, last1, mid1, first2, last2, mid2)
}
unsafe extern "win64" fn cvt_c_unshift_pub(
    this: P,
    state: P,
    first2: P,
    last2: P,
    mid2: *mut P,
) -> i32 {
    let f: unsafe extern "win64" fn(P, P, P, P, *mut P) -> i32 = std::mem::transmute(slot(this, 8));
    f(this, state, first2, last2, mid2)
}
unsafe extern "win64" fn always_noconv(this: P) -> bool {
    let f: unsafe extern "win64" fn(P) -> bool = std::mem::transmute(slot(this, 3));
    f(this)
}

unsafe extern "win64" fn getcat_codecvt_c(ppf: *mut P, _ploc: *const usize) -> usize {
    if !ppf.is_null() && (*ppf).is_null() {
        *ppf = new_codecvt_c();
    }
    2
}

// ---------- codecvt<wchar_t, char, mbstate_t> ----------

unsafe fn new_codecvt_w() -> P {
    let p = alloc_zeroed(64);
    codecvt_w_ctor(p, 0)
}

unsafe extern "win64" fn codecvt_w_ctor(this: P, initrefs: usize) -> P {
    std::ptr::write_bytes(this, 0, 64);
    wr(this, 0, tables().codecvt_w.as_ptr());
    wr(this, REFS, initrefs as u32);
    // _Cvtvec de la locale « C ».
    wr(this, 20, 1u32);
    wr(this, 24, 1i32);
    this
}

unsafe extern "win64" fn codecvt_w_dtor(_this: P) {}

unsafe extern "win64" fn codecvt_w_deleting(this: P, flags: u32) -> P {
    facet_deleting(this, flags)
}

unsafe extern "win64" fn cvt_w_max_length(this: P) -> i32 {
    rd::<u32>(this, 20) as i32
}
unsafe extern "win64" fn cvt_w_encoding(this: P) -> i32 {
    (rd::<u32>(this, 20) == 1) as i32
}

/// Octets → caractères larges, locale « C » : un octet par caractère.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn cvt_w_do_in(
    _this: P,
    _state: P,
    first1: *const u8,
    last1: *const u8,
    mid1: *mut *const u8,
    first2: *mut u16,
    last2: *mut u16,
    mid2: *mut *mut u16,
) -> i32 {
    *mid1 = first1;
    *mid2 = first2;
    loop {
        if *mid1 == last1 {
            return CV_OK;
        }
        if *mid2 == last2 {
            return CV_PARTIAL;
        }
        **mid2 = **mid1 as u16;
        *mid1 = (*mid1).add(1);
        *mid2 = (*mid2).add(1);
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn cvt_w_do_out(
    _this: P,
    _state: P,
    first1: *const u16,
    last1: *const u16,
    mid1: *mut *const u16,
    first2: P,
    last2: P,
    mid2: *mut P,
) -> i32 {
    *mid1 = first1;
    *mid2 = first2;
    while *mid1 != last1 && *mid2 != last2 {
        let c = **mid1;
        if c > 255 {
            return CV_ERROR;
        }
        **mid2 = c as u8;
        *mid1 = (*mid1).add(1);
        *mid2 = (*mid2).add(1);
    }
    if *mid1 == last1 {
        CV_OK
    } else {
        CV_PARTIAL
    }
}

unsafe extern "win64" fn cvt_w_do_unshift(
    _this: P,
    _state: P,
    first2: P,
    _last2: P,
    mid2: *mut P,
) -> i32 {
    *mid2 = first2;
    CV_OK
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn cvt_w_in(
    this: P,
    state: P,
    first1: *const u8,
    last1: *const u8,
    mid1: *mut *const u8,
    first2: *mut u16,
    last2: *mut u16,
    mid2: *mut *mut u16,
) -> i32 {
    type F = unsafe extern "win64" fn(
        P,
        P,
        *const u8,
        *const u8,
        *mut *const u8,
        *mut u16,
        *mut u16,
        *mut *mut u16,
    ) -> i32;
    let f: F = std::mem::transmute(slot(this, 6));
    f(this, state, first1, last1, mid1, first2, last2, mid2)
}

// ---------- time_put<char, ostreambuf_iterator<char>> ----------

/// `ostreambuf_iterator<C>` : { `_Failed`, `_Strbuf` }.
#[repr(C)]
#[derive(Clone, Copy)]
struct Obi {
    failed: bool,
    sb: P,
}

unsafe fn obi_put<C: Ch>(it: &mut Obi, c: C) {
    if !it.failed && C::is_eof(sb_sputc::<C>(it.sb, c)) {
        it.failed = true;
    }
}

unsafe fn new_time_put_c() -> P {
    let p = alloc_zeroed(24);
    wr(p, 0, tables().time_put.as_ptr());
    p
}

unsafe extern "win64" fn time_put_deleting(this: P, flags: u32) -> P {
    free(rd::<P>(this, 16));
    facet_deleting(this, flags)
}

const STRFTIME_SPECIFIERS: &[u8] = b"aAbBcCdDeFgGhHIjmMnprRStTuUVwWxXyYzZ";

/// Un champ de `strftime`, rendu comme la CRT de Microsoft en locale « C ».
fn strftime_field(tm: &[i32; 9], spec: u8, modifier: u8) -> Vec<u8> {
    let fmt: Vec<u8> = match (modifier, spec) {
        (b'#', b'c') => b"%A, %B %d, %Y %H:%M:%S".to_vec(),
        (b'#', b'x') => b"%A, %B %d, %Y".to_vec(),
        (_, b'c') => b"%m/%d/%y %H:%M:%S".to_vec(),
        (_, s) => vec![b'%', s],
    };
    let mut t: libc::tm = unsafe { std::mem::zeroed() };
    t.tm_sec = tm[0];
    t.tm_min = tm[1];
    t.tm_hour = tm[2];
    t.tm_mday = tm[3];
    t.tm_mon = tm[4];
    t.tm_year = tm[5];
    t.tm_wday = tm[6];
    t.tm_yday = tm[7];
    t.tm_isdst = tm[8];
    let mut cfmt = fmt;
    cfmt.push(0);
    let mut buf = [0u8; 256];
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            cfmt.as_ptr() as *const libc::c_char,
            &t,
        )
    };
    let mut out = buf[..n].to_vec();
    // « # » devant un champ numérique : sans les zéros de tête (CRT MSVC).
    if modifier == b'#' && b"dHIjmMSUwWyY".contains(&spec) {
        while out.len() > 1 && out[0] == b'0' {
            out.remove(0);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn time_put_do_put(
    _this: P,
    out: *mut Obi,
    dest: *const Obi,
    _ios: P,
    _fill: u8,
    tm: *const [i32; 9],
    spec: u8,
    modifier: u8,
) -> *mut Obi {
    let mut it = *dest;
    for b in strftime_field(&*tm, spec, modifier) {
        obi_put::<u8>(&mut it, b);
    }
    *out = it;
    out
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn time_put_put(
    this: P,
    out: *mut Obi,
    dest: *const Obi,
    ios: P,
    fill: u8,
    tm: *const [i32; 9],
    first: *const u8,
    last: *const u8,
) -> *mut Obi {
    type DoPut = unsafe extern "win64" fn(
        P,
        *mut Obi,
        *const Obi,
        P,
        u8,
        *const [i32; 9],
        u8,
        u8,
    ) -> *mut Obi;
    let do_put: DoPut = std::mem::transmute(slot(this, 3));
    let mut it = *dest;
    let mut p = first;
    while p < last {
        let c = *p;
        if c != b'%' {
            obi_put::<u8>(&mut it, c);
            p = p.add(1);
            continue;
        }
        p = p.add(1);
        if p == last {
            obi_put::<u8>(&mut it, b'%');
            break;
        }
        let mut spec = *p;
        let mut modifier = 0u8;
        if matches!(spec, b'E' | b'O' | b'Q' | b'#') {
            p = p.add(1);
            if p == last {
                obi_put::<u8>(&mut it, b'%');
                obi_put::<u8>(&mut it, spec);
                break;
            }
            modifier = spec;
            spec = *p;
        }
        if spec == b'%' && modifier == 0 {
            obi_put::<u8>(&mut it, b'%');
        } else if !STRFTIME_SPECIFIERS.contains(&spec) {
            obi_put::<u8>(&mut it, b'%');
            if modifier != 0 {
                obi_put::<u8>(&mut it, modifier);
            }
            obi_put::<u8>(&mut it, spec);
        } else {
            let mut next = Obi {
                failed: false,
                sb: std::ptr::null_mut(),
            };
            do_put(this, &mut next, &it, ios, fill, tm, spec, modifier);
            it = next;
        }
        p = p.add(1);
    }
    *out = it;
    out
}

unsafe extern "win64" fn getcat_time_put_c(ppf: *mut P, _ploc: *const usize) -> usize {
    if !ppf.is_null() && (*ppf).is_null() {
        *ppf = new_time_put_c();
    }
    5
}

// ---------- caractères ----------

/// `char` ou `wchar_t`, avec le `int_type` de leur `char_traits`.
pub trait Ch: Copy + PartialEq + Default + 'static {
    type I: Copy + PartialEq;
    fn eof() -> Self::I;
    fn to_int(self) -> Self::I;
    fn from_int(i: Self::I) -> Self;
    fn widen(b: u8) -> Self;
    fn code(self) -> u32;
    fn is_eof(i: Self::I) -> bool {
        i == Self::eof()
    }
}

impl Ch for u8 {
    type I = i32;
    fn eof() -> i32 {
        -1
    }
    fn to_int(self) -> i32 {
        self as i32
    }
    fn from_int(i: i32) -> u8 {
        i as u8
    }
    fn widen(b: u8) -> u8 {
        b
    }
    fn code(self) -> u32 {
        self as u32
    }
}

impl Ch for u16 {
    type I = u16;
    fn eof() -> u16 {
        0xFFFF
    }
    fn to_int(self) -> u16 {
        self
    }
    fn from_int(i: u16) -> u16 {
        i
    }
    fn widen(b: u8) -> u16 {
        b as u16
    }
    fn code(self) -> u32 {
        self as u32
    }
}

fn is_space(c: u32) -> bool {
    matches!(c, 0x20 | 0x09..=0x0d)
}

// ---------- basic_streambuf ----------

const SB_GFIRST: usize = 8;
const SB_PFIRST: usize = 16;
const SB_IGFIRST: usize = 24;
const SB_IPFIRST: usize = 32;
const SB_GNEXT: usize = 40;
const SB_PNEXT: usize = 48;
const SB_IGNEXT: usize = 56;
const SB_IPNEXT: usize = 64;
const SB_GCOUNT: usize = 72;
const SB_PCOUNT: usize = 76;
const SB_IGCOUNT: usize = 80;
const SB_IPCOUNT: usize = 88;
const SB_PLOCALE: usize = 96;
const SB_SIZE: usize = 104;

/// fpos<_Mbstatet> : position, position C (inutilisée), état.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Fpos {
    off: i64,
    fpos: i64,
    state: u64,
}

impl Fpos {
    fn at(off: i64) -> Fpos {
        Fpos {
            off,
            fpos: 0,
            state: 0,
        }
    }
    fn offset(&self) -> i64 {
        self.off.wrapping_add(self.fpos)
    }
}

unsafe fn pp<C>(sb: P, off: usize) -> *mut *mut C {
    rd::<*mut *mut C>(sb, off)
}
unsafe fn pc(sb: P, off: usize) -> *mut i32 {
    rd::<*mut i32>(sb, off)
}

unsafe extern "win64" fn sb_eback<C: Ch>(sb: P) -> *mut C {
    *pp::<C>(sb, SB_IGFIRST)
}
unsafe extern "win64" fn sb_gptr<C: Ch>(sb: P) -> *mut C {
    *pp::<C>(sb, SB_IGNEXT)
}
unsafe extern "win64" fn sb_egptr<C: Ch>(sb: P) -> *mut C {
    (*pp::<C>(sb, SB_IGNEXT)).wrapping_offset(*pc(sb, SB_IGCOUNT) as isize)
}
unsafe extern "win64" fn sb_pbase<C: Ch>(sb: P) -> *mut C {
    *pp::<C>(sb, SB_IPFIRST)
}
unsafe extern "win64" fn sb_pptr<C: Ch>(sb: P) -> *mut C {
    *pp::<C>(sb, SB_IPNEXT)
}
unsafe extern "win64" fn sb_epptr<C: Ch>(sb: P) -> *mut C {
    (*pp::<C>(sb, SB_IPNEXT)).wrapping_offset(*pc(sb, SB_IPCOUNT) as isize)
}
unsafe extern "win64" fn sb_gbump<C: Ch>(sb: P, n: i32) {
    *pc(sb, SB_IGCOUNT) -= n;
    let g = pp::<C>(sb, SB_IGNEXT);
    *g = (*g).wrapping_offset(n as isize);
}
unsafe extern "win64" fn sb_pbump<C: Ch>(sb: P, n: i32) {
    *pc(sb, SB_IPCOUNT) -= n;
    let g = pp::<C>(sb, SB_IPNEXT);
    *g = (*g).wrapping_offset(n as isize);
}
unsafe extern "win64" fn sb_setg<C: Ch>(sb: P, first: *mut C, next: *mut C, last: *mut C) {
    *pp::<C>(sb, SB_IGFIRST) = first;
    *pp::<C>(sb, SB_IGNEXT) = next;
    *pc(sb, SB_IGCOUNT) = (last as isize - next as isize) as i32 / std::mem::size_of::<C>() as i32;
}
unsafe extern "win64" fn sb_setp3<C: Ch>(sb: P, first: *mut C, next: *mut C, last: *mut C) {
    *pp::<C>(sb, SB_IPFIRST) = first;
    *pp::<C>(sb, SB_IPNEXT) = next;
    *pc(sb, SB_IPCOUNT) = (last as isize - next as isize) as i32 / std::mem::size_of::<C>() as i32;
}
unsafe extern "win64" fn sb_setp2<C: Ch>(sb: P, first: *mut C, last: *mut C) {
    sb_setp3::<C>(sb, first, first, last)
}
unsafe extern "win64" fn sb_gnavail<C: Ch>(sb: P) -> i64 {
    if (*pp::<C>(sb, SB_IGNEXT)).is_null() {
        0
    } else {
        *pc(sb, SB_IGCOUNT) as i64
    }
}
unsafe extern "win64" fn sb_pnavail<C: Ch>(sb: P) -> i64 {
    if (*pp::<C>(sb, SB_IPNEXT)).is_null() {
        0
    } else {
        *pc(sb, SB_IPCOUNT) as i64
    }
}
unsafe extern "win64" fn sb_gndec<C: Ch>(sb: P) -> *mut C {
    *pc(sb, SB_IGCOUNT) += 1;
    let g = pp::<C>(sb, SB_IGNEXT);
    *g = (*g).wrapping_sub(1);
    *g
}
unsafe extern "win64" fn sb_gninc<C: Ch>(sb: P) -> *mut C {
    *pc(sb, SB_IGCOUNT) -= 1;
    let g = pp::<C>(sb, SB_IGNEXT);
    let old = *g;
    *g = old.wrapping_add(1);
    old
}
unsafe fn sb_gnpreinc<C: Ch>(sb: P) -> *mut C {
    *pc(sb, SB_IGCOUNT) -= 1;
    let g = pp::<C>(sb, SB_IGNEXT);
    *g = (*g).wrapping_add(1);
    *g
}
unsafe extern "win64" fn sb_pninc<C: Ch>(sb: P) -> *mut C {
    *pc(sb, SB_IPCOUNT) -= 1;
    let g = pp::<C>(sb, SB_IPNEXT);
    let old = *g;
    *g = old.wrapping_add(1);
    old
}

/// `_Init()` : les pointeurs indirects visent les champs de l'objet.
unsafe extern "win64" fn sb_init0<C: Ch>(sb: P) {
    wr(sb, SB_IGFIRST, sb.add(SB_GFIRST));
    wr(sb, SB_IPFIRST, sb.add(SB_PFIRST));
    wr(sb, SB_IGNEXT, sb.add(SB_GNEXT));
    wr(sb, SB_IPNEXT, sb.add(SB_PNEXT));
    wr(sb, SB_IGCOUNT, sb.add(SB_GCOUNT));
    wr(sb, SB_IPCOUNT, sb.add(SB_PCOUNT));
    sb_setp2::<C>(sb, std::ptr::null_mut(), std::ptr::null_mut());
    sb_setg::<C>(
        sb,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn sb_init6<C: Ch>(sb: P, gf: P, gn: P, gc: P, pf: P, pn: P, pcount: P) {
    wr(sb, SB_IGFIRST, gf);
    wr(sb, SB_IPFIRST, pf);
    wr(sb, SB_IGNEXT, gn);
    wr(sb, SB_IPNEXT, pn);
    wr(sb, SB_IGCOUNT, gc);
    wr(sb, SB_IPCOUNT, pcount);
}

unsafe extern "win64" fn sb_ctor<C: Ch>(sb: P) -> P {
    std::ptr::write_bytes(sb.add(8), 0, SB_SIZE - 8);
    wr(sb, 0, tables().sb_vt::<C>());
    wr(sb, SB_PLOCALE, new_locale());
    sb_init0::<C>(sb);
    sb
}

unsafe extern "win64" fn sb_dtor<C: Ch>(sb: P) {
    delete_locale(rd::<*mut usize>(sb, SB_PLOCALE));
    wr(sb, SB_PLOCALE, std::ptr::null_mut::<usize>());
}

unsafe extern "win64" fn sb_deleting<C: Ch>(sb: P, flags: u32) -> P {
    sb_dtor::<C>(sb);
    if flags & 1 != 0 {
        free(sb);
    }
    sb
}

// Virtuelles par défaut (exportées : les classes dérivées les reprennent).
unsafe extern "win64" fn sb_noop(_sb: P) {}
unsafe extern "win64" fn sb_eof<C: Ch>(_sb: P, _c: C::I) -> C::I {
    C::eof()
}
unsafe extern "win64" fn sb_underflow<C: Ch>(_sb: P) -> C::I {
    C::eof()
}
unsafe extern "win64" fn sb_showmanyc(_sb: P) -> i64 {
    0
}
unsafe extern "win64" fn sb_uflow<C: Ch>(sb: P) -> C::I {
    let under: unsafe extern "win64" fn(P) -> C::I = std::mem::transmute(slot(sb, 6));
    if C::is_eof(under(sb)) {
        C::eof()
    } else {
        (*sb_gninc::<C>(sb)).to_int()
    }
}
unsafe extern "win64" fn sb_xsgetn<C: Ch>(sb: P, dst: *mut C, count: i64) -> i64 {
    let mut left = count;
    let mut d = dst;
    while left > 0 {
        let avail = sb_gnavail::<C>(sb);
        if avail > 0 {
            let n = avail.min(left);
            std::ptr::copy(sb_gptr::<C>(sb), d, n as usize);
            d = d.add(n as usize);
            left -= n;
            sb_gbump::<C>(sb, n as i32);
        } else {
            let uflow: unsafe extern "win64" fn(P) -> C::I = std::mem::transmute(slot(sb, 7));
            let meta = uflow(sb);
            if C::is_eof(meta) {
                break;
            }
            *d = C::from_int(meta);
            d = d.add(1);
            left -= 1;
        }
    }
    count - left
}
unsafe extern "win64" fn sb_xsputn<C: Ch>(sb: P, src: *const C, count: i64) -> i64 {
    let mut left = count;
    let mut s = src;
    while left > 0 {
        let avail = sb_pnavail::<C>(sb);
        if avail > 0 {
            let n = avail.min(left);
            std::ptr::copy(s, sb_pptr::<C>(sb), n as usize);
            s = s.add(n as usize);
            left -= n;
            sb_pbump::<C>(sb, n as i32);
        } else {
            let overflow: unsafe extern "win64" fn(P, C::I) -> C::I =
                std::mem::transmute(slot(sb, 3));
            if C::is_eof(overflow(sb, (*s).to_int())) {
                break;
            }
            s = s.add(1);
            left -= 1;
        }
    }
    count - left
}
unsafe extern "win64" fn sb_seekoff(
    _sb: P,
    out: *mut Fpos,
    _off: i64,
    _way: i32,
    _mode: i32,
) -> *mut Fpos {
    *out = Fpos::at(-1);
    out
}
unsafe extern "win64" fn sb_seekpos(
    _sb: P,
    out: *mut Fpos,
    _pos: *const Fpos,
    _mode: i32,
) -> *mut Fpos {
    *out = Fpos::at(-1);
    out
}
unsafe extern "win64" fn sb_setbuf<C: Ch>(sb: P, _p: *mut C, _n: i64) -> P {
    sb
}
unsafe extern "win64" fn sb_sync(_sb: P) -> i32 {
    0
}
unsafe extern "win64" fn sb_imbue(_sb: P, _loc: *const usize) {}

// Fonctions publiques : la logique en ligne de l'en-tête.
unsafe extern "win64" fn sb_sgetc<C: Ch>(sb: P) -> C::I {
    if sb_gnavail::<C>(sb) > 0 {
        (*sb_gptr::<C>(sb)).to_int()
    } else {
        let under: unsafe extern "win64" fn(P) -> C::I = std::mem::transmute(slot(sb, 6));
        under(sb)
    }
}
unsafe extern "win64" fn sb_sbumpc<C: Ch>(sb: P) -> C::I {
    if sb_gnavail::<C>(sb) > 0 {
        (*sb_gninc::<C>(sb)).to_int()
    } else {
        let uflow: unsafe extern "win64" fn(P) -> C::I = std::mem::transmute(slot(sb, 7));
        uflow(sb)
    }
}
unsafe extern "win64" fn sb_snextc<C: Ch>(sb: P) -> C::I {
    if sb_gnavail::<C>(sb) > 1 {
        (*sb_gnpreinc::<C>(sb)).to_int()
    } else if C::is_eof(sb_sbumpc::<C>(sb)) {
        C::eof()
    } else {
        sb_sgetc::<C>(sb)
    }
}
unsafe extern "win64" fn sb_sputc<C: Ch>(sb: P, c: C) -> C::I {
    if sb_pnavail::<C>(sb) > 0 {
        let p = sb_pninc::<C>(sb);
        *p = c;
        c.to_int()
    } else {
        let overflow: unsafe extern "win64" fn(P, C::I) -> C::I = std::mem::transmute(slot(sb, 3));
        overflow(sb, c.to_int())
    }
}
unsafe extern "win64" fn sb_sputn<C: Ch>(sb: P, s: *const C, n: i64) -> i64 {
    let f: unsafe extern "win64" fn(P, *const C, i64) -> i64 = std::mem::transmute(slot(sb, 9));
    f(sb, s, n)
}
unsafe fn sb_sgetn<C: Ch>(sb: P, s: *mut C, n: i64) -> i64 {
    let f: unsafe extern "win64" fn(P, *mut C, i64) -> i64 = std::mem::transmute(slot(sb, 8));
    f(sb, s, n)
}
unsafe fn sb_pubsync(sb: P) -> i32 {
    let f: unsafe extern "win64" fn(P) -> i32 = std::mem::transmute(slot(sb, 13));
    f(sb)
}
unsafe fn sb_lock(sb: P, unlock_it: bool) {
    if !sb.is_null() {
        let f: unsafe extern "win64" fn(P) =
            std::mem::transmute(slot(sb, if unlock_it { 2 } else { 1 }));
        f(sb)
    }
}
unsafe fn sb_pubseekoff(sb: P, off: i64, way: i32, mode: i32) -> Fpos {
    let f: unsafe extern "win64" fn(P, *mut Fpos, i64, i32, i32) -> *mut Fpos =
        std::mem::transmute(slot(sb, 10));
    let mut r = Fpos::at(-1);
    f(sb, &mut r, off, way, mode);
    r
}
unsafe fn sb_pubseekpos(sb: P, pos: &Fpos, mode: i32) -> Fpos {
    let f: unsafe extern "win64" fn(P, *mut Fpos, *const Fpos, i32) -> *mut Fpos =
        std::mem::transmute(slot(sb, 11));
    let mut r = Fpos::at(-1);
    let copy = *pos;
    f(sb, &mut r, &copy, mode);
    r
}
unsafe fn sb_pubimbue(sb: P, loc: *const usize) {
    let f: unsafe extern "win64" fn(P, *const usize) = std::mem::transmute(slot(sb, 14));
    f(sb, loc);
    let pl: *mut usize = rd(sb, SB_PLOCALE);
    if !pl.is_null() {
        locale_assign(pl, loc);
    }
}
unsafe extern "win64" fn sb_getloc(sb: P, out: *mut usize) -> *mut usize {
    locale_copy(out, rd::<*const usize>(sb, SB_PLOCALE));
    out
}

// ---------- ios_base ----------

const IOS_STDSTR: usize = 8;
const IOS_STATE: usize = 16;
const IOS_EXCEPT: usize = 20;
const IOS_FMTFL: usize = 24;
const IOS_PREC: usize = 32;
const IOS_WIDE: usize = 40;
const IOS_ARR: usize = 48;
const IOS_CALLS: usize = 56;
const IOS_PLOC: usize = 64;
const BIOS_STRBUF: usize = 72;
const BIOS_TIE: usize = 80;
const BIOS_FILL: usize = 88;
const BIOS_SIZE: usize = 96;

const GOODBIT: i32 = 0;
const EOFBIT: i32 = 1;
const FAILBIT: i32 = 2;
const BADBIT: i32 = 4;
const STATMASK: i32 = 0x17;
const SKIPWS: i32 = 0x1;
const UNITBUF: i32 = 0x2;
const UPPERCASE: i32 = 0x4;
const SHOWBASE: i32 = 0x8;
const SHOWPOINT: i32 = 0x10;
const SHOWPOS: i32 = 0x20;
const LEFT: i32 = 0x40;
const INTERNAL: i32 = 0x100;
const DEC: i32 = 0x200;
const OCT: i32 = 0x400;
const HEX: i32 = 0x800;
const SCIENTIFIC: i32 = 0x1000;
const FIXED: i32 = 0x2000;
const BOOLALPHA: i32 = 0x4000;
const ADJUSTFIELD: i32 = 0x1c0;
const BASEFIELD: i32 = 0xe00;
const FLOATFIELD: i32 = 0x3000;
const MODE_IN: i32 = 1;
const MODE_OUT: i32 = 2;

unsafe extern "win64" fn ios_flags(ios: P) -> i32 {
    rd(ios, IOS_FMTFL)
}
unsafe extern "win64" fn ios_setf(ios: P, f: i32) -> i32 {
    let old: i32 = rd(ios, IOS_FMTFL);
    wr(ios, IOS_FMTFL, old | (f & 0xffff));
    old
}
unsafe extern "win64" fn ios_setf2(ios: P, f: i32, mask: i32) -> i32 {
    let old: i32 = rd(ios, IOS_FMTFL);
    wr(ios, IOS_FMTFL, (old & !mask) | (f & mask & 0xffff));
    old
}
unsafe extern "win64" fn ios_unsetf(ios: P, mask: i32) {
    wr(ios, IOS_FMTFL, rd::<i32>(ios, IOS_FMTFL) & !mask);
}
unsafe extern "win64" fn ios_width(ios: P) -> i64 {
    rd(ios, IOS_WIDE)
}
unsafe extern "win64" fn ios_set_width(ios: P, w: i64) -> i64 {
    let old: i64 = rd(ios, IOS_WIDE);
    wr(ios, IOS_WIDE, w);
    old
}
unsafe extern "win64" fn ios_good(ios: P) -> bool {
    rd::<i32>(ios, IOS_STATE) == GOODBIT
}
unsafe extern "win64" fn ios_eof(ios: P) -> bool {
    rd::<i32>(ios, IOS_STATE) & EOFBIT != 0
}
unsafe extern "win64" fn ios_fail(ios: P) -> bool {
    rd::<i32>(ios, IOS_STATE) & (BADBIT | FAILBIT) != 0
}
unsafe extern "win64" fn ios_bad(ios: P) -> bool {
    rd::<i32>(ios, IOS_STATE) & BADBIT != 0
}
unsafe extern "win64" fn ios_not(ios: P) -> bool {
    ios_fail(ios)
}
unsafe extern "win64" fn ios_to_bool(ios: P) -> bool {
    !ios_fail(ios)
}
unsafe extern "win64" fn ios_getloc(ios: P, out: *mut usize) -> *mut usize {
    locale_copy(out, rd::<*const usize>(ios, IOS_PLOC));
    out
}

static SYNC_WITH_STDIO: AtomicBool = AtomicBool::new(true);

unsafe extern "win64" fn ios_sync_with_stdio(sync: bool) -> bool {
    let _g = Held::new(LOCK_STREAM);
    SYNC_WITH_STDIO.swap(sync, Ordering::AcqRel)
}

/// `ios_base::clear(state, reraise)` : lève `ios_base::failure` (ici :
/// arrêt) si l'état touche le masque d'exceptions.
unsafe fn ios_base_clear(ios: P, state: i32) {
    let state = state & STATMASK;
    wr(ios, IOS_STATE, state);
    let filtered = state & rd::<i32>(ios, IOS_EXCEPT);
    if filtered != 0 {
        let msg: &[u8] = if filtered & BADBIT != 0 {
            b"ios_base::badbit set\0"
        } else if filtered & FAILBIT != 0 {
            b"ios_base::failbit set\0"
        } else {
            b"ios_base::eofbit set\0"
        };
        fatal("ios_base::failure", msg);
    }
}

/// `ios_base::_Callfns` : rappels enregistrés (`register_callback`).
unsafe fn ios_callfns(ios: P, event: i32) {
    let mut node: P = rd(ios, IOS_CALLS);
    while !node.is_null() {
        let f: unsafe extern "win64" fn(i32, P, i32) = std::mem::transmute(rd::<usize>(node, 16));
        f(event, ios, rd::<i32>(node, 8));
        node = rd(node, 0);
    }
}

static STD_STREAMS: [AtomicUsize; 10] = [const { AtomicUsize::new(0) }; 10];
static STD_OPENS: [AtomicI32; 10] = [const { AtomicI32::new(0) }; 10];

unsafe fn ios_addstd(ios: P) {
    let _g = Held::new(LOCK_STREAM);
    let mut i = 1;
    while i < 8 {
        let s = STD_STREAMS[i].load(Ordering::Relaxed);
        if s == 0 || s == ios as usize {
            break;
        }
        i += 1;
    }
    wr(ios, IOS_STDSTR, i);
    STD_STREAMS[i].store(ios as usize, Ordering::Relaxed);
    STD_OPENS[i].fetch_add(1, Ordering::Relaxed);
}

/// `ios_base::_Ios_base_dtor`.
unsafe fn ios_base_dtor(ios: P) {
    let s: usize = rd(ios, IOS_STDSTR);
    if s > 0 && s < STD_OPENS.len() && STD_OPENS[s].fetch_sub(1, Ordering::AcqRel) > 1 {
        return;
    }
    ios_callfns(ios, 0);
    let mut node: P = rd(ios, IOS_ARR);
    while !node.is_null() {
        let next: P = rd(node, 0);
        free(node);
        node = next;
    }
    let mut node: P = rd(ios, IOS_CALLS);
    while !node.is_null() {
        let next: P = rd(node, 0);
        free(node);
        node = next;
    }
    wr(ios, IOS_ARR, std::ptr::null_mut::<u8>());
    wr(ios, IOS_CALLS, std::ptr::null_mut::<u8>());
    delete_locale(rd::<*mut usize>(ios, IOS_PLOC));
    wr(ios, IOS_PLOC, std::ptr::null_mut::<usize>());
}

unsafe extern "win64" fn ios_base_deleting(ios: P, flags: u32) -> P {
    ios_base_dtor(ios);
    if flags & 1 != 0 {
        free(ios);
    }
    ios
}

// ---------- basic_ios ----------

unsafe extern "win64" fn bios_ctor<C: Ch>(ios: P) -> P {
    std::ptr::write_bytes(ios.add(8), 0, BIOS_SIZE - 8);
    wr(ios, 0, tables().bios_vt::<C>());
    ios
}

unsafe extern "win64" fn bios_dtor<C: Ch>(ios: P) {
    ios_base_dtor(ios);
}

unsafe extern "win64" fn bios_clear<C: Ch>(ios: P, state: i32, _reraise: bool) {
    let sb: P = rd(ios, BIOS_STRBUF);
    ios_base_clear(ios, state | if sb.is_null() { BADBIT } else { GOODBIT });
}
unsafe extern "win64" fn bios_setstate<C: Ch>(ios: P, state: i32, reraise: bool) {
    bios_clear::<C>(ios, rd::<i32>(ios, IOS_STATE) | state, reraise);
}
unsafe extern "win64" fn bios_fill<C: Ch>(ios: P) -> C {
    rd(ios, BIOS_FILL)
}
unsafe extern "win64" fn bios_set_fill<C: Ch>(ios: P, c: C) -> C {
    let old: C = rd(ios, BIOS_FILL);
    wr(ios, BIOS_FILL, c);
    old
}
unsafe extern "win64" fn bios_rdbuf<C: Ch>(ios: P) -> P {
    rd(ios, BIOS_STRBUF)
}
unsafe extern "win64" fn bios_tie<C: Ch>(ios: P) -> P {
    rd(ios, BIOS_TIE)
}
unsafe extern "win64" fn bios_widen<C: Ch>(_ios: P, c: u8) -> C {
    C::widen(c)
}
unsafe extern "win64" fn bios_imbue<C: Ch>(
    ios: P,
    out: *mut usize,
    loc: *const usize,
) -> *mut usize {
    let pl: *mut usize = rd(ios, IOS_PLOC);
    locale_copy(out, pl);
    locale_assign(pl, loc);
    ios_callfns(ios, 1);
    let sb: P = rd(ios, BIOS_STRBUF);
    if !sb.is_null() {
        sb_pubimbue(sb, loc);
    }
    out
}

/// `basic_ios::init(strbuf, isstd)` avec `ios_base::_Init`.
unsafe fn bios_init<C: Ch>(ios: P, sb: P, isstd: bool) {
    wr(ios, IOS_PLOC, std::ptr::null_mut::<usize>());
    wr(ios, IOS_STDSTR, 0usize);
    wr(ios, IOS_EXCEPT, GOODBIT);
    wr(ios, IOS_FMTFL, SKIPWS | DEC);
    wr(ios, IOS_PREC, 6i64);
    wr(ios, IOS_WIDE, 0i64);
    wr(ios, IOS_ARR, std::ptr::null_mut::<u8>());
    wr(ios, IOS_CALLS, std::ptr::null_mut::<u8>());
    wr(ios, IOS_STATE, GOODBIT);
    wr(ios, IOS_PLOC, new_locale());
    wr(ios, BIOS_STRBUF, sb);
    wr(ios, BIOS_TIE, std::ptr::null_mut::<u8>());
    wr(ios, BIOS_FILL, C::widen(b' '));
    if sb.is_null() {
        bios_setstate::<C>(ios, BADBIT, false);
    }
    if isstd {
        ios_addstd(ios);
    }
}

unsafe fn ios_setstate<C: Ch>(ios: P, state: i32) {
    bios_setstate::<C>(ios, state, false)
}

// ---------- bases virtuelles ----------

/// Base virtuelle `basic_ios` d'un flux, par sa vbtable.
unsafe fn vbase(obj: P) -> P {
    let vbt: *const i32 = rd(obj, 0);
    obj.offset(*vbt.add(1) as isize)
}

/// Après la construction de la base : vftable du flux et vtordisp
/// (décalage réel moins décalage statique), comme le code MSVC.
unsafe fn set_stream_vft(obj: P, vft: *const usize, static_off: isize) -> P {
    let b = vbase(obj);
    wr(b, 0, vft);
    let actual = b.offset_from(obj);
    wr(b.offset(-4), 0, (actual - static_off) as i32);
    b
}

/// Destructeur « avec suppression » d'un flux, appelé par la vftable de sa
/// base virtuelle : `this` est la base, corrigée par la vtordisp.
unsafe fn stream_deleting(b: P, flags: u32, static_off: isize) -> P {
    let vtordisp = rd::<i32>(b.offset(-4), 0) as isize;
    let complete = b.offset(-vtordisp - static_off);
    ios_base_dtor(b);
    if flags & 1 != 0 {
        free(complete);
    }
    complete
}

const OS_VBOFF: isize = 16;
const IS_VBOFF: isize = 24;
const IOS_VBOFF: isize = 32;
const IS_CHCOUNT: usize = 8;

static OS_VBTABLE: [i32; 2] = [0, OS_VBOFF as i32];
static IS_VBTABLE: [i32; 2] = [0, IS_VBOFF as i32];
static IOS_VBTABLE_I: [i32; 2] = [0, IOS_VBOFF as i32];
static IOS_VBTABLE_O: [i32; 2] = [0, (IOS_VBOFF - 16) as i32];

unsafe extern "win64" fn os_deleting<C: Ch>(b: P, flags: u32) -> P {
    stream_deleting(b, flags, OS_VBOFF)
}
unsafe extern "win64" fn is_deleting<C: Ch>(b: P, flags: u32) -> P {
    stream_deleting(b, flags, IS_VBOFF)
}
unsafe extern "win64" fn iostream_deleting<C: Ch>(b: P, flags: u32) -> P {
    stream_deleting(b, flags, IOS_VBOFF)
}

/// Destructeurs `~basic_istream`, `~basic_ostream`, `~basic_iostream` :
/// corps vide dans la MS STL ; la base virtuelle est détruite à part.
unsafe extern "win64" fn stream_dtor(_b: P) {}

// ---------- basic_ostream ----------

unsafe extern "win64" fn os_ctor<C: Ch>(os: P, sb: P, isstd: bool, most_derived: i32) -> P {
    if most_derived != 0 {
        wr(os, 0, OS_VBTABLE.as_ptr());
        bios_ctor::<C>(os.offset(OS_VBOFF));
    }
    let b = set_stream_vft(os, tables().os_vt::<C>(), OS_VBOFF);
    bios_init::<C>(b, sb, isstd);
    os
}

/// Sentinelle de sortie : verrou du tampon, état, vidage du flux lié.
unsafe fn os_sentry<C: Ch>(os: P) -> bool {
    let b = vbase(os);
    sb_lock(rd(b, BIOS_STRBUF), false);
    if rd::<i32>(b, IOS_STATE) != GOODBIT {
        return false;
    }
    let tie: P = rd(b, BIOS_TIE);
    if !tie.is_null() && tie != os {
        os_flush::<C>(tie);
    }
    rd::<i32>(b, IOS_STATE) == GOODBIT
}

unsafe fn os_sentry_end<C: Ch>(os: P) {
    os_osfx::<C>(os);
    sb_lock(rd(vbase(os), BIOS_STRBUF), true);
}

unsafe extern "win64" fn os_osfx<C: Ch>(os: P) {
    let b = vbase(os);
    if rd::<i32>(b, IOS_STATE) == GOODBIT && rd::<i32>(b, IOS_FMTFL) & UNITBUF != 0 {
        let sb: P = rd(b, BIOS_STRBUF);
        if !sb.is_null() && sb_pubsync(sb) == -1 {
            ios_setstate::<C>(b, BADBIT);
        }
    }
}

unsafe extern "win64" fn os_flush<C: Ch>(os: P) -> P {
    let b = vbase(os);
    let sb: P = rd(b, BIOS_STRBUF);
    if !sb.is_null() {
        let ok = os_sentry::<C>(os);
        if ok && sb_pubsync(sb) == -1 {
            ios_setstate::<C>(b, BADBIT);
        }
        os_sentry_end::<C>(os);
    }
    os
}

/// Écrit `text` (ASCII) avec la largeur, le remplissage et l'alignement du
/// flux, puis remet la largeur à zéro (`num_put`).
unsafe fn put_padded<C: Ch>(b: P, text: &[u8], prefix: usize) -> bool {
    let sb: P = rd(b, BIOS_STRBUF);
    let width: i64 = rd(b, IOS_WIDE);
    let flags: i32 = rd(b, IOS_FMTFL);
    let fill: C = rd(b, BIOS_FILL);
    let n = text.len() as i64;
    let pad = if width <= 0 || width <= n {
        0
    } else {
        (width - n) as usize
    };
    let adjust = flags & ADJUSTFIELD;
    let mut it = Obi { failed: false, sb };
    let prefix = if adjust == INTERNAL {
        prefix.min(text.len())
    } else {
        0
    };
    if adjust != LEFT {
        for &c in &text[..prefix] {
            obi_put(&mut it, C::widen(c));
        }
        for _ in 0..pad {
            obi_put(&mut it, fill);
        }
        for &c in &text[prefix..] {
            obi_put(&mut it, C::widen(c));
        }
    } else {
        for &c in text {
            obi_put(&mut it, C::widen(c));
        }
        for _ in 0..pad {
            obi_put(&mut it, fill);
        }
    }
    wr(b, IOS_WIDE, 0i64);
    !it.failed
}

/// Longueur du préfixe placé avant le remplissage « internal » : signe,
/// puis « 0x ».
fn internal_prefix(text: &[u8]) -> usize {
    let mut p = 0;
    if matches!(text.first(), Some(b'+') | Some(b'-')) {
        p = 1;
    }
    if text.len() >= p + 2 && text[p] == b'0' && matches!(text[p + 1], b'x' | b'X') {
        p += 2;
    }
    p
}

/// Entier formaté comme `num_put` (printf `%ld`, `%lu`, `%lx`, `%lo`…) :
/// en base 8 ou 16, la valeur est lue comme un non signé de `bits` bits.
fn fmt_int(flags: i32, v: i128, bits: u32, signed: bool) -> Vec<u8> {
    let mut out = Vec::new();
    match flags & BASEFIELD {
        OCT | HEX => {
            let u = (v as u128) & ((1u128 << bits) - 1);
            let hex = flags & BASEFIELD == HEX;
            if flags & SHOWBASE != 0 && u != 0 {
                out.push(b'0');
                if hex {
                    out.push(if flags & UPPERCASE != 0 { b'X' } else { b'x' });
                }
            }
            let digits = if !hex {
                format!("{u:o}")
            } else if flags & UPPERCASE != 0 {
                format!("{u:X}")
            } else {
                format!("{u:x}")
            };
            out.extend_from_slice(digits.as_bytes());
        }
        _ => {
            if signed && v < 0 {
                out.push(b'-');
            } else if signed && flags & SHOWPOS != 0 {
                out.push(b'+');
            }
            out.extend_from_slice(v.unsigned_abs().to_string().as_bytes());
        }
    }
    out
}

/// Flottant formaté comme `num_put` : `%.*f`, `%.*e`, `%a` ou `%.*g`.
fn fmt_double(flags: i32, prec: i64, v: f64) -> Vec<u8> {
    let mut fmt = vec![b'%'];
    if flags & SHOWPOS != 0 {
        fmt.push(b'+');
    }
    if flags & SHOWPOINT != 0 {
        fmt.push(b'#');
    }
    let upper = flags & UPPERCASE != 0;
    let field = flags & FLOATFIELD;
    let hexfloat = field == FIXED | SCIENTIFIC;
    if !hexfloat {
        fmt.extend_from_slice(b".*");
    }
    fmt.push(match (field, upper) {
        (FIXED, _) => b'f',
        (SCIENTIFIC, false) => b'e',
        (SCIENTIFIC, true) => b'E',
        (_, false) if hexfloat => b'a',
        (_, true) if hexfloat => b'A',
        (_, false) => b'g',
        (_, true) => b'G',
    });
    fmt.push(0);
    let prec = prec.clamp(-1, 1000) as libc::c_int;
    let mut buf = vec![0u8; 512];
    loop {
        let n = unsafe {
            if hexfloat {
                libc::snprintf(
                    buf.as_mut_ptr() as *mut libc::c_char,
                    buf.len(),
                    fmt.as_ptr() as *const libc::c_char,
                    v,
                )
            } else {
                libc::snprintf(
                    buf.as_mut_ptr() as *mut libc::c_char,
                    buf.len(),
                    fmt.as_ptr() as *const libc::c_char,
                    prec,
                    v,
                )
            }
        };
        if n < 0 {
            return Vec::new();
        }
        if (n as usize) < buf.len() {
            buf.truncate(n as usize);
            return buf;
        }
        buf = vec![0u8; n as usize + 1];
    }
}

/// `operator<<` d'une valeur : sentinelle, `num_put`, état.
unsafe fn os_insert<C: Ch>(os: P, format: impl FnOnce(i32, i64) -> Vec<u8>) -> P {
    let b = vbase(os);
    let mut state = GOODBIT;
    if os_sentry::<C>(os) {
        let text = format(rd(b, IOS_FMTFL), rd(b, IOS_PREC));
        if !put_padded::<C>(b, &text, internal_prefix(&text)) {
            state |= BADBIT;
        }
    }
    ios_setstate::<C>(b, state);
    os_sentry_end::<C>(os);
    os
}

unsafe extern "win64" fn os_ushort<C: Ch>(os: P, v: u16) -> P {
    os_insert::<C>(os, |f, _| fmt_int(f, v as i128, 32, false))
}
unsafe extern "win64" fn os_int<C: Ch>(os: P, v: i32) -> P {
    os_insert::<C>(os, |f, _| fmt_int(f, v as i128, 32, true))
}
unsafe extern "win64" fn os_uint<C: Ch>(os: P, v: u32) -> P {
    os_insert::<C>(os, |f, _| fmt_int(f, v as i128, 32, false))
}
unsafe extern "win64" fn os_i64<C: Ch>(os: P, v: i64) -> P {
    os_insert::<C>(os, |f, _| fmt_int(f, v as i128, 64, true))
}
unsafe extern "win64" fn os_u64<C: Ch>(os: P, v: u64) -> P {
    os_insert::<C>(os, |f, _| fmt_int(f, v as i128, 64, false))
}
unsafe extern "win64" fn os_double<C: Ch>(os: P, v: f64) -> P {
    os_insert::<C>(os, |f, prec| fmt_double(f, prec, v))
}
unsafe extern "win64" fn os_bool<C: Ch>(os: P, v: bool) -> P {
    os_insert::<C>(os, |f, _| {
        if f & BOOLALPHA == 0 {
            fmt_int(f, v as i128, 32, true)
        } else if v {
            b"true".to_vec()
        } else {
            b"false".to_vec()
        }
    })
}
/// `%p` de la CRT Microsoft : seize chiffres hexadécimaux majuscules.
unsafe extern "win64" fn os_ptr<C: Ch>(os: P, v: *const c_void) -> P {
    os_insert::<C>(os, |_, _| format!("{:016X}", v as usize).into_bytes())
}
unsafe extern "win64" fn os_manip<C: Ch>(os: P, f: unsafe extern "win64" fn(P) -> P) -> P {
    f(os);
    os
}
unsafe extern "win64" fn os_ios_manip<C: Ch>(os: P, f: unsafe extern "win64" fn(P) -> P) -> P {
    f(vbase(os));
    os
}

unsafe extern "win64" fn os_put<C: Ch>(os: P, c: C) -> P {
    let b = vbase(os);
    let mut state = GOODBIT;
    if !os_sentry::<C>(os) || C::is_eof(sb_sputc::<C>(rd(b, BIOS_STRBUF), c)) {
        state |= BADBIT;
    }
    ios_setstate::<C>(b, state);
    os_sentry_end::<C>(os);
    os
}

unsafe extern "win64" fn os_write<C: Ch>(os: P, s: *const C, n: i64) -> P {
    let b = vbase(os);
    let mut state = GOODBIT;
    if !os_sentry::<C>(os) || (n > 0 && sb_sputn::<C>(rd(b, BIOS_STRBUF), s, n) != n) {
        state |= BADBIT;
    }
    ios_setstate::<C>(b, state);
    os_sentry_end::<C>(os);
    os
}

unsafe extern "win64" fn os_seekp<C: Ch>(os: P, pos: *const Fpos) -> P {
    let b = vbase(os);
    os_sentry::<C>(os);
    if !ios_fail(b) && sb_pubseekpos(rd(b, BIOS_STRBUF), &*pos, MODE_OUT).offset() == -1 {
        ios_setstate::<C>(b, FAILBIT);
    }
    os_sentry_end::<C>(os);
    os
}

// ---------- basic_istream ----------

unsafe extern "win64" fn is_ctor<C: Ch>(is: P, sb: P, isstd: bool, most_derived: i32) -> P {
    if most_derived != 0 {
        wr(is, 0, IS_VBTABLE.as_ptr());
        bios_ctor::<C>(is.offset(IS_VBOFF));
    }
    let b = set_stream_vft(is, tables().is_vt::<C>(), IS_VBOFF);
    wr(is, IS_CHCOUNT, 0i64);
    bios_init::<C>(b, sb, isstd);
    is
}

/// `??_D` : destructeur de l'objet complet, base virtuelle comprise.
unsafe extern "win64" fn is_vbase_dtor<C: Ch>(is: P) {
    let b = vbase(is);
    stream_dtor(b);
    bios_dtor::<C>(b);
}

/// `_Ipfx(noskip)` : état, vidage du flux lié, blancs sautés.
unsafe extern "win64" fn is_ipfx<C: Ch>(is: P, noskip: bool) -> bool {
    let b = vbase(is);
    if rd::<i32>(b, IOS_STATE) != GOODBIT {
        ios_setstate::<C>(b, FAILBIT);
        return false;
    }
    let tie: P = rd(b, BIOS_TIE);
    if !tie.is_null() {
        os_flush::<C>(tie);
    }
    let mut at_eof = false;
    if !noskip && rd::<i32>(b, IOS_FMTFL) & SKIPWS != 0 {
        let sb: P = rd(b, BIOS_STRBUF);
        let mut meta = sb_sgetc::<C>(sb);
        loop {
            if C::is_eof(meta) {
                at_eof = true;
                break;
            }
            if !is_space(C::from_int(meta).code()) {
                break;
            }
            meta = sb_snextc::<C>(sb);
        }
    }
    if at_eof {
        ios_setstate::<C>(b, EOFBIT | FAILBIT);
    }
    rd::<i32>(b, IOS_STATE) == GOODBIT
}

unsafe fn is_sentry<C: Ch>(is: P, noskip: bool) -> bool {
    sb_lock(rd(vbase(is), BIOS_STRBUF), false);
    is_ipfx::<C>(is, noskip)
}
unsafe fn is_sentry_end(is: P) {
    sb_lock(rd(vbase(is), BIOS_STRBUF), true);
}

/// Étape de lecture de `num_get` pour un entier : signe, préfixe, chiffres
/// de la base du flux. Rend (négatif, valeur absolue) ou None sans chiffre.
unsafe fn read_integer<C: Ch>(b: P, state: &mut i32) -> Option<(bool, u128)> {
    let sb: P = rd(b, BIOS_STRBUF);
    let mut base: u32 = match rd::<i32>(b, IOS_FMTFL) & BASEFIELD {
        OCT => 8,
        HEX => 16,
        0 => 0,
        _ => 10,
    };
    let mut meta = sb_sgetc::<C>(sb);
    let ch = |m: C::I| {
        if C::is_eof(m) {
            None
        } else {
            Some(C::from_int(m).code())
        }
    };
    let mut neg = false;
    if let Some(c) = ch(meta) {
        if c == b'+' as u32 || c == b'-' as u32 {
            neg = c == b'-' as u32;
            meta = sb_snextc::<C>(sb);
        }
    }
    let mut seen = false;
    if ch(meta) == Some(b'0' as u32) && (base == 0 || base == 16) {
        seen = true;
        meta = sb_snextc::<C>(sb);
        if matches!(ch(meta), Some(c) if c == b'x' as u32 || c == b'X' as u32) {
            base = 16;
            seen = false;
            meta = sb_snextc::<C>(sb);
        } else if base == 0 {
            base = 8;
        }
    }
    if base == 0 {
        base = 10;
    }
    let mut mag: u128 = 0;
    loop {
        let Some(c) = ch(meta) else {
            *state |= EOFBIT;
            break;
        };
        let Some(d) = char::from_u32(c).and_then(|c| c.to_digit(base)) else {
            break;
        };
        seen = true;
        mag = (mag * base as u128 + d as u128).min(u128::from(u64::MAX) + 1);
        meta = sb_snextc::<C>(sb);
    }
    if seen {
        Some((neg, mag))
    } else {
        None
    }
}

/// `operator>>` d'un entier non signé de valeur maximale `max`.
unsafe fn is_extract_unsigned<C: Ch>(is: P, max: u64, store: impl FnOnce(u64)) -> P {
    let b = vbase(is);
    let mut state = GOODBIT;
    if is_sentry::<C>(is, false) {
        match read_integer::<C>(b, &mut state) {
            None => {
                state |= FAILBIT;
                store(0);
            }
            Some((_, mag)) if mag > max as u128 => {
                state |= FAILBIT;
                store(max);
            }
            Some((neg, mag)) => {
                let m = mag as u64;
                store(if neg { m.wrapping_neg() & max } else { m });
            }
        }
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    is
}

unsafe extern "win64" fn is_ushort<C: Ch>(is: P, v: *mut u16) -> P {
    is_extract_unsigned::<C>(is, u16::MAX as u64, |x| *v = x as u16)
}
unsafe extern "win64" fn is_uint<C: Ch>(is: P, v: *mut u32) -> P {
    is_extract_unsigned::<C>(is, u32::MAX as u64, |x| *v = x as u32)
}
unsafe extern "win64" fn is_u64<C: Ch>(is: P, v: *mut u64) -> P {
    is_extract_unsigned::<C>(is, u64::MAX, |x| *v = x)
}
unsafe extern "win64" fn is_int<C: Ch>(is: P, v: *mut i32) -> P {
    let b = vbase(is);
    let mut state = GOODBIT;
    if is_sentry::<C>(is, false) {
        match read_integer::<C>(b, &mut state) {
            None => {
                state |= FAILBIT;
                *v = 0;
            }
            Some((true, mag)) if mag > 1u128 << 31 => {
                state |= FAILBIT;
                *v = i32::MIN;
            }
            Some((false, mag)) if mag > i32::MAX as u128 => {
                state |= FAILBIT;
                *v = i32::MAX;
            }
            Some((neg, mag)) => {
                let m = mag as i64;
                *v = if neg { -m } else { m } as i32;
            }
        }
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    is
}
unsafe extern "win64" fn is_manip<C: Ch>(is: P, f: unsafe extern "win64" fn(P) -> P) -> P {
    f(is);
    is
}

unsafe extern "win64" fn is_gcount<C: Ch>(is: P) -> i64 {
    rd(is, IS_CHCOUNT)
}

fn bump_gcount(is: P) {
    unsafe {
        let n: i64 = rd(is, IS_CHCOUNT);
        if n != i64::MAX {
            wr(is, IS_CHCOUNT, n + 1);
        }
    }
}

unsafe extern "win64" fn is_getline<C: Ch>(is: P, s: *mut C, count: i64) -> P {
    let b = vbase(is);
    let mut state = GOODBIT;
    wr(is, IS_CHCOUNT, 0i64);
    let mut out = s;
    let mut left = count;
    if is_sentry::<C>(is, true) && left > 0 {
        let sb: P = rd(b, BIOS_STRBUF);
        let delim = C::widen(b'\n').to_int();
        let mut meta = sb_sgetc::<C>(sb);
        loop {
            if C::is_eof(meta) {
                state |= EOFBIT;
                break;
            } else if meta == delim {
                bump_gcount(is);
                sb_sbumpc::<C>(sb);
                break;
            }
            left -= 1;
            if left <= 0 {
                state |= FAILBIT;
                break;
            }
            *out = C::from_int(meta);
            out = out.add(1);
            bump_gcount(is);
            meta = sb_snextc::<C>(sb);
        }
    }
    if count > 0 {
        *out = C::default();
    }
    let n: i64 = rd(is, IS_CHCOUNT);
    ios_setstate::<C>(b, if n == 0 { state | FAILBIT } else { state });
    is_sentry_end(is);
    is
}

unsafe extern "win64" fn is_read<C: Ch>(is: P, s: *mut C, count: i64) -> P {
    let b = vbase(is);
    let mut state = GOODBIT;
    wr(is, IS_CHCOUNT, 0i64);
    if is_sentry::<C>(is, true) && count > 0 {
        let n = sb_sgetn::<C>(rd(b, BIOS_STRBUF), s, count);
        wr(is, IS_CHCOUNT, n);
        if n != count {
            state |= EOFBIT | FAILBIT;
        }
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    is
}

unsafe fn is_seek<C: Ch>(is: P, seek: impl FnOnce(P) -> Fpos) -> P {
    let b = vbase(is);
    let old: i32 = rd(b, IOS_STATE);
    bios_clear::<C>(b, old & !EOFBIT, false);
    is_sentry::<C>(is, true);
    let mut state = GOODBIT;
    if !ios_fail(b) && seek(rd(b, BIOS_STRBUF)).offset() == -1 {
        state |= FAILBIT;
    }
    ios_setstate::<C>(b, state);
    is_sentry_end(is);
    is
}

unsafe extern "win64" fn is_seekg_pos<C: Ch>(is: P, pos: *const Fpos) -> P {
    let pos = *pos;
    is_seek::<C>(is, |sb| sb_pubseekpos(sb, &pos, MODE_IN))
}
unsafe extern "win64" fn is_seekg_off<C: Ch>(is: P, off: i64, way: i32) -> P {
    is_seek::<C>(is, |sb| sb_pubseekoff(sb, off, way, MODE_IN))
}
unsafe extern "win64" fn is_tellg<C: Ch>(is: P, out: *mut Fpos) -> *mut Fpos {
    let b = vbase(is);
    is_sentry::<C>(is, true);
    *out = if !ios_fail(b) {
        sb_pubseekoff(rd(b, BIOS_STRBUF), 0, 1, MODE_IN)
    } else {
        Fpos::at(-1)
    };
    is_sentry_end(is);
    out
}

// ---------- basic_iostream ----------

unsafe extern "win64" fn iostream_ctor<C: Ch>(ios: P, sb: P, most_derived: i32) -> P {
    if most_derived != 0 {
        wr(ios, 0, IOS_VBTABLE_I.as_ptr());
        wr(ios, 16, IOS_VBTABLE_O.as_ptr());
        bios_ctor::<C>(ios.offset(IOS_VBOFF));
    }
    let b = set_stream_vft(ios, tables().iostream_vt::<C>(), IOS_VBOFF);
    wr(ios, IS_CHCOUNT, 0i64);
    bios_init::<C>(b, sb, false);
    ios
}

// ---------- cout, cerr ----------

/// Objet statique de taille fixe, à l'adresse exportée.
#[repr(C, align(16))]
pub struct Raw<const N: usize>(UnsafeCell<[u8; N]>);
unsafe impl<const N: usize> Sync for Raw<N> {}
impl<const N: usize> Raw<N> {
    const fn new() -> Self {
        Raw(UnsafeCell::new([0; N]))
    }
    fn ptr(&self) -> P {
        self.0.get() as P
    }
}

static COUT: Raw<112> = Raw::new();
static CERR: Raw<112> = Raw::new();
/// Tampons de cout et cerr : un basic_streambuf<char> sans réserve qui
/// écrit dans le FILE de la CRT (stdout, stderr), comme le filebuf MSVC.
static COUT_BUF: Raw<112> = Raw::new();
static CERR_BUF: Raw<112> = Raw::new();
const CON_FILE: usize = SB_SIZE;

unsafe fn con_file(sb: P) -> *mut c_void {
    super::crt_stdio::std_stream(rd::<usize>(sb, CON_FILE))
}
unsafe extern "win64" fn con_overflow(sb: P, c: i32) -> i32 {
    if c == -1 {
        return 0;
    }
    if super::crt_stdio::fputc(c & 0xff, con_file(sb)) == -1 {
        -1
    } else {
        c
    }
}
unsafe extern "win64" fn con_xsputn(sb: P, s: *const u8, n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    super::crt_stdio::fwrite(s, 1, n as usize, con_file(sb)) as i64
}
unsafe extern "win64" fn con_sync(sb: P) -> i32 {
    if super::crt_stdio::fflush(con_file(sb)) == 0 {
        0
    } else {
        -1
    }
}

fn init_std_streams() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        for (buf, file) in [(&COUT_BUF, 1usize), (&CERR_BUF, 2usize)] {
            sb_ctor::<u8>(buf.ptr());
            wr(buf.ptr(), 0, tables().console.as_ptr());
            wr(buf.ptr(), CON_FILE, file);
        }
        os_ctor::<u8>(COUT.ptr(), COUT_BUF.ptr(), false, 1);
        os_ctor::<u8>(CERR.ptr(), CERR_BUF.ptr(), false, 1);
        let b = vbase(CERR.ptr());
        wr(b, BIOS_TIE, COUT.ptr());
        ios_setf(b, UNITBUF);
    });
}

// ---------- <iomanip>, fichiers, collation ----------

unsafe extern "win64" fn manip_precision(ios: P, n: i64) {
    wr(ios, IOS_PREC, n);
}
unsafe extern "win64" fn manip_width(ios: P, n: i64) {
    wr(ios, IOS_WIDE, n);
}
/// `_Smanip<streamsize>` : { fonction, argument }, rendu par adresse.
unsafe extern "win64" fn setprecision(out: *mut [usize; 2], n: i64) -> *mut [usize; 2] {
    *out = [manip_precision as *const () as usize, n as usize];
    out
}
unsafe extern "win64" fn setw(out: *mut [usize; 2], n: i64) -> *mut [usize; 2] {
    *out = [manip_width as *const () as usize, n as usize];
    out
}

/// Modes `fopen` des combinaisons d'`openmode` admises (fiopen.cpp).
fn fiopen_mode(mode: i32) -> Option<(&'static str, bool)> {
    const ATE: i32 = 4;
    const APP: i32 = 8;
    const TRUNC: i32 = 0x10;
    const BINARY: i32 = 0x20;
    const NOCREATE: i32 = 0x40;
    const NOREPLACE: i32 = 0x80;
    const IN: i32 = MODE_IN;
    const OUT: i32 = MODE_OUT;
    let valid: [(i32, &str); 20] = [
        (IN, "r"),
        (OUT, "w"),
        (OUT | TRUNC, "w"),
        (OUT | APP, "a"),
        (IN | BINARY, "rb"),
        (OUT | BINARY, "wb"),
        (OUT | TRUNC | BINARY, "wb"),
        (OUT | APP | BINARY, "ab"),
        (IN | OUT, "r+"),
        (IN | OUT | TRUNC, "w+"),
        (IN | OUT | APP, "a+"),
        (IN | OUT | BINARY, "r+b"),
        (IN | OUT | TRUNC | BINARY, "w+b"),
        (IN | OUT | APP | BINARY, "a+b"),
        (OUT | NOREPLACE, "wx"),
        (OUT | TRUNC | NOREPLACE, "wx"),
        (OUT | IN | TRUNC | NOREPLACE, "w+x"),
        (OUT | BINARY | NOREPLACE, "wbx"),
        (OUT | BINARY | TRUNC | NOREPLACE, "wbx"),
        (OUT | IN | TRUNC | BINARY | NOREPLACE, "w+bx"),
    ];
    let at_end = mode & ATE != 0;
    let mut m = mode;
    if m & NOCREATE != 0 {
        m |= IN;
    }
    if m & APP != 0 {
        m |= OUT;
    }
    m &= !(ATE | NOCREATE);
    valid
        .iter()
        .find(|(v, _)| *v == m)
        .map(|(_, s)| (*s, at_end))
}

unsafe fn fiopen_finish(fp: *mut c_void, at_end: bool) -> *mut c_void {
    if !fp.is_null() && at_end && super::crt_stdio::fseek(fp, 0, 2) != 0 {
        super::crt_stdio::fclose(fp);
        return std::ptr::null_mut();
    }
    fp
}

unsafe extern "win64" fn fiopen(name: *const u8, mode: i32, _prot: i32) -> *mut c_void {
    let Some((m, at_end)) = fiopen_mode(mode) else {
        return std::ptr::null_mut();
    };
    let cm = format!("{m}\0");
    fiopen_finish(super::crt_stdio::fopen(name, cm.as_ptr()), at_end)
}

unsafe extern "win64" fn wfiopen(name: *const u16, mode: i32, _prot: i32) -> *mut c_void {
    let Some((m, at_end)) = fiopen_mode(mode) else {
        return std::ptr::null_mut();
    };
    let wm: Vec<u16> = m.encode_utf16().chain([0]).collect();
    fiopen_finish(super::crt_stdio::wfopen(name, wm.as_ptr()), at_end)
}

/// `_Strcoll` en locale « C » : ordre des octets, puis longueur.
unsafe extern "win64" fn strcoll(
    s1: *const u8,
    e1: *const u8,
    s2: *const u8,
    e2: *const u8,
    _coll: *const u8,
) -> i32 {
    let a = std::slice::from_raw_parts(s1, e1.offset_from(s1).max(0) as usize);
    let b = std::slice::from_raw_parts(s2, e2.offset_from(s2).max(0) as usize);
    let n = a.len().min(b.len());
    match a[..n].cmp(&b[..n]) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => (a.len() as i64 - b.len() as i64).signum() as i32,
    }
}

/// `_Strxfrm` en locale « C » : copie si la place suffit, rend la taille.
unsafe extern "win64" fn strxfrm(
    s1: P,
    e1: P,
    s2: *const u8,
    e2: *const u8,
    _coll: *const u8,
) -> usize {
    let n1 = e1.offset_from(s1).max(0) as usize;
    let n2 = e2.offset_from(s2).max(0) as usize;
    if n2 <= n1 {
        std::ptr::copy(s2, s1, n2);
    }
    n2
}

// ---------- tables virtuelles ----------

struct Tables {
    facet: [usize; 3],
    locimp: [usize; 3],
    ctype_c: [usize; 11],
    ctype_w: [usize; 15],
    codecvt_c: [usize; 10],
    codecvt_w: [usize; 10],
    time_put: [usize; 4],
    bios_c: [usize; 1],
    bios_w: [usize; 1],
    sb_c: [usize; 15],
    sb_w: [usize; 15],
    console: [usize; 15],
    os_c: [usize; 1],
    os_w: [usize; 1],
    is_c: [usize; 1],
    is_w: [usize; 1],
    iostream_c: [usize; 1],
    iostream_w: [usize; 1],
}

impl Tables {
    fn pick<C: 'static>(&self, c: &[usize], w: &[usize]) -> *const usize {
        if std::any::TypeId::of::<C>() == std::any::TypeId::of::<u8>() {
            c.as_ptr()
        } else {
            w.as_ptr()
        }
    }
    fn sb_vt<C: 'static>(&self) -> *const usize {
        self.pick::<C>(&self.sb_c, &self.sb_w)
    }
    fn bios_vt<C: 'static>(&self) -> *const usize {
        self.pick::<C>(&self.bios_c, &self.bios_w)
    }
    fn os_vt<C: 'static>(&self) -> *const usize {
        self.pick::<C>(&self.os_c, &self.os_w)
    }
    fn is_vt<C: 'static>(&self) -> *const usize {
        self.pick::<C>(&self.is_c, &self.is_w)
    }
    fn iostream_vt<C: 'static>(&self) -> *const usize {
        self.pick::<C>(&self.iostream_c, &self.iostream_w)
    }
}

macro_rules! a {
    ($f:expr) => {
        $f as *const () as usize
    };
}

fn sb_table<C: Ch>() -> [usize; 15] {
    [
        a!(sb_deleting::<C>),
        a!(sb_noop),
        a!(sb_noop),
        a!(sb_eof::<C>),
        a!(sb_eof::<C>),
        a!(sb_showmanyc),
        a!(sb_underflow::<C>),
        a!(sb_uflow::<C>),
        a!(sb_xsgetn::<C>),
        a!(sb_xsputn::<C>),
        a!(sb_seekoff),
        a!(sb_seekpos),
        a!(sb_setbuf::<C>),
        a!(sb_sync),
        a!(sb_imbue),
    ]
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut console = sb_table::<u8>();
        console[3] = a!(con_overflow);
        console[9] = a!(con_xsputn);
        console[13] = a!(con_sync);
        Tables {
            facet: [a!(facet_deleting), a!(facet_incref), a!(facet_decref)],
            locimp: [a!(locimp_deleting), a!(facet_incref), a!(facet_decref)],
            ctype_c: [
                a!(ctype_c_deleting),
                a!(facet_incref),
                a!(facet_decref),
                a!(ctype_c_do_tolower_r),
                a!(ctype_c_do_tolower),
                a!(ctype_c_do_toupper_r),
                a!(ctype_c_do_toupper),
                a!(ctype_c_do_widen_r),
                a!(ctype_c_do_widen),
                a!(ctype_c_do_narrow_r),
                a!(ctype_c_do_narrow),
            ],
            ctype_w: [
                a!(ctype_w_deleting),
                a!(facet_incref),
                a!(facet_decref),
                a!(ctype_w_do_is_r),
                a!(ctype_w_do_is),
                a!(ctype_w_do_scan_is),
                a!(ctype_w_do_scan_not),
                a!(ctype_w_do_tolower_r),
                a!(ctype_w_do_tolower),
                a!(ctype_w_do_toupper_r),
                a!(ctype_w_do_toupper),
                a!(ctype_w_do_widen_r),
                a!(ctype_w_do_widen),
                a!(ctype_w_do_narrow_r),
                a!(ctype_w_do_narrow),
            ],
            codecvt_c: [
                a!(facet_deleting),
                a!(facet_incref),
                a!(facet_decref),
                a!(cvt_true),
                a!(cvt_one),
                a!(cvt_one),
                a!(cvt_c_noconv),
                a!(cvt_c_noconv),
                a!(cvt_c_unshift),
                a!(cvt_length),
            ],
            codecvt_w: [
                a!(codecvt_w_deleting),
                a!(facet_incref),
                a!(facet_decref),
                a!(cvt_false),
                a!(cvt_w_max_length),
                a!(cvt_w_encoding),
                a!(cvt_w_do_in),
                a!(cvt_w_do_out),
                a!(cvt_w_do_unshift),
                a!(cvt_length),
            ],
            time_put: [
                a!(time_put_deleting),
                a!(facet_incref),
                a!(facet_decref),
                a!(time_put_do_put),
            ],
            bios_c: [a!(ios_base_deleting)],
            bios_w: [a!(ios_base_deleting)],
            sb_c: sb_table::<u8>(),
            sb_w: sb_table::<u16>(),
            console,
            os_c: [a!(os_deleting::<u8>)],
            os_w: [a!(os_deleting::<u16>)],
            is_c: [a!(is_deleting::<u8>)],
            is_w: [a!(is_deleting::<u16>)],
            iostream_c: [a!(iostream_deleting::<u8>)],
            iostream_w: [a!(iostream_deleting::<u16>)],
        }
    })
}

// ---------- exports ----------

pub fn exports() -> Exports {
    init_std_streams();
    crate::exports! {
        // basic_ios
        "??0?$basic_ios@DU?$char_traits@D@std@@@std@@IEAA@XZ" => fn bios_ctor::<u8>,
        "??0?$basic_ios@_WU?$char_traits@_W@std@@@std@@IEAA@XZ" => fn bios_ctor::<u16>,
        "??1?$basic_ios@DU?$char_traits@D@std@@@std@@UEAA@XZ" => fn bios_dtor::<u8>,
        "??1?$basic_ios@_WU?$char_traits@_W@std@@@std@@UEAA@XZ" => fn bios_dtor::<u16>,
        "?clear@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAAXH_N@Z" => fn bios_clear::<u8>,
        "?setstate@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAAXH_N@Z" => fn bios_setstate::<u8>,
        "?setstate@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEAAXH_N@Z" => fn bios_setstate::<u16>,
        "?fill@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAADD@Z" => fn bios_set_fill::<u8>,
        "?fill@?$basic_ios@DU?$char_traits@D@std@@@std@@QEBADXZ" => fn bios_fill::<u8>,
        "?fill@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEAA_W_W@Z" => fn bios_set_fill::<u16>,
        "?fill@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEBA_WXZ" => fn bios_fill::<u16>,
        "?rdbuf@?$basic_ios@DU?$char_traits@D@std@@@std@@QEBAPEAV?$basic_streambuf@DU?$char_traits@D@std@@@2@XZ" => fn bios_rdbuf::<u8>,
        "?rdbuf@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEBAPEAV?$basic_streambuf@_WU?$char_traits@_W@std@@@2@XZ" => fn bios_rdbuf::<u16>,
        "?tie@?$basic_ios@DU?$char_traits@D@std@@@std@@QEBAPEAV?$basic_ostream@DU?$char_traits@D@std@@@2@XZ" => fn bios_tie::<u8>,
        "?tie@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEBAPEAV?$basic_ostream@_WU?$char_traits@_W@std@@@2@XZ" => fn bios_tie::<u16>,
        "?widen@?$basic_ios@DU?$char_traits@D@std@@@std@@QEBADD@Z" => fn bios_widen::<u8>,
        "?widen@?$basic_ios@_WU?$char_traits@_W@std@@@std@@QEBA_WD@Z" => fn bios_widen::<u16>,
        "?imbue@?$basic_ios@DU?$char_traits@D@std@@@std@@QEAA?AVlocale@2@AEBV32@@Z" => fn bios_imbue::<u8>,
        // ios_base
        "?flags@ios_base@std@@QEBAHXZ" => fn ios_flags,
        "?setf@ios_base@std@@QEAAHH@Z" => fn ios_setf,
        "?setf@ios_base@std@@QEAAHHH@Z" => fn ios_setf2,
        "?unsetf@ios_base@std@@QEAAXH@Z" => fn ios_unsetf,
        "?width@ios_base@std@@QEAA_J_J@Z" => fn ios_set_width,
        "?width@ios_base@std@@QEBA_JXZ" => fn ios_width,
        "?good@ios_base@std@@QEBA_NXZ" => fn ios_good,
        "?eof@ios_base@std@@QEBA_NXZ" => fn ios_eof,
        "?fail@ios_base@std@@QEBA_NXZ" => fn ios_fail,
        "?bad@ios_base@std@@QEBA_NXZ" => fn ios_bad,
        "??7ios_base@std@@QEBA_NXZ" => fn ios_not,
        "??Bios_base@std@@QEBA_NXZ" => fn ios_to_bool,
        "?getloc@ios_base@std@@QEBA?AVlocale@2@XZ" => fn ios_getloc,
        "?sync_with_stdio@ios_base@std@@SA_N_N@Z" => fn ios_sync_with_stdio,
        // basic_streambuf
        "??0?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAA@XZ" => fn sb_ctor::<u8>,
        "??0?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEAA@XZ" => fn sb_ctor::<u16>,
        "??1?$basic_streambuf@DU?$char_traits@D@std@@@std@@UEAA@XZ" => fn sb_dtor::<u8>,
        "??1?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@UEAA@XZ" => fn sb_dtor::<u16>,
        "?_Gnavail@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBA_JXZ" => fn sb_gnavail::<u8>,
        "?_Gndec@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAPEADXZ" => fn sb_gndec::<u8>,
        "?_Gninc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAPEADXZ" => fn sb_gninc::<u8>,
        "?_Pnavail@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBA_JXZ" => fn sb_pnavail::<u8>,
        "?_Pninc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAPEADXZ" => fn sb_pninc::<u8>,
        "?_Pninc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEAAPEA_WXZ" => fn sb_pninc::<u16>,
        "?_Init@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXPEAPEAD0PEAH001@Z" => fn sb_init6::<u8>,
        "?_Init@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXXZ" => fn sb_init0::<u8>,
        "?_Lock@?$basic_streambuf@DU?$char_traits@D@std@@@std@@UEAAXXZ" => fn sb_noop,
        "?_Lock@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@UEAAXXZ" => fn sb_noop,
        "?_Unlock@?$basic_streambuf@DU?$char_traits@D@std@@@std@@UEAAXXZ" => fn sb_noop,
        "?_Unlock@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@UEAAXXZ" => fn sb_noop,
        "?eback@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ" => fn sb_eback::<u8>,
        "?eback@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEBAPEA_WXZ" => fn sb_eback::<u16>,
        "?egptr@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ" => fn sb_egptr::<u8>,
        "?egptr@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEBAPEA_WXZ" => fn sb_egptr::<u16>,
        "?epptr@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ" => fn sb_epptr::<u8>,
        "?epptr@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEBAPEA_WXZ" => fn sb_epptr::<u16>,
        "?gptr@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ" => fn sb_gptr::<u8>,
        "?gptr@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEBAPEA_WXZ" => fn sb_gptr::<u16>,
        "?pbase@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ" => fn sb_pbase::<u8>,
        "?pbase@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEBAPEA_WXZ" => fn sb_pbase::<u16>,
        "?pptr@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEBAPEADXZ" => fn sb_pptr::<u8>,
        "?pptr@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEBAPEA_WXZ" => fn sb_pptr::<u16>,
        "?gbump@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXH@Z" => fn sb_gbump::<u8>,
        "?gbump@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEAAXH@Z" => fn sb_gbump::<u16>,
        "?pbump@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXH@Z" => fn sb_pbump::<u8>,
        "?setg@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXPEAD00@Z" => fn sb_setg::<u8>,
        "?setg@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEAAXPEA_W00@Z" => fn sb_setg::<u16>,
        "?setp@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXPEAD00@Z" => fn sb_setp3::<u8>,
        "?setp@?$basic_streambuf@DU?$char_traits@D@std@@@std@@IEAAXPEAD0@Z" => fn sb_setp2::<u8>,
        "?setp@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEAAXPEA_W00@Z" => fn sb_setp3::<u16>,
        "?setp@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@IEAAXPEA_W0@Z" => fn sb_setp2::<u16>,
        "?getloc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEBA?AVlocale@2@XZ" => fn sb_getloc,
        "?imbue@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAXAEBVlocale@2@@Z" => fn sb_imbue,
        "?imbue@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAAXAEBVlocale@2@@Z" => fn sb_imbue,
        "?sbumpc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAAHXZ" => fn sb_sbumpc::<u8>,
        "?sbumpc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@QEAAGXZ" => fn sb_sbumpc::<u16>,
        "?sgetc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAAHXZ" => fn sb_sgetc::<u8>,
        "?sgetc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@QEAAGXZ" => fn sb_sgetc::<u16>,
        "?snextc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAAHXZ" => fn sb_snextc::<u8>,
        "?snextc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@QEAAGXZ" => fn sb_snextc::<u16>,
        "?sputc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAAHD@Z" => fn sb_sputc::<u8>,
        "?sputc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@QEAAG_W@Z" => fn sb_sputc::<u16>,
        "?sputn@?$basic_streambuf@DU?$char_traits@D@std@@@std@@QEAA_JPEBD_J@Z" => fn sb_sputn::<u8>,
        "?sputn@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@QEAA_JPEB_W_J@Z" => fn sb_sputn::<u16>,
        "?setbuf@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAPEAV12@PEAD_J@Z" => fn sb_setbuf::<u8>,
        "?setbuf@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAAPEAV12@PEA_W_J@Z" => fn sb_setbuf::<u16>,
        "?showmanyc@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA_JXZ" => fn sb_showmanyc,
        "?showmanyc@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAA_JXZ" => fn sb_showmanyc,
        "?sync@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHXZ" => fn sb_sync,
        "?sync@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAAHXZ" => fn sb_sync,
        "?uflow@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAHXZ" => fn sb_uflow::<u8>,
        "?uflow@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAAGXZ" => fn sb_uflow::<u16>,
        "?xsgetn@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA_JPEAD_J@Z" => fn sb_xsgetn::<u8>,
        "?xsgetn@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAA_JPEA_W_J@Z" => fn sb_xsgetn::<u16>,
        "?xsputn@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAA_JPEBD_J@Z" => fn sb_xsputn::<u8>,
        "?xsputn@?$basic_streambuf@_WU?$char_traits@_W@std@@@std@@MEAA_JPEB_W_J@Z" => fn sb_xsputn::<u16>,
        // basic_ostream
        "??0?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAA@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@_N@Z" => fn os_ctor::<u8>,
        "??0?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAA@PEAV?$basic_streambuf@_WU?$char_traits@_W@std@@@1@_N@Z" => fn os_ctor::<u16>,
        "??1?$basic_ostream@DU?$char_traits@D@std@@@std@@UEAA@XZ" => fn stream_dtor,
        "??1?$basic_ostream@_WU?$char_traits@_W@std@@@std@@UEAA@XZ" => fn stream_dtor,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@G@Z" => fn os_ushort::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@H@Z" => fn os_int::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@I@Z" => fn os_uint::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@J@Z" => fn os_int::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@K@Z" => fn os_uint::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@N@Z" => fn os_double::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@P6AAEAV01@AEAV01@@Z@Z" => fn os_manip::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@P6AAEAVios_base@1@AEAV21@@Z@Z" => fn os_ios_manip::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@PEBX@Z" => fn os_ptr::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@_J@Z" => fn os_i64::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@_K@Z" => fn os_u64::<u8>,
        "??6?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@_N@Z" => fn os_bool::<u8>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@G@Z" => fn os_ushort::<u16>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@H@Z" => fn os_int::<u16>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@I@Z" => fn os_uint::<u16>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@K@Z" => fn os_uint::<u16>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@P6AAEAV01@AEAV01@@Z@Z" => fn os_manip::<u16>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@P6AAEAVios_base@1@AEAV21@@Z@Z" => fn os_ios_manip::<u16>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@_J@Z" => fn os_i64::<u16>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@_K@Z" => fn os_u64::<u16>,
        "??6?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV01@_N@Z" => fn os_bool::<u16>,
        "?flush@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@XZ" => fn os_flush::<u8>,
        "?flush@?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV12@XZ" => fn os_flush::<u16>,
        "?_Osfx@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAXXZ" => fn os_osfx::<u8>,
        "?_Osfx@?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAXXZ" => fn os_osfx::<u16>,
        "?put@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@D@Z" => fn os_put::<u8>,
        "?put@?$basic_ostream@_WU?$char_traits@_W@std@@@std@@QEAAAEAV12@_W@Z" => fn os_put::<u16>,
        "?write@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@PEBD_J@Z" => fn os_write::<u8>,
        "?seekp@?$basic_ostream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@V?$fpos@U_Mbstatet@@@2@@Z" => fn os_seekp::<u8>,
        // basic_istream
        "??0?$basic_istream@DU?$char_traits@D@std@@@std@@QEAA@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@_N@Z" => fn is_ctor::<u8>,
        "??1?$basic_istream@DU?$char_traits@D@std@@@std@@UEAA@XZ" => fn stream_dtor,
        "??_D?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAXXZ" => fn is_vbase_dtor::<u8>,
        "??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAG@Z" => fn is_ushort::<u8>,
        "??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAH@Z" => fn is_int::<u8>,
        "??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEAI@Z" => fn is_uint::<u8>,
        "??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@AEA_K@Z" => fn is_u64::<u8>,
        "??5?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV01@P6AAEAV01@AEAV01@@Z@Z" => fn is_manip::<u8>,
        "?_Ipfx@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAA_N_N@Z" => fn is_ipfx::<u8>,
        "?_Ipfx@?$basic_istream@_WU?$char_traits@_W@std@@@std@@QEAA_N_N@Z" => fn is_ipfx::<u16>,
        "?gcount@?$basic_istream@DU?$char_traits@D@std@@@std@@QEBA_JXZ" => fn is_gcount::<u8>,
        "?getline@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@PEAD_J@Z" => fn is_getline::<u8>,
        "?read@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@PEAD_J@Z" => fn is_read::<u8>,
        "?seekg@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@V?$fpos@U_Mbstatet@@@2@@Z" => fn is_seekg_pos::<u8>,
        "?seekg@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAAAEAV12@_JH@Z" => fn is_seekg_off::<u8>,
        "?tellg@?$basic_istream@DU?$char_traits@D@std@@@std@@QEAA?AV?$fpos@U_Mbstatet@@@2@XZ" => fn is_tellg::<u8>,
        // basic_iostream
        "??0?$basic_iostream@DU?$char_traits@D@std@@@std@@QEAA@PEAV?$basic_streambuf@DU?$char_traits@D@std@@@1@@Z" => fn iostream_ctor::<u8>,
        "??0?$basic_iostream@_WU?$char_traits@_W@std@@@std@@QEAA@PEAV?$basic_streambuf@_WU?$char_traits@_W@std@@@1@@Z" => fn iostream_ctor::<u16>,
        "??1?$basic_iostream@DU?$char_traits@D@std@@@std@@UEAA@XZ" => fn stream_dtor,
        "??1?$basic_iostream@_WU?$char_traits@_W@std@@@std@@UEAA@XZ" => fn stream_dtor,
        // Objets globaux et manipulateurs
        "?cout@std@@3V?$basic_ostream@DU?$char_traits@D@std@@@1@A" => data COUT,
        "?cerr@std@@3V?$basic_ostream@DU?$char_traits@D@std@@@1@A" => data CERR,
        "?setw@std@@YA?AU?$_Smanip@_J@1@_J@Z" => fn setw,
        "?setprecision@std@@YA?AU?$_Smanip@_J@1@_J@Z" => fn setprecision,
        "?_Fiopen@std@@YAPEAU_iobuf@@PEBDHH@Z" => fn fiopen,
        "?_Fiopen@std@@YAPEAU_iobuf@@PEB_WHH@Z" => fn wfiopen,
        // Locales
        "??0_Lockit@std@@QEAA@H@Z" => fn lockit_ctor,
        "??1_Lockit@std@@QEAA@XZ" => fn lockit_dtor,
        "??0_Locinfo@std@@QEAA@HPEBD@Z" => fn locinfo_ctor_cat,
        "??0_Locinfo@std@@QEAA@PEBD@Z" => fn locinfo_ctor,
        "??1_Locinfo@std@@QEAA@XZ" => fn locinfo_dtor,
        "?_Getname@_Locinfo@std@@QEBAPEBDXZ" => fn locinfo_getname,
        "?_Getcoll@_Locinfo@std@@QEBA?AU_Collvec@@XZ" => fn locinfo_getcoll,
        "??4?$_Yarn@D@std@@QEAAAEAV01@PEBD@Z" => fn yarn_assign,
        "?c_str@?$_Yarn@D@std@@QEBAPEBDXZ" => fn yarn_c_str,
        "??0facet@locale@std@@IEAA@_K@Z" => fn facet_ctor,
        "??1facet@locale@std@@MEAA@XZ" => fn facet_dtor,
        "?_Incref@facet@locale@std@@UEAAXXZ" => fn facet_incref,
        "?_Decref@facet@locale@std@@UEAAPEAV_Facet_base@3@XZ" => fn facet_decref,
        "?_Id_cnt@id@locale@std@@0HA" => data ID_CNT,
        "?_Init@locale@std@@CAPEAV_Locimp@12@_N@Z" => fn locale_init,
        "?_Getgloballocale@locale@std@@CAPEAV_Locimp@12@XZ" => fn getgloballocale,
        "?classic@locale@std@@SAAEBV12@XZ" => fn locale_classic,
        "?_Locimp_Addfac@_Locimp@locale@std@@CAXPEAV123@PEAVfacet@23@_K@Z" => fn locimp_addfac,
        "?_Makeloc@_Locimp@locale@std@@CAPEAV123@AEBV_Locinfo@3@HPEAV123@PEBV23@@Z" => fn makeloc,
        "?_New_Locimp@_Locimp@locale@std@@CAPEAV123@AEBV123@@Z" => fn new_locimp_copy,
        "?_New_Locimp@_Locimp@locale@std@@CAPEAV123@_N@Z" => fn new_locimp,
        // Facettes
        "?id@?$ctype@D@std@@2V0locale@2@A" => data ID_CTYPE_C,
        "?id@?$ctype@_W@std@@2V0locale@2@A" => data ID_CTYPE_W,
        "?id@?$codecvt@DDU_Mbstatet@@@std@@2V0locale@2@A" => data ID_CODECVT_C,
        "?id@?$codecvt@_WDU_Mbstatet@@@std@@2V0locale@2@A" => data ID_CODECVT_W,
        "?id@?$collate@D@std@@2V0locale@2@A" => data ID_COLLATE_C,
        "?id@?$time_put@DV?$ostreambuf_iterator@DU?$char_traits@D@std@@@std@@@std@@2V0locale@2@A" => data ID_TIME_PUT_C,
        "?_Getcat@?$ctype@D@std@@SA_KPEAPEBVfacet@locale@2@PEBV42@@Z" => fn getcat_ctype_c,
        "?_Getcat@?$ctype@_W@std@@SA_KPEAPEBVfacet@locale@2@PEBV42@@Z" => fn getcat_ctype_w,
        "?_Getcat@?$codecvt@DDU_Mbstatet@@@std@@SA_KPEAPEBVfacet@locale@2@PEBV42@@Z" => fn getcat_codecvt_c,
        "?_Getcat@?$time_put@DV?$ostreambuf_iterator@DU?$char_traits@D@std@@@std@@@std@@SA_KPEAPEBVfacet@locale@2@PEBV42@@Z" => fn getcat_time_put_c,
        "?tolower@?$ctype@D@std@@QEBADD@Z" => fn ctype_c_tolower,
        "?tolower@?$ctype@D@std@@QEBAPEBDPEADPEBD@Z" => fn ctype_c_tolower_r,
        "?toupper@?$ctype@D@std@@QEBADD@Z" => fn ctype_c_toupper,
        "?widen@?$ctype@D@std@@QEBADD@Z" => fn ctype_c_widen,
        "?tolower@?$ctype@_W@std@@QEBAPEB_WPEA_WPEB_W@Z" => fn ctype_w_tolower_r,
        "?tolower@?$ctype@_W@std@@QEBA_W_W@Z" => fn ctype_w_tolower,
        "?toupper@?$ctype@_W@std@@QEBAPEB_WPEA_WPEB_W@Z" => fn ctype_w_toupper_r,
        "?toupper@?$ctype@_W@std@@QEBA_W_W@Z" => fn ctype_w_toupper,
        "?widen@?$ctype@_W@std@@QEBA_WD@Z" => fn ctype_w_widen,
        "??0?$codecvt@_WDU_Mbstatet@@@std@@QEAA@_K@Z" => fn codecvt_w_ctor,
        "??1?$codecvt@_WDU_Mbstatet@@@std@@MEAA@XZ" => fn codecvt_w_dtor,
        "?in@?$codecvt@DDU_Mbstatet@@@std@@QEBAHAEAU_Mbstatet@@PEBD1AEAPEBDPEAD3AEAPEAD@Z" => fn cvt_c_in,
        "?out@?$codecvt@DDU_Mbstatet@@@std@@QEBAHAEAU_Mbstatet@@PEBD1AEAPEBDPEAD3AEAPEAD@Z" => fn cvt_c_out,
        "?unshift@?$codecvt@DDU_Mbstatet@@@std@@QEBAHAEAU_Mbstatet@@PEAD1AEAPEAD@Z" => fn cvt_c_unshift_pub,
        "?in@?$codecvt@_WDU_Mbstatet@@@std@@QEBAHAEAU_Mbstatet@@PEBD1AEAPEBDPEA_W3AEAPEA_W@Z" => fn cvt_w_in,
        "?always_noconv@codecvt_base@std@@QEBA_NXZ" => fn always_noconv,
        "?put@?$time_put@DV?$ostreambuf_iterator@DU?$char_traits@D@std@@@std@@@std@@QEBA?AV?$ostreambuf_iterator@DU?$char_traits@D@std@@@2@V32@AEAVios_base@2@DPEBUtm@@PEBD3@Z" => fn time_put_put,
        "_Strcoll" => fn strcoll,
        "_Strxfrm" => fn strxfrm,
    }
}

#[cfg(test)]
mod tests;
