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

unsafe extern "win64" fn rt_typeid(inptr: usize) -> usize {
    if inptr == 0 {
        crate::log_warn!(
            "rtti",
            "typeid sur un pointeur nul : std::bad_typeid non levée"
        );
        return 0;
    }
    match locator(inptr) {
        Some((col, base)) => rva(base, rd_i32(col + 12)),
        None => 0,
    }
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
