//! Tests de `std::exception_ptr` (#2307).

use super::*;
use std::cell::{Cell, UnsafeCell};

thread_local! {
    static COPIES: Cell<u32> = const { Cell::new(0) };
    static DETRUITS: Cell<Vec<usize>> = const { Cell::new(Vec::new()) };
}

/// Constructeur de copie de l'objet d'essai : la copie vaut source + 1000.
unsafe extern "win64" fn copie_essai(dest: usize, src: usize) {
    COPIES.with(|c| c.set(c.get() + 1));
    *(dest as *mut u64) = *(src as *const u64) + 1000;
}

unsafe extern "win64" fn dtor_essai(obj: usize) {
    DETRUITS.with(|d| {
        let mut v = d.take();
        v.push(obj);
        d.set(v);
    });
}

fn detruits() -> Vec<usize> {
    DETRUITS.with(|d| {
        let v = d.take();
        d.set(v.clone());
        v
    })
}

/// ThrowInfo, tableau de types capturables et type capturable x64, en
/// déplacements relatifs à `base` comme dans une image.
#[repr(C, align(16))]
struct Tables {
    ti: [u32; 4],
    cta: [u32; 2],
    ct: [u32; 7],
}

/// Prépare les tables d'un objet de 8 octets, non simple, avec
/// constructeur de copie et destructeur. Renvoie (ThrowInfo, base).
fn tables(t: &mut Tables) -> (usize, usize) {
    let adr = &*t as *const Tables as usize;
    let copie = copie_essai as *const () as usize;
    let dtor = dtor_essai as *const () as usize;
    // Une page sous la plus basse des trois adresses : aucune RVA ne vaut
    // 0, valeur qui signifie « pas de destructeur » (pmfnUnwind) ou « pas
    // de constructeur de copie ». Arrondie seulement à la page, la base
    // tombait sur dtor_essai quand le lieur la plaçait en début de page, et
    // le destructeur n'était plus appelé (#2378).
    let base = (adr.min(copie).min(dtor) & !0xFFF) - 0x1000;
    let rva = |a: usize| u32::try_from(a - base).expect("même image");
    let cta = adr + std::mem::offset_of!(Tables, cta);
    let ct = adr + std::mem::offset_of!(Tables, ct);
    t.ti = [0, rva(dtor), 0, rva(cta)];
    t.cta = [1, rva(ct)];
    // properties, pType, mdisp, pdisp (-1 : pas de base virtuelle), vdisp,
    // sizeOrOffset, copyFunction.
    t.ct = [0, 0, 0, u32::MAX, 0, 8, rva(copie)];
    (adr, base)
}

impl Tables {
    const VIDES: Tables = Tables {
        ti: [0; 4],
        cta: [0; 2],
        ct: [0; 7],
    };
}

/// Les tables vivent dans l'image du binaire de test (#2325) : un tas peut
/// se trouver à plus de 4 Gio du code, et les déplacements relatifs à la
/// base, sur 32 bits, ne tiendraient plus. Un emplacement par test, les
/// tests tournant en parallèle.
struct Emplacements(UnsafeCell<[Tables; 4]>);
// SAFETY : chaque test n'emploie que son propre emplacement.
unsafe impl Sync for Emplacements {}
static EMPLACEMENTS: Emplacements = Emplacements(UnsafeCell::new([Tables::VIDES; 4]));

const CAPTURE: usize = 0;
const AFFECTATION: usize = 1;
const RELANCE: usize = 2;
const SIMPLE: usize = 3;

/// Les tables vierges de l'emplacement `i`, propre à un seul test.
fn neuves(i: usize) -> &'static mut Tables {
    // SAFETY : `i` n'est employé que par un seul test.
    let t = unsafe { &mut (*EMPLACEMENTS.0.get())[i] };
    *t = Tables::VIDES;
    t
}

fn nul() -> [usize; 2] {
    [0xDEAD, 0xBEEF]
}

unsafe fn record_cxx(obj: *const u64, ti: usize, base: usize) -> ExceptionRecord {
    let mut r = ExceptionRecord::new(cxx_eh::CXX_EXCEPTION, EXCEPTION_NONCONTINUABLE, 0);
    r.nparams = 4;
    r.info[0] = 0x1993_0520;
    r.info[1] = obj as usize;
    r.info[2] = ti;
    r.info[3] = base;
    r
}

#[test]
fn vide_construit_compare_et_detruit() {
    unsafe {
        let mut a = nul();
        let mut b = nul();
        eptr_create(a.as_mut_ptr().cast());
        eptr_create(b.as_mut_ptr().cast());
        assert_eq!(a, [0, 0]);
        assert!(!eptr_to_bool(a.as_ptr().cast()));
        assert!(eptr_compare(a.as_ptr().cast(), b.as_ptr().cast()));
        eptr_destroy(a.as_mut_ptr().cast());
        assert_eq!(a, [0, 0]);
    }
}

#[test]
fn capture_copie_l_objet_et_le_detruit_au_dernier_lacher() {
    let t = neuves(CAPTURE);
    let (ti, base) = tables(t);
    unsafe {
        let obj = 7u64;
        let mut p = nul();
        *eptr(p.as_mut_ptr().cast()) = capturer(&record_cxx(&obj, ti, base));
        assert_eq!(COPIES.with(|c| c.get()), 1);
        assert!(eptr_to_bool(p.as_ptr().cast()));
        let rec = &*(p[0] as *const ExceptionRecord);
        assert_eq!(rec.code, 0xE06D_7363);
        assert_ne!(
            rec.info[1], &obj as *const u64 as usize,
            "l'objet est copié"
        );
        assert_eq!(*(rec.info[1] as *const u64), 1007);
        assert_eq!(rec.info[1] % 16, 0);
        let copie = rec.info[1];

        let mut q = nul();
        eptr_copy(q.as_mut_ptr().cast(), p.as_ptr().cast());
        assert_eq!(p, q);
        assert!(eptr_compare(p.as_ptr().cast(), q.as_ptr().cast()));
        eptr_destroy(p.as_mut_ptr().cast());
        assert!(detruits().is_empty(), "une référence reste");
        eptr_destroy(q.as_mut_ptr().cast());
        assert_eq!(detruits(), vec![copie]);
    }
}

#[test]
fn affectation_lache_l_ancien_et_echange() {
    let t = neuves(AFFECTATION);
    let (ti, base) = tables(t);
    unsafe {
        let (o1, o2) = (1u64, 2u64);
        let mut a = nul();
        let mut b = nul();
        *eptr(a.as_mut_ptr().cast()) = capturer(&record_cxx(&o1, ti, base));
        *eptr(b.as_mut_ptr().cast()) = capturer(&record_cxx(&o2, ti, base));
        let copie_a = (*(a[0] as *const ExceptionRecord)).info[1];
        let (a0, b0) = (a, b);
        eptr_swap(a.as_mut_ptr().cast(), b.as_mut_ptr().cast());
        assert_eq!((a, b), (b0, a0));
        // b tient maintenant l'ancien a ; l'affecter depuis a le lâche.
        eptr_assign(b.as_mut_ptr().cast(), a.as_ptr().cast());
        assert_eq!(detruits(), vec![copie_a]);
        assert_eq!(a, b);
        eptr_destroy(a.as_mut_ptr().cast());
        eptr_destroy(b.as_mut_ptr().cast());
        assert_eq!(detruits().len(), 2);
    }
}

#[test]
fn exception_courante_absente_ou_seh() {
    unsafe {
        let mut p = [0usize; 2];
        eptr_current_exception(p.as_mut_ptr().cast());
        assert_eq!(p, [0, 0], "rien en cours : exception_ptr vide");

        let mut r = ExceptionRecord::new(0xC000_0005, 0, 0x1234);
        r.nparams = 2;
        r.info[0] = 1;
        r.info[1] = 0x42;
        let slot = cxx_eh::current_exception();
        *slot = &r as *const ExceptionRecord as usize;
        eptr_current_exception(p.as_mut_ptr().cast());
        *slot = 0;
        let c = &*(p[0] as *const ExceptionRecord);
        assert_eq!((c.code, c.nparams, c.info[1]), (0xC000_0005, 2, 0x42));
        assert_eq!(c.flags & EXCEPTION_NONCONTINUABLE, EXCEPTION_NONCONTINUABLE);
        assert_eq!(c.address, 0);
        eptr_destroy(p.as_mut_ptr().cast());
    }
}

#[test]
fn relance_leve_une_nouvelle_copie_rendue_apres_le_catch() {
    let t = neuves(RELANCE);
    let (ti, base) = tables(t);
    unsafe {
        let obj = 5u64;
        let mut p = nul();
        *eptr(p.as_mut_ptr().cast()) = capturer(&record_cxx(&obj, ti, base));
        let tenue = (*(p[0] as *const ExceptionRecord)).info[1];
        let r = &*preparer_relance(p.as_ptr().cast());
        assert_eq!((r.code, r.nparams), (0xE06D_7363, 4));
        assert_eq!(r.info[2], ti);
        let levee = r.info[1];
        assert_ne!(levee, tenue);
        assert_eq!(*(levee as *const u64), 2005);
        assert_eq!(*(tenue as *const u64), 1005, "l'objet tenu reste intact");
        let avant = NB_RELANCEES.load(Ordering::Acquire);
        assert!(avant >= 1);
        // Fin du catch : cxx_eh détruit l'objet levé et rend sa copie.
        cxx_eh::destroy_thrown(levee, ti, base);
        liberer_copie_relancee(levee);
        assert!(!RELANCEES
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|m| m.contains_key(&levee)));
        eptr_destroy(p.as_mut_ptr().cast());
        assert_eq!(detruits(), vec![levee, tenue]);
    }
}

#[test]
fn make_exception_ptr_par_copy_exception() {
    let t = neuves(SIMPLE);
    let (ti, _base) = tables(t);
    // module_base ne connaît pas ces tables : on vérifie seulement que la
    // capture d'un objet simple (type sans constructeur) recopie ses octets.
    t.ct[0] = 1;
    unsafe {
        let mut r = record_cxx(&9u64, ti, 0);
        r.info[3] = _base;
        let mut p = nul();
        *eptr(p.as_mut_ptr().cast()) = capturer(&r);
        let rec = &*(p[0] as *const ExceptionRecord);
        assert_eq!(*(rec.info[1] as *const u64), 9);
        assert_eq!(COPIES.with(|c| c.get()), 0);
        eptr_destroy(p.as_mut_ptr().cast());
    }
}

#[test]
fn noms_exportes() {
    let e = exports();
    for n in [
        "?__ExceptionPtrCreate@@YAXPEAX@Z",
        "?__ExceptionPtrDestroy@@YAXPEAX@Z",
        "?__ExceptionPtrCopy@@YAXPEAXPEBX@Z",
        "?__ExceptionPtrAssign@@YAXPEAXPEBX@Z",
        "?__ExceptionPtrCompare@@YA_NPEBX0@Z",
        "?__ExceptionPtrToBool@@YA_NPEBX@Z",
        "?__ExceptionPtrSwap@@YAXPEAX0@Z",
        "?__ExceptionPtrCurrentException@@YAXPEAX@Z",
        "?__ExceptionPtrRethrow@@YAXPEBX@Z",
        "?__ExceptionPtrCopyException@@YAXPEAXPEBX1@Z",
        "?_Rethrow_future_exception@std@@YAXVexception_ptr@1@@Z",
    ] {
        assert!(e.contains_key(n), "{n}");
    }
}
