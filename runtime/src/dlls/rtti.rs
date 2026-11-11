//! RTTI de MSVC : `__RTDynamicCast`, `__RTtypeid`, `__RTCastToVoid`
//! (vcruntime140), pour `dynamic_cast` et `typeid` des programmes C++.
//!
//! Disposition x64 des structures, telle que publiée (articles sur la
//! rétro-ingénierie des classes Visual C++, en-tête `rttidata.h` du SDK) :
//! - vtable[-1] → *Complete Object Locator* : signature, offset, cdOffset,
//!   puis les RVA du TypeDescriptor, du ClassHierarchyDescriptor et de
//!   lui-même (qui donne la base de l'image) ;
//! - TypeDescriptor : pVFTable, spare, nom décoré (« .?AVFoo@@ ») ;
//! - ClassHierarchyDescriptor : signature, attributs, nombre de bases,
//!   RVA du tableau des bases ;
//! - BaseClassDescriptor : RVA du TypeDescriptor, bases contenues,
//!   PMD { mdisp, pdisp, vdisp }, attributs.

use std::ffi::CStr;

const BCD_NOTVISIBLE: u32 = 0x01;
const BCD_AMBIGUOUS: u32 = 0x02;

unsafe fn rd_i32(p: usize) -> i32 {
    (p as *const i32).read_unaligned()
}

unsafe fn rd_u32(p: usize) -> u32 {
    (p as *const u32).read_unaligned()
}

unsafe fn rd_ptr(p: usize) -> usize {
    (p as *const usize).read_unaligned()
}

/// Locateur d'objet complet de l'objet polymorphe `obj`, et base de l'image.
unsafe fn locator(obj: usize) -> Option<(usize, usize)> {
    let vt = rd_ptr(obj);
    if vt == 0 {
        return None;
    }
    let col = rd_ptr(vt - 8);
    if col == 0 {
        return None;
    }
    // Signature 1 (x64) : la base se déduit de pSelf.
    let base = if rd_u32(col) == 1 {
        col.wrapping_sub(rd_i32(col + 20) as u32 as usize)
    } else {
        0
    };
    Some((col, base))
}

/// Adresse de l'objet complet depuis un sous-objet.
unsafe fn complete_object(obj: usize, col: usize) -> usize {
    let offset = rd_u32(col + 4) as usize;
    let cd = rd_u32(col + 8) as usize;
    let mut p = obj;
    if cd != 0 {
        // Déplacement vtordisp stocké juste avant le sous-objet.
        p = p.wrapping_sub(rd_i32(obj - cd) as isize as usize);
    }
    p.wrapping_sub(offset)
}

fn rva(base: usize, r: i32) -> usize {
    if base == 0 {
        r as isize as usize
    } else {
        base.wrapping_add(r as u32 as usize)
    }
}

unsafe fn type_name<'a>(td: usize) -> &'a [u8] {
    CStr::from_ptr((td + 16) as *const std::ffi::c_char).to_bytes()
}

/// Deux TypeDescriptor désignent le même type (même adresse ou même nom :
/// chaque DLL a sa copie).
unsafe fn same_type(a: usize, b: usize) -> bool {
    a == b || (a != 0 && b != 0 && type_name(a) == type_name(b))
}

#[derive(Clone, Copy, Debug)]
struct Base {
    td: usize,
    mdisp: i32,
    pdisp: i32,
    vdisp: i32,
    attrs: u32,
}

unsafe fn bases(col: usize, base: usize) -> Vec<Base> {
    let chd = rva(base, rd_i32(col + 16));
    let n = rd_u32(chd + 8) as usize;
    let arr = rva(base, rd_i32(chd + 12));
    (0..n.min(4096))
        .map(|i| {
            let bcd = rva(base, rd_i32(arr + i * 4));
            Base {
                td: rva(base, rd_i32(bcd)),
                mdisp: rd_i32(bcd + 8),
                pdisp: rd_i32(bcd + 12),
                vdisp: rd_i32(bcd + 16),
                attrs: rd_u32(bcd + 20),
            }
        })
        .collect()
}

/// Adresse du sous-objet `b` dans l'objet complet `obj`.
unsafe fn subobject(obj: usize, b: &Base) -> usize {
    let mut off = b.mdisp as isize;
    if b.pdisp >= 0 {
        let vbtable = rd_ptr(obj.wrapping_add(b.pdisp as isize as usize));
        off += rd_i32(vbtable.wrapping_add(b.vdisp as isize as usize)) as isize + b.pdisp as isize;
    }
    obj.wrapping_add(off as usize)
}

/// Résultat d'un dynamic_cast, ou 0.
unsafe fn dynamic_cast(inptr: usize, vf_delta: i32, _src: usize, target: usize) -> usize {
    if inptr == 0 {
        return 0;
    }
    let sub = inptr.wrapping_add(vf_delta as isize as usize);
    let Some((col, base)) = locator(sub) else {
        return 0;
    };
    let obj = complete_object(sub, col);
    let list = bases(col, base);
    // La base 0 est la classe complète elle-même.
    let mut found = None;
    for b in &list {
        if same_type(b.td, target) && b.attrs & (BCD_NOTVISIBLE | BCD_AMBIGUOUS) == 0 {
            found = Some(*b);
            break;
        }
    }
    match found {
        Some(b) => subobject(obj, &b),
        None => 0,
    }
}

unsafe extern "win64" fn rt_dynamic_cast(
    inptr: usize,
    vf_delta: i32,
    src: usize,
    target: usize,
    is_ref: i32,
) -> usize {
    let r = dynamic_cast(inptr, vf_delta, src, target);
    if r == 0 && inptr != 0 && is_ref != 0 {
        crate::log_warn!(
            "rtti",
            "dynamic_cast vers une référence impossible ({}) : std::bad_cast non levée",
            String::from_utf8_lossy(type_name(target))
        );
    }
    r
}

/// Résultat de `__RTtypeid` ; `THROW_BAD_TYPEID` quand il faut lever
/// std::bad_typeid (pointeur nul, programme x64).
unsafe extern "win64" fn typeid_of(inptr: usize) -> usize {
    if inptr == 0 {
        if crate::wow32::pointer_size() == 8 {
            return THROW_BAD_TYPEID;
        }
        crate::log_warn!(
            "rtti",
            "typeid sur un pointeur nul : std::bad_typeid non levée (x86)"
        );
        return 0;
    }
    match locator(inptr) {
        Some((col, base)) => rva(base, rd_i32(col + 12)),
        None => 0,
    }
}

const THROW_BAD_TYPEID: usize = usize::MAX;

/// `__RTtypeid` : type_info de l'objet complet ; `typeid(*p)` avec `p`
/// nul lève std::bad_typeid depuis le cadre de l'appelant, comme
/// vcruntime (#1658).
#[unsafe(naked)]
unsafe extern "win64" fn rt_typeid(_inptr: usize) -> usize {
    core::arch::naked_asm!(
        "sub rsp, 0x28",
        "call {typeid_of}",
        "add rsp, 0x28",
        "cmp rax, -1",
        "je 2f",
        "ret",
        "2:",
        "sub rsp, 0x28",
        "call {args}",
        "add rsp, 0x28",
        "mov ecx, 0xE06D7363",
        "mov edx, 1",
        "mov r8d, 4",
        "mov r9, rax",
        "jmp {raise}",
        typeid_of = sym typeid_of,
        args = sym bad_typeid_args,
        raise = sym super::seh::raise_exception,
    )
}

// --- std::bad_typeid lancée par __RTtypeid ------------------------------------

/// Objet lancé : disposition de std::exception (vptr, puis
/// __std_exception_data { what, dofree }).
#[repr(C)]
struct StdException {
    vptr: usize,
    what: *const u8,
    dofree: usize,
}

const BAD_TYPEID_WHAT: &[u8] = b"Attempted a typeid of nullptr pointer!\0";

/// Bloc de données RTTI et EH de std::bad_typeid, adressé par RVA depuis
/// son début (qui sert de « base d'image ») : TypeDescriptor, COL,
/// hiérarchie de classes, types capturables et ThrowInfo, au format x64
/// de vcruntime (`rttidata.h`, `ehdata.h`).
struct BadTypeid {
    base: usize,
    vtable: usize,
    throw_info: usize,
}

unsafe impl Send for BadTypeid {}
unsafe impl Sync for BadTypeid {}

unsafe extern "win64" fn exception_dtor(this: usize, flags: u32) -> usize {
    // Objet et chaîne statiques : rien à libérer.
    let _ = flags;
    this
}

unsafe extern "win64" fn exception_what(this: *const StdException) -> *const u8 {
    let w = (*this).what;
    if w.is_null() {
        c"Unknown exception".as_ptr().cast()
    } else {
        w
    }
}

fn bad_typeid_data() -> &'static BadTypeid {
    static DATA: std::sync::OnceLock<BadTypeid> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        // Disposition (décalages dans le bloc) :
        const TD_BAD: usize = 0x000; // TypeDescriptor bad_typeid (16 + nom)
        const TD_EXC: usize = 0x040; // TypeDescriptor exception
        const COL: usize = 0x080; // Complete Object Locator (24 octets)
        const CHD: usize = 0x0A0; // ClassHierarchyDescriptor (16)
        const BCA: usize = 0x0B0; // tableau des bases (2 RVA)
        const BCD0: usize = 0x0C0; // BaseClassDescriptor bad_typeid (28)
        const BCD1: usize = 0x0E0; // BaseClassDescriptor exception (28)
        const CT0: usize = 0x100; // CatchableType bad_typeid (28)
        const CT1: usize = 0x120; // CatchableType exception (28)
        const CTA: usize = 0x140; // CatchableTypeArray (4 + 2 × 4)
        const TI: usize = 0x150; // ThrowInfo (16)
        const VT: usize = 0x168; // [COL, destructeur, what]
        const SIZE: usize = 0x180;
        let block: &'static mut [u8] = Box::leak(vec![0u8; SIZE].into_boxed_slice());
        let base = block.as_mut_ptr() as usize;
        let put32 = |off: usize, v: u32| unsafe { ((base + off) as *mut u32).write_unaligned(v) };
        let put64 =
            |off: usize, v: usize| unsafe { ((base + off) as *mut usize).write_unaligned(v) };
        let name = |off: usize, n: &[u8]| unsafe {
            std::ptr::copy_nonoverlapping(n.as_ptr(), (base + off + 16) as *mut u8, n.len())
        };
        // TypeDescriptor : pVFTable (celle de type_info, ici le bloc
        // lui-même ; seul le nom est lu), spare, nom décoré.
        put64(TD_BAD, base + VT);
        name(TD_BAD, b".?AVbad_typeid@std@@");
        put64(TD_EXC, base + VT);
        name(TD_EXC, b".?AVexception@std@@");
        // COL signature 1 : offset 0, cdOffset 0, RVA TD, CHD, lui-même.
        put32(COL, 1);
        put32(COL + 12, TD_BAD as u32);
        put32(COL + 16, CHD as u32);
        put32(COL + 20, COL as u32);
        // CHD : signature 0, attributs 0, deux bases, RVA du tableau.
        put32(CHD + 8, 2);
        put32(CHD + 12, BCA as u32);
        put32(BCA, BCD0 as u32);
        put32(BCA + 4, BCD1 as u32);
        // BCD : RVA TD, bases contenues, PMD { 0, -1, 0 }, attributs, RVA CHD.
        for (bcd, td, contained) in [(BCD0, TD_BAD, 1u32), (BCD1, TD_EXC, 0)] {
            put32(bcd, td as u32);
            put32(bcd + 4, contained);
            put32(bcd + 12, u32::MAX);
            put32(bcd + 24, CHD as u32);
        }
        // CatchableType : propriétés CT_IsSimpleType (copie octet par
        // octet), RVA TD, PMD { 0, -1, 0 }, taille, pas de constructeur.
        for (ct, td) in [(CT0, TD_BAD), (CT1, TD_EXC)] {
            put32(ct, 1);
            put32(ct + 4, td as u32);
            put32(ct + 12, u32::MAX);
            put32(ct + 20, std::mem::size_of::<StdException>() as u32);
        }
        put32(CTA, 2);
        put32(CTA + 4, CT0 as u32);
        put32(CTA + 8, CT1 as u32);
        // ThrowInfo : attributs, pas de destructeur, compat, RVA du tableau.
        put32(TI + 12, CTA as u32);
        // vtable : COL juste avant, puis destructeur et what().
        put64(VT, base + COL);
        put64(VT + 8, exception_dtor as *const () as usize);
        put64(VT + 16, exception_what as *const () as usize);
        BadTypeid {
            base,
            vtable: base + VT + 8,
            throw_info: base + TI,
        }
    })
}

thread_local! {
    static BAD_TYPEID_ARGS: std::cell::RefCell<[usize; 4]> =
        const { std::cell::RefCell::new([0; 4]) };
}

/// Arguments de RaiseException pour std::bad_typeid : magie, objet,
/// ThrowInfo, base du bloc.
unsafe extern "win64" fn bad_typeid_args() -> *const usize {
    let d = bad_typeid_data();
    let obj = Box::leak(Box::new(StdException {
        vptr: d.vtable,
        what: BAD_TYPEID_WHAT.as_ptr(),
        dofree: 0,
    })) as *mut StdException as usize;
    BAD_TYPEID_ARGS.with(|a| {
        *a.borrow_mut() = [0x1993_0520, obj, d.throw_info, d.base];
        a.as_ptr() as *const usize
    })
}

unsafe extern "win64" fn rt_cast_to_void(inptr: usize) -> usize {
    if inptr == 0 {
        return 0;
    }
    match locator(inptr) {
        Some((col, _)) => complete_object(inptr, col),
        None => inptr,
    }
}

/// `__AdjustPointer(ptr, PMD*)` : sous-objet désigné par un PMD
/// { mdisp, pdisp, vdisp } (objets lancés et bases virtuelles).
unsafe extern "win64" fn adjust_pointer(ptr: usize, pmd: *const i32) -> usize {
    if pmd.is_null() {
        return ptr;
    }
    let b = Base {
        td: 0,
        mdisp: *pmd,
        pdisp: *pmd.add(1),
        vdisp: *pmd.add(2),
        attrs: 0,
    };
    subobject(ptr, &b)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "__AdjustPointer" => fn adjust_pointer,
        "__RTDynamicCast" => fn rt_dynamic_cast,
        "__RTtypeid" => fn rt_typeid,
        "__RTCastToVoid" => fn rt_cast_to_void,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_typeid_complete() {
        unsafe {
            assert_eq!(typeid_of(0), THROW_BAD_TYPEID);
            let args = bad_typeid_args();
            let a = std::slice::from_raw_parts(args, 4);
            assert_eq!(a[0], 0x1993_0520);
            // L'objet lancé a un RTTI lisible : typeid(e) donne bad_typeid.
            let td = typeid_of(a[1]);
            assert_eq!(type_name(td), b".?AVbad_typeid@std@@");
            // Le ThrowInfo propose bad_typeid puis exception.
            let cta = a[3] + rd_u32(a[2] + 12) as usize;
            assert_eq!(rd_i32(cta), 2);
            let ct1 = a[3] + rd_u32(cta + 8) as usize;
            assert_eq!(
                type_name(a[3] + rd_u32(ct1 + 4) as usize),
                b".?AVexception@std@@"
            );
            // what() rend le message de vcruntime.
            let what: unsafe extern "win64" fn(usize) -> *const u8 =
                std::mem::transmute(rd_ptr(rd_ptr(a[1]) + 8));
            assert_eq!(
                CStr::from_ptr(what(a[1]).cast()).to_bytes(),
                b"Attempted a typeid of nullptr pointer!"
            );
        }
    }

    /// Image factice : struct Derived : Base1, Base2 (héritage multiple).
    struct Image {
        mem: Vec<u8>,
    }

    impl Image {
        fn base(&self) -> usize {
            self.mem.as_ptr() as usize
        }
        fn w32(&mut self, off: usize, v: u32) {
            self.mem[off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
        fn w64(&mut self, off: usize, v: u64) {
            self.mem[off..off + 8].copy_from_slice(&v.to_le_bytes());
        }
        fn name(&mut self, off: usize, s: &str) {
            self.mem[off + 16..off + 16 + s.len()].copy_from_slice(s.as_bytes());
        }
    }

    #[test]
    fn transtypage_dynamique_heritage_multiple() {
        let mut im = Image {
            mem: vec![0u8; 0x1000],
        };
        // TypeDescriptors.
        let (td_d, td_b1, td_b2, td_x) = (0x100, 0x140, 0x180, 0x1c0);
        im.name(td_d, ".?AVDerived@@");
        im.name(td_b1, ".?AVBase1@@");
        im.name(td_b2, ".?AVBase2@@");
        im.name(td_x, ".?AVAutre@@");
        // BaseClassDescriptors : Derived (0), Base1 (0), Base2 (+16).
        for (i, (td, mdisp)) in [(td_d, 0u32), (td_b1, 0), (td_b2, 16)].iter().enumerate() {
            let bcd = 0x200 + i * 0x20;
            im.w32(bcd, *td as u32);
            im.w32(bcd + 8, *mdisp);
            im.w32(bcd + 12, u32::MAX); // pdisp -1
            im.w32(0x300 + i * 4, bcd as u32);
        }
        // ClassHierarchyDescriptor.
        im.w32(0x340, 0);
        im.w32(0x344, 1);
        im.w32(0x348, 3);
        im.w32(0x34c, 0x300);
        // Locateurs : objet complet (offset 0) et sous-objet Base2 (offset 16).
        for (col, offset) in [(0x400usize, 0u32), (0x420, 16)] {
            im.w32(col, 1);
            im.w32(col + 4, offset);
            im.w32(col + 12, td_d as u32);
            im.w32(col + 16, 0x340);
            im.w32(col + 20, col as u32);
        }
        // vtables : [-1] = locateur.
        let b = im.base() as u64;
        im.w64(0x500, b + 0x400);
        im.w64(0x520, b + 0x420);
        // Objet : vfptr Base1 à 0, vfptr Base2 à 16.
        im.w64(0x800, b + 0x508);
        im.w64(0x810, b + 0x528);
        let base = im.base();
        let obj = base + 0x800;
        unsafe {
            // Base2* → Base1* (transtypage croisé).
            assert_eq!(dynamic_cast(obj + 16, 0, base + td_b2, base + td_b1), obj);
            // Base1* → Base2*.
            assert_eq!(dynamic_cast(obj, 0, base + td_b1, base + td_b2), obj + 16);
            // Vers le type complet.
            assert_eq!(dynamic_cast(obj + 16, 0, base + td_b2, base + td_d), obj);
            // Type absent.
            assert_eq!(dynamic_cast(obj, 0, base + td_b1, base + td_x), 0);
            assert_eq!(rt_cast_to_void(obj + 16), obj);
            assert_eq!(rt_typeid(obj + 16), base + td_d);
            assert_eq!(dynamic_cast(0, 0, 0, base + td_b1), 0);
        }
    }
}
