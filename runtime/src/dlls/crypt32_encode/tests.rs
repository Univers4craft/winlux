use super::*;
use crate::dlls::crypt32::{children, name_attrs, name_to_str, tlv};

#[test]
fn oid_et_noms_x500() {
    assert_eq!(
        oid_der("1.2.840.113549").unwrap(),
        [0x06, 0x06, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d]
    );
    assert!(oid_der("3.1").is_none());
    let der = str_to_name("CN=Winlux, O=\"A, B\"; OU=Banc+E=a@b.fr", 0).unwrap();
    let attrs = name_attrs(&der);
    assert_eq!(
        attrs,
        [
            ("2.5.4.3".to_string(), "Winlux".to_string()),
            ("2.5.4.10".to_string(), "A, B".to_string()),
            ("2.5.4.11".to_string(), "Banc".to_string()),
            ("1.2.840.113549.1.9.1".to_string(), "a@b.fr".to_string()),
        ]
    );
    // OU et E partagent un même RDN (SET).
    let (outer, _) = tlv(&der).unwrap();
    assert_eq!(children(outer.body).len(), 3);
    assert_eq!(
        name_to_str(&der, false),
        "CN=Winlux, O=\"A, B\", OU=Banc, E=a@b.fr"
    );
    // Une clé inconnue est signalée à sa position.
    assert_eq!(str_to_name("CN=a, XX=b", 0), Err(6));
    assert_eq!(str_to_name("CN=\"a", 0), Err(3));
    let rev = str_to_name("CN=a, O=b", CERT_NAME_STR_REVERSE_FLAG).unwrap();
    assert_eq!(name_to_str(&rev, false), "O=b, CN=a");
    // Hors PrintableString : BMPString.
    let e = str_to_name("CN=Été", 0).unwrap();
    assert!(e.windows(2).any(|w| w == [0x1e, 0x06]));
    assert_eq!(name_attrs(&e)[0].1, "Été");
}

#[test]
fn cert_str_to_name_a() {
    let _teb = crate::process::test_teb();
    unsafe {
        let text = c"CN=Winlux, C=FR";
        let mut len = 0u32;
        let mut err = 1usize;
        assert_eq!(
            CertStrToNameA(
                1,
                text.as_ptr().cast(),
                3,
                0,
                std::ptr::null_mut(),
                &mut len,
                &mut err
            ),
            1
        );
        assert_eq!(err, 0);
        let mut buf = vec![0u8; len as usize];
        let mut small = 2u32;
        assert_eq!(
            CertStrToNameA(
                1,
                text.as_ptr().cast(),
                3,
                0,
                buf.as_mut_ptr(),
                &mut small,
                &mut err
            ),
            0
        );
        assert_eq!(crate::process::last_error(), ERROR_MORE_DATA);
        assert_eq!(
            CertStrToNameA(
                1,
                text.as_ptr().cast(),
                3,
                0,
                buf.as_mut_ptr(),
                &mut len,
                &mut err
            ),
            1
        );
        assert_eq!(name_to_str(&buf, false), "CN=Winlux, C=FR");
        let bad = c"CN=a, ZZ=b";
        assert_eq!(
            CertStrToNameA(
                1,
                bad.as_ptr().cast(),
                3,
                0,
                buf.as_mut_ptr(),
                &mut len,
                &mut err
            ),
            0
        );
        assert_eq!(crate::process::last_error(), CRYPT_E_INVALID_X500_STRING);
        assert_eq!(err, bad.as_ptr() as usize + 6);
    }
}

unsafe fn encode(ty: usize, info: usize) -> Vec<u8> {
    let mut len = 0u32;
    assert_eq!(
        CryptEncodeObjectEx(1, ty, info, 0, 0, std::ptr::null_mut(), &mut len),
        1
    );
    let mut v = vec![0u8; len as usize];
    assert_eq!(
        CryptEncodeObjectEx(1, ty, info, 0, 0, v.as_mut_ptr(), &mut len),
        1
    );
    v
}

#[test]
fn crypt_encode_object_ex() {
    let _teb = crate::process::test_teb();
    unsafe {
        let bc = [1u32, 1, 0];
        assert_eq!(
            encode(X509_BASIC_CONSTRAINTS2, bc.as_ptr() as usize),
            [0x30, 0x06, 0x01, 0x01, 0xff, 0x02, 0x01, 0x00]
        );
        let i = 128i32;
        assert_eq!(
            encode(X509_INTEGER, &i as *const i32 as usize),
            [0x02, 0x02, 0x00, 0x80]
        );
        let m = -1i32;
        assert_eq!(
            encode(X509_INTEGER, &m as *const i32 as usize),
            [0x02, 0x01, 0xff]
        );
        let ku = [0xa0u8, 0x00];
        let ku_blob = [2u64, ku.as_ptr() as u64, 0];
        assert_eq!(
            encode(X509_KEY_USAGE, ku_blob.as_ptr() as usize),
            [0x03, 0x02, 0x05, 0xa0]
        );
        let oid = c"1.3.6.1.5.5.7.3.1";
        let list = [oid.as_ptr() as u64];
        let eku = [1u64, list.as_ptr() as u64];
        let eku_oid = c"2.5.29.37";
        assert_eq!(
            encode(eku_oid.as_ptr() as usize, eku.as_ptr() as usize),
            [0x30, 0x0a, 0x06, 0x08, 0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01]
        );
        let dns: Vec<u16> = "a.b\0".encode_utf16().collect();
        let entry = [3u64, dns.as_ptr() as u64, 0];
        let alt = [1u64, entry.as_ptr() as u64];
        assert_eq!(
            encode(X509_ALTERNATE_NAME, alt.as_ptr() as usize),
            [0x30, 0x05, 0x82, 0x03, 0x61, 0x2e, 0x62]
        );
        // CERT_NAME_INFO : un RDN, un attribut CN en PrintableString.
        let cn = c"2.5.4.3";
        let val = b"Banc";
        let attr = [cn.as_ptr() as u64, 5, 4, val.as_ptr() as u64];
        let rdn = [1u64, attr.as_ptr() as u64];
        let info = [1u64, rdn.as_ptr() as u64];
        let name = encode(X509_NAME, info.as_ptr() as usize);
        assert_eq!(name_to_str(&name, false), "CN=Banc");
        // Allocation par LocalAlloc.
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        assert_eq!(
            CryptEncodeObjectEx(
                1,
                X509_INTEGER,
                &i as *const i32 as usize,
                CRYPT_ENCODE_ALLOC_FLAG,
                0,
                (&mut p as *mut *mut u8).cast(),
                &mut len
            ),
            1
        );
        assert_eq!(
            std::slice::from_raw_parts(p, len as usize),
            [0x02, 0x02, 0x00, 0x80]
        );
        let mut len = 0u32;
        assert_eq!(
            CryptEncodeObjectEx(
                1,
                999,
                bc.as_ptr() as usize,
                0,
                0,
                std::ptr::null_mut(),
                &mut len
            ),
            0
        );
        assert_eq!(crate::process::last_error(), CRYPT_E_NOT_FOUND);
    }
}

#[test]
fn condensat_et_certificat_auto_signe() {
    let _teb = crate::process::test_teb();
    if crate::dlls::hostssl::ssl().is_none() {
        return;
    }
    unsafe {
        let alg: Vec<u16> = "SHA256\0".encode_utf16().collect();
        let mut len = 0u32;
        assert_eq!(
            CryptHashCertificate2(
                alg.as_ptr(),
                0,
                0,
                b"abc".as_ptr(),
                3,
                std::ptr::null_mut(),
                &mut len
            ),
            1
        );
        assert_eq!(len, 32);
        let mut h = [0u8; 32];
        assert_eq!(
            CryptHashCertificate2(
                alg.as_ptr(),
                0,
                0,
                b"abc".as_ptr(),
                3,
                h.as_mut_ptr(),
                &mut len
            ),
            1
        );
        assert_eq!(&h[..4], &[0xba, 0x78, 0x16, 0xbf]);
        let bad: Vec<u16> = "XYZ\0".encode_utf16().collect();
        assert_eq!(
            CryptHashCertificate2(
                bad.as_ptr(),
                0,
                0,
                b"abc".as_ptr(),
                3,
                h.as_mut_ptr(),
                &mut len
            ),
            0
        );
        assert_eq!(crate::process::last_error(), NTE_BAD_ALGID);

        let name = str_to_name("CN=Winlux banc", 0).unwrap();
        let subject = [name.len() as u64, name.as_ptr() as u64];
        let sha256 = c"1.2.840.113549.1.1.11";
        let sig_alg = [sha256.as_ptr() as u64, 0, 0];
        let ctx = CertCreateSelfSignCertificate(
            0,
            subject.as_ptr() as usize,
            0,
            0,
            sig_alg.as_ptr() as usize,
            0,
            0,
            0,
        );
        assert_ne!(ctx, 0);
        let c = crate::dlls::crypt32::context(ctx);
        let der = std::slice::from_raw_parts(c.der, c.der_len as usize);
        let info = &*c.info;
        let subj = std::slice::from_raw_parts(info.subject.pb, info.subject.cb as usize);
        assert_eq!(name_to_str(subj, false), "CN=Winlux banc");
        assert!(crate::dlls::crypt32::private_key(ctx).is_some());
        // La signature se vérifie avec la clé publique du certificat.
        let (outer, _) = tlv(der).unwrap();
        let parts = children(outer.body);
        assert_eq!(parts.len(), 3);
        assert_eq!(
            crate::dlls::hostssl::verify_signature(
                der,
                c"SHA256",
                &parts[2].body[1..],
                parts[0].all
            ),
            Some(true)
        );
        // Disposition x86 (#1667) : CERT_CONTEXT de 20 octets, CERT_INFO de 112.
        let (v, i) = crate::dlls::crypt32::x86_view(ctx);
        assert_eq!(v[0], c.encoding);
        assert_eq!(v[2], c.der_len);
        assert_eq!(i[0], info.version);
        assert_eq!(i[12], info.subject.cb);
        assert_eq!(i[26], info.ext_count);
        assert_eq!(std::mem::size_of_val(&i), 112);
        // Chaîne en disposition x86 (#1708) : 56, 32 et 32 octets.
        let mut chain = 0usize;
        assert_eq!(
            crate::dlls::crypt32::CertGetCertificateChain(0, ctx, 0, 0, 0, 0, 0, &mut chain),
            1
        );
        let (cc, simple, elements) = crate::dlls::crypt32::chain_x86_view(chain);
        assert_eq!((cc[0], cc[3]), (56, 1));
        assert_eq!(simple[0], 32);
        assert_eq!(simple[3] as usize, elements.len());
        assert!(!elements.is_empty() && elements.iter().all(|e| e[0] == 32 && e[1] != 0));
        assert_eq!(cc[1], simple[1]);
        crate::dlls::crypt32::CertFreeCertificateChain(chain);
        crate::dlls::crypt32::CertFreeCertificateContext(ctx);
    }
}
