use super::*;

/// BMP 24 bits de 2 × 1 pixels (rouge, bleu), en-tête BITMAPINFOHEADER.
fn tiny_bmp() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&62u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&2i32.to_le_bytes());
    b.extend_from_slice(&1i32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0u8; 24]);
    b.extend_from_slice(&[0, 0, 255, 255, 0, 0, 0, 0]);
    b
}

unsafe fn new_stream_with(bytes: &[u8]) -> usize {
    let mut s: *mut std::ffi::c_void = std::ptr::null_mut();
    assert_eq!(
        super::super::ole32::create_stream_on_hglobal(0, 1, &mut s),
        S_OK
    );
    let s = s as usize;
    assert_eq!(stream_write(s, bytes), S_OK);
    let seek: unsafe extern "win64" fn(usize, i64, u32, *mut u64) -> i32 =
        std::mem::transmute(slot(s, 5));
    assert_eq!(seek(s, 0, 0, std::ptr::null_mut()), S_OK);
    s
}

#[test]
fn image_chargee_d_un_flux_puis_relue() {
    unsafe {
        let bmp = tiny_bmp();
        let stm = new_stream_with(&bmp);
        let mut p = 0usize;
        assert_eq!(
            ole_load_picture(stm, bmp.len() as i32, 0, &IID_IPICTURE, &mut p),
            S_OK
        );
        let mut kind = 0i16;
        assert_eq!(pic_get_type(p, &mut kind), S_OK);
        assert_eq!(kind, PICTYPE_BITMAP);
        let (mut w, mut h) = (0, 0);
        pic_get_width(p, &mut w);
        pic_get_height(p, &mut h);
        assert_eq!((w, h), (himetric(2), himetric(1)));
        let mut handle = 0u32;
        assert_eq!(pic_get_handle(p, &mut handle), S_OK);
        assert_ne!(handle, 0);

        // IPictureDisp : propriété Type par Invoke.
        let mut d = 0usize;
        assert_eq!(pic_qi(p, &IID_IPICTURE_DISP, &mut d), S_OK);
        assert_eq!(d, p + 8);
        let mut v = [0u8; VARIANT_SIZE];
        assert_eq!(
            picd_invoke(
                d,
                3,
                &IID_IUNKNOWN,
                0,
                DISPATCH_PROPERTYGET,
                std::ptr::null(),
                v.as_mut_ptr(),
                0,
                std::ptr::null_mut()
            ),
            S_OK
        );
        assert_eq!(
            std::ptr::read_unaligned(v.as_ptr().add(8) as *const i16),
            PICTYPE_BITMAP
        );
        picd_release(d);

        // SaveAsFile rend les octets d'origine.
        let out = new_stream_with(&[]);
        let mut size = 0i32;
        assert_eq!(pic_save_as_file(p, out, 1, &mut size), S_OK);
        assert_eq!(size as usize, bmp.len());
        assert_eq!(pic_release(p), 0);
        release(stm);
        release(out);

        // Données qui ne sont pas une image.
        let junk = new_stream_with(b"pas une image");
        assert_eq!(
            ole_load_picture(junk, 0, 0, &IID_IPICTURE, &mut p),
            CTL_E_INVALIDPICTURE
        );
        assert_eq!(p, 0);
        release(junk);

        let missing: Vec<u16> = "C:\\Essai\\absente-1511.bmp\0".encode_utf16().collect();
        assert_eq!(
            ole_load_picture_path(missing.as_ptr(), 0, 0, 0, &IID_IPICTURE, &mut p),
            CTL_E_PATHFILEACCESSERROR
        );
        let url: Vec<u16> = "https://exemple.invalid/a.bmp\0".encode_utf16().collect();
        assert_eq!(
            ole_load_picture_path(url.as_ptr(), 0, 0, 0, &IID_IPICTURE, &mut p),
            E_NOTIMPL
        );
    }
}

#[test]
fn image_creee_depuis_un_handle_et_vide() {
    unsafe {
        let mut p = 0usize;
        assert_eq!(
            ole_create_picture_indirect(std::ptr::null(), &IID_IPICTURE, 0, &mut p),
            S_OK
        );
        let mut kind = 0i16;
        pic_get_type(p, &mut kind);
        assert_eq!(kind, PICTYPE_UNINITIALIZED);
        pic_release(p);

        let mut desc = [0u8; 24];
        desc[0..4].copy_from_slice(&24u32.to_le_bytes());
        desc[4..8].copy_from_slice(&(PICTYPE_NONE as u32).to_le_bytes());
        assert_eq!(
            ole_create_picture_indirect(desc.as_ptr(), &IID_IDISPATCH, 0, &mut p),
            S_OK
        );
        picd_release(p);

        // Bitmap inconnue : refusée.
        desc[4..8].copy_from_slice(&(PICTYPE_BITMAP as u32).to_le_bytes());
        desc[8..16].copy_from_slice(&0x7FFF_FFF0usize.to_le_bytes());
        assert_eq!(
            ole_create_picture_indirect(desc.as_ptr(), &IID_IPICTURE, 0, &mut p),
            E_INVALIDARG
        );
        assert_eq!(p, 0);
    }
}

#[test]
fn police_ole_proprietes_clone_et_dispatch() {
    unsafe {
        let name: Vec<u16> = "Arial\0".encode_utf16().collect();
        let mut desc = [0u8; 40];
        desc[0..4].copy_from_slice(&40u32.to_le_bytes());
        desc[8..16].copy_from_slice(&(name.as_ptr() as usize).to_le_bytes());
        desc[16..24].copy_from_slice(&120_000i64.to_le_bytes()); // 12 pt
        desc[24..26].copy_from_slice(&700i16.to_le_bytes());
        desc[28..32].copy_from_slice(&1i32.to_le_bytes());
        let mut f = 0usize;
        assert_eq!(
            ole_create_font_indirect(desc.as_ptr(), &IID_IFONT, &mut f),
            S_OK
        );

        let mut b = 0i32;
        font_get_bold(f, &mut b);
        assert_eq!(b, 1);
        font_get_italic(f, &mut b);
        assert_eq!(b, 1);
        let mut size = 0i64;
        font_get_size(f, &mut size);
        assert_eq!(size, 120_000);
        let mut bs: *mut u16 = std::ptr::null_mut();
        font_get_name(f, &mut bs);
        assert_eq!(super::super::oleaut32_typelib::wide(bs), "Arial");
        super::super::oleaut32::sys_free_string(bs);

        let st = lock(&font(f).state).clone();
        assert_eq!(font_height(&st), -16, "12 pt à 96 ppp");

        let mut c = 0usize;
        assert_eq!(font_clone(f, &mut c), S_OK);
        assert_eq!(font_is_equal(f, c), S_OK);
        assert_eq!(font_put_bold(c, 0), S_OK);
        assert_eq!(font_is_equal(f, c), S_FALSE);
        let mut weight = 0i16;
        font_get_weight(c, &mut weight);
        assert_eq!(weight, 400);

        // IFontDisp : Name par Invoke, puis Size en écriture.
        let mut d = 0usize;
        assert_eq!(font_qi(f, &IID_IFONT_DISP, &mut d), S_OK);
        let mut v = [0u8; VARIANT_SIZE];
        assert_eq!(
            fontd_invoke(
                d,
                0,
                &IID_IUNKNOWN,
                0,
                DISPATCH_PROPERTYGET,
                std::ptr::null(),
                v.as_mut_ptr(),
                0,
                std::ptr::null_mut()
            ),
            S_OK
        );
        assert_eq!(std::ptr::read_unaligned(v.as_ptr() as *const u16), VT_BSTR);
        variant_clear(v.as_mut_ptr().cast());
        let mut arg = [0u8; VARIANT_SIZE];
        std::ptr::write_unaligned(arg.as_mut_ptr() as *mut u16, VT_I4);
        std::ptr::write_unaligned(arg.as_mut_ptr().add(8) as *mut i32, 9);
        let mut params = [0u8; 24];
        params[0..8].copy_from_slice(&(arg.as_ptr() as usize).to_le_bytes());
        params[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            fontd_invoke(
                d,
                2,
                &IID_IUNKNOWN,
                0,
                DISPATCH_PROPERTYPUT,
                params.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut()
            ),
            S_OK
        );
        font_get_size(f, &mut size);
        assert_eq!(size, 90_000);
        let names = [name.as_ptr()];
        let mut id = 0i32;
        let size_name: Vec<u16> = "size\0".encode_utf16().collect();
        let size_names = [size_name.as_ptr()];
        assert_eq!(
            fontd_ids_of_names(d, std::ptr::null(), size_names.as_ptr(), 1, 0, &mut id),
            S_OK
        );
        assert_eq!(id, 2);
        assert_eq!(
            fontd_ids_of_names(d, std::ptr::null(), names.as_ptr(), 1, 0, &mut id),
            DISP_E_UNKNOWNNAME
        );
        fontd_release(d);
        assert_eq!(font_release(c), 0);
        assert_eq!(font_release(f), 0);
    }
}
