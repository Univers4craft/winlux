use super::*;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

const IID_ITYPEINFO: Guid = guid(0x0002_0401, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const GUID_POINT: Guid = guid(0x1511_0010, 0x1511, 0x1511, [1, 2, 3, 4, 5, 6, 7, 9]);

/// Structure « Point » { BSTR Nom @0 ; long Age @8 } dans une bibliothèque
/// créée par CreateTypeLib2 ; rend son ITypeInfo.
unsafe fn point_type() -> usize {
    let file = w("C:\\Essai\\lot1511.tlb");
    let mut ctl = 0usize;
    assert_eq!(
        super::super::oleaut32_typelib::create_type_lib2(3, file.as_ptr(), &mut ctl),
        S_OK
    );
    let create_ti: unsafe extern "win64" fn(usize, *const u16, u32, *mut usize) -> i32 =
        std::mem::transmute(slot(ctl, 3));
    let name = w("Point");
    let mut cti = 0usize;
    assert_eq!(create_ti(ctl, name.as_ptr(), TKIND_RECORD, &mut cti), S_OK);
    let set_guid: unsafe extern "win64" fn(usize, *const Guid) -> i32 =
        std::mem::transmute(slot(cti, 3));
    assert_eq!(set_guid(cti, &GUID_POINT), S_OK);
    let add_var: unsafe extern "win64" fn(usize, u32, *const u8) -> i32 =
        std::mem::transmute(slot(cti, 14));
    let set_name: unsafe extern "win64" fn(usize, u32, *const u16) -> i32 =
        std::mem::transmute(slot(cti, 16));
    for (i, (field, offset, vt)) in [("Nom", 0u32, 8u16), ("Age", 8, 3)].into_iter().enumerate() {
        // VARDESC aligné sur 8 comme celui d'un vrai appelant.
        let mut words = [0u64; 8];
        let vd = words.as_mut_ptr() as *mut u8;
        std::ptr::write_unaligned(vd as *mut i32, MEMBERID_NIL);
        std::ptr::write_unaligned(vd.add(16) as *mut u32, offset);
        std::ptr::write_unaligned(vd.add(32) as *mut u16, vt);
        assert_eq!(add_var(cti, i as u32, vd), S_OK);
        let n = w(field);
        assert_eq!(set_name(cti, i as u32, n.as_ptr()), S_OK);
    }
    let mut ti = 0usize;
    assert_eq!(
        super::super::ole32_lot_c::query(cti, &IID_ITYPEINFO, &mut ti),
        S_OK
    );
    release(cti);
    release(ctl);
    ti
}

unsafe fn vt_of(v: &[u8; VARIANT_SIZE]) -> u16 {
    std::ptr::read_unaligned(v.as_ptr() as *const u16)
}

#[test]
fn structure_decrite_lue_ecrite_et_copiee() {
    unsafe {
        let ti = point_type();
        let mut rec = 0usize;
        assert_eq!(get_record_info_from_type_info(ti, &mut rec), S_OK);
        let mut size = 0u32;
        assert_eq!(ri_get_size(rec, &mut size), S_OK);
        assert!(size >= 12);
        let mut g = [0u8; 16];
        assert_eq!(ri_get_guid(rec, &mut g), S_OK);
        assert_eq!(g, GUID_POINT);
        let mut name: *mut u16 = std::ptr::null_mut();
        assert_eq!(ri_get_name(rec, &mut name), S_OK);
        assert_eq!(wide(name), "Point");
        sys_free_string(name);

        let mut count = 0u32;
        assert_eq!(
            ri_get_field_names(rec, &mut count, std::ptr::null_mut()),
            S_OK
        );
        assert_eq!(count, 2);

        // PutField avec conversion : chaîne « 42 » vers le champ long.
        let data = ri_record_create(rec);
        assert!(!data.is_null());
        let mut v = crate::dlls::comobj::Aligned::<VARIANT_SIZE>::default();
        let texte = w("42");
        std::ptr::write_unaligned(v.as_mut_ptr() as *mut u16, 8);
        std::ptr::write_unaligned(
            v.as_mut_ptr().add(8) as *mut *mut u16,
            sys_alloc_string_len(texte.as_ptr(), 2),
        );
        let age = w("age");
        assert_eq!(ri_put_field(rec, 1, data, age.as_ptr(), v.as_ptr()), S_OK);
        let nom = w("Nom");
        assert_eq!(ri_put_field(rec, 1, data, nom.as_ptr(), v.as_ptr()), S_OK);
        variant_clear(v.as_mut_ptr().cast());
        assert_eq!(std::ptr::read_unaligned(data.add(8) as *const i32), 42);

        let mut out = crate::dlls::comobj::Aligned::<VARIANT_SIZE>::default();
        assert_eq!(
            ri_get_field(rec, data, age.as_ptr(), out.as_mut_ptr()),
            S_OK
        );
        assert_eq!(vt_of(&out), 3);
        assert_eq!(
            std::ptr::read_unaligned(out.as_ptr().add(8) as *const i32),
            42
        );

        // Copie profonde : la chaîne est dupliquée.
        let mut copy: *mut u8 = std::ptr::null_mut();
        assert_eq!(ri_record_create_copy(rec, data, &mut copy), S_OK);
        let a = std::ptr::read_unaligned(data as *const *const u16);
        let b = std::ptr::read_unaligned(copy as *const *const u16);
        assert!(!a.is_null() && a != b);
        assert_eq!(wide(b), "42");
        assert_eq!(
            ri_get_field(rec, copy, nom.as_ptr(), out.as_mut_ptr()),
            S_OK
        );
        assert_eq!(vt_of(&out), 8);
        variant_clear(out.as_mut_ptr().cast());

        let inconnu = w("Taille");
        assert_eq!(
            ri_get_field(rec, data, inconnu.as_ptr(), out.as_mut_ptr()),
            TYPE_E_FIELDNOTFOUND
        );
        assert_eq!(ri_is_matching_type(rec, rec), 1);

        // Tableau de structures : SafeArrayCreateEx, puis RecordClear à la
        // destruction.
        let bound = [2u32, 0u32];
        let sa = super::super::oleaut32::safe_array_create_ex(36, 1, bound.as_ptr() as usize, rec);
        assert_ne!(sa, 0);
        let mut got = 0usize;
        assert_eq!(
            super::super::oleaut32::safe_array_get_record_info(sa, &mut got),
            S_OK
        );
        assert_eq!(got, rec);
        ri_release(got);
        assert_eq!(super::super::oleaut32::safe_array_get_elem_size(sa), size);
        assert_eq!(super::super::oleaut32::safe_array_destroy(sa), 0);

        ri_record_destroy(rec, copy);
        ri_record_destroy(rec, data);
        assert_eq!(ri_release(rec), 0);

        // Un type qui n'est pas une structure est refusé.
        let mut none = 1usize;
        assert_eq!(get_record_info_from_type_info(0, &mut none), E_INVALIDARG);
        assert_eq!(none, 0);
        release(ti);
    }
}
