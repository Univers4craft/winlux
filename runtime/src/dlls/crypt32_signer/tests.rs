use super::*;

type OpenFn = unsafe extern "win64" fn(u32, u32, u32, usize, usize, *const u8) -> usize;
type UpdateFn = unsafe extern "win64" fn(usize, *const u8, u32, i32) -> i32;
type GetParamFn = unsafe extern "win64" fn(usize, u32, u32, *mut u8, *mut u32) -> i32;
type CloseFn = unsafe extern "win64" fn(usize) -> i32;

unsafe fn export<T>(name: &str) -> T {
    let a = crate::dlls::lookup("crypt32", name).unwrap_or_else(|| panic!("{name}"));
    std::mem::transmute_copy(&a)
}

unsafe fn rd32(b: *const u8, at: usize) -> u32 {
    b.add(at).cast::<u32>().read_unaligned()
}

unsafe fn rd_ptr(b: *const u8, at: usize) -> *const u8 {
    b.add(at).cast::<usize>().read_unaligned() as *const u8
}

unsafe fn blob<'a>(b: *const u8, at: usize) -> &'a [u8] {
    let cb = rd32(b, at) as usize;
    if cb == 0 {
        return &[];
    }
    std::slice::from_raw_parts(rd_ptr(b, at + 8), cb)
}

unsafe fn sz(p: *const u8) -> String {
    std::ffi::CStr::from_ptr(p.cast())
        .to_string_lossy()
        .into_owned()
}

/// #2689 : CryptMsgGetParam(CMSG_SIGNER_INFO_PARAM) rend le SignerInfo
/// d'un message signé, comme le lit un programme qui vérifie l'éditeur
/// d'un module signé (CryptQueryObject puis CryptMsgGetParam).
#[test]
fn informations_du_signataire_rendues() {
    // SpcIndirectDataContent quelconque : seul le SignerInfo compte ici.
    let spc = [0x30, 0x03, 0x02, 0x01, 0x00];
    let Some(sig) = crate::setup::appx_signature::tests::openssl_sign_spc(&spc, "signataire")
    else {
        return;
    };
    unsafe {
        let open: OpenFn = export("CryptMsgOpenToDecode");
        let update: UpdateFn = export("CryptMsgUpdate");
        let get: GetParamFn = export("CryptMsgGetParam");
        let close: CloseFn = export("CryptMsgClose");
        let msg = open(1 | 0x10000, 0, 0, 0, 0, std::ptr::null());
        assert_ne!(msg, 0);
        assert_eq!(update(msg, sig.as_ptr(), sig.len() as u32, 1), 1);

        // Taille demandée, puis tampon trop petit.
        let mut len = 0u32;
        assert_eq!(get(msg, 6, 0, std::ptr::null_mut(), &mut len), 1);
        assert!(len as usize > 136);
        let mut small = len - 1;
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let b = buf.as_mut_ptr().cast::<u8>();
        assert_eq!(get(msg, 6, 0, b, &mut small), 0);
        assert_eq!(small, len);
        assert_eq!(get(msg, 6, 0, b, &mut len), 1);

        // Version 1 : issuerAndSerialNumber.
        assert_eq!(rd32(b, 0), 1);
        // Émetteur : Name DER du certificat auto-signé.
        let issuer = blob(b, 8);
        assert_eq!(issuer.first(), Some(&0x30));
        assert_eq!(
            super::super::crypt32::name_to_str(issuer, false),
            "CN=Testeur Winlux, O=Essai"
        );
        // Numéro de série petit-boutiste, comme dans le certificat.
        let mut cert_len = 0u32;
        assert_eq!(get(msg, 12, 0, std::ptr::null_mut(), &mut cert_len), 1);
        let mut cert = vec![0u8; cert_len as usize];
        assert_eq!(get(msg, 12, 0, cert.as_mut_ptr(), &mut cert_len), 1);
        let (c, _) = tlv(&cert).unwrap();
        let (tbs, _) = tlv(c.body).unwrap();
        let serial = children(tbs.body)
            .into_iter()
            .find(|t| t.tag == 0x02)
            .unwrap();
        let le: Vec<u8> = serial.body.iter().rev().copied().collect();
        assert_eq!(blob(b, 24), le.as_slice());
        // Condensat SHA-256, signature RSA de 2048 bits.
        assert_eq!(sz(rd_ptr(b, 40)), "2.16.840.1.101.3.4.2.1");
        assert_eq!(sz(rd_ptr(b, 64)), "1.2.840.113549.1.1.1");
        assert_eq!(blob(b, 88).len(), 256);
        // Attributs authentifiés : messageDigest, valeur OCTET STRING de
        // 32 octets.
        let n = rd32(b, 104) as usize;
        let rg = rd_ptr(b, 112);
        let digest = (0..n)
            .map(|i| rg.add(24 * i))
            .find(|a| sz(rd_ptr(*a, 0)) == "1.2.840.113549.1.9.4")
            .expect("messageDigest");
        assert_eq!(rd32(digest, 8), 1);
        let value = blob(rd_ptr(digest, 16), 0);
        assert_eq!(value[..2], [0x04, 0x20]);
        // Pas d'attribut non authentifié.
        assert_eq!(rd32(b, 120), 0);

        // Un seul signataire.
        let mut len = 0u32;
        assert_eq!(get(msg, 6, 1, std::ptr::null_mut(), &mut len), 0);
        assert_eq!(close(msg), 1);

        // x86 : 68 octets de structure, pointeurs de 4 octets dans le tampon.
        let (bytes, fixups) = signer_info(&sig, 0, 4).unwrap();
        assert!(fixups
            .iter()
            .all(|&(at, t)| at >= 4 && at + 4 <= bytes.len() && t >= 68));
        assert_eq!(bytes[..4], [1, 0, 0, 0]);
        assert_eq!(u32::from_le_bytes(bytes[44..48].try_into().unwrap()), 256);
    }
}
