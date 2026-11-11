use super::*;

#[test]
fn luid_guid_et_noms() {
    unsafe {
        let mut luid = 0u64;
        let name: Vec<u16> = "ethernet_7\0".encode_utf16().collect();
        assert_eq!(
            ConvertInterfaceNameToLuidW(name.as_ptr(), &mut luid),
            ERROR_SUCCESS
        );
        let mut index = 0u32;
        assert_eq!(
            ConvertInterfaceLuidToIndex(&luid, &mut index),
            ERROR_SUCCESS
        );
        assert_eq!(index, 7);
        let bad: Vec<u16> = "inexistante-xyz\0".encode_utf16().collect();
        assert_eq!(
            ConvertInterfaceNameToLuidW(bad.as_ptr(), &mut luid),
            ERROR_INVALID_NAME
        );
        let g = parse_guid("{01234567-89AB-CDEF-0123-456789ABCDEF}").unwrap();
        assert_eq!(&g[..4], &[0x67, 0x45, 0x23, 0x01]);
        assert_eq!(&g[8..10], &[0x01, 0x23]);
        // Une interface réelle, si la machine en a une.
        if let Some(a) = crate::dlls::iphlpapi::adapters()
            .into_iter()
            .find(|a| a.index != 0)
        {
            let luid = (6u64 << 48) | ((a.index as u64) << 24);
            let mut guid = [0u8; 16];
            assert_eq!(
                ConvertInterfaceLuidToGuid(&luid, guid.as_mut_ptr()),
                ERROR_SUCCESS
            );
            assert_eq!(
                Some(guid),
                parse_guid(&crate::dlls::iphlpapi::adapter_guid(&a))
            );
        }
    }
}

#[test]
fn ordre_des_octets_et_chaines() {
    let _teb = crate::process::test_teb();
    unsafe {
        let mut l = 0u32;
        assert_eq!(WSAHtonl(0, 0x0102_0304, &mut l), 0);
        assert_eq!(l.to_ne_bytes(), [1, 2, 3, 4]);
        let mut back = 0u32;
        assert_eq!(WSANtohl(0, l, &mut back), 0);
        assert_eq!(back, 0x0102_0304);
        let mut s = 0u16;
        assert_eq!(WSANtohs(0, 0x0201u16.to_be(), &mut s), 0);
        assert_eq!(s, 0x0201);
        assert_eq!(WSAHtonl(0, 1, std::ptr::null_mut()), -1);

        let text = c"Corsair iCUE";
        let p = StrStrIA(text.as_ptr().cast(), c"ICUE".as_ptr().cast());
        assert_eq!(p as usize - text.as_ptr() as usize, 8);
        assert!(StrStrIA(text.as_ptr().cast(), c"xyz".as_ptr().cast()).is_null());

        let src: Vec<u16> = "Texte direct\0".encode_utf16().collect();
        let mut out = [0u16; 32];
        assert_eq!(
            SHLoadIndirectString(src.as_ptr(), out.as_mut_ptr(), 32, 0),
            S_OK
        );
        assert_eq!(crate::dlls::wstr(out.as_ptr()), "Texte direct");
        let mut court = [0u16; 4];
        assert_eq!(
            SHLoadIndirectString(src.as_ptr(), court.as_mut_ptr(), 4, 0),
            STRSAFE_E_INSUFFICIENT_BUFFER
        );
        let ind: Vec<u16> = "@introuvable.dll,-12\0".encode_utf16().collect();
        assert_eq!(
            SHLoadIndirectString(ind.as_ptr(), out.as_mut_ptr(), 32, 0),
            E_FAIL
        );

        let mut buf = [0u16; 8];
        let mut n = 0usize;
        assert_eq!(
            mbstowcs_s(
                &mut n,
                buf.as_mut_ptr(),
                8,
                c"abc".as_ptr().cast(),
                usize::MAX
            ),
            0
        );
        assert_eq!((n, &buf[..4]), (4, &[97u16, 98, 99, 0][..]));
        assert_eq!(
            mbstowcs_s(
                &mut n,
                buf.as_mut_ptr(),
                3,
                c"abcdef".as_ptr().cast(),
                usize::MAX
            ),
            STRUNCATE
        );
        assert_eq!((n, &buf[..3]), (3, &[97u16, 98, 0][..]));
        assert_eq!(
            mbstowcs_s(&mut n, buf.as_mut_ptr(), 3, c"abcdef".as_ptr().cast(), 6),
            ERANGE
        );
        assert_eq!(
            mbstowcs_s(
                &mut n,
                std::ptr::null_mut(),
                0,
                c"abcdef".as_ptr().cast(),
                6
            ),
            0
        );
        assert_eq!(n, 7);
    }
}

#[test]
fn melangeur_volume_et_sourdine() {
    unsafe {
        let mut h = 0usize;
        assert_eq!(mixerOpen(&mut h, 0, 0, 0, 0), MMSYSERR_NOERROR);
        assert_eq!(mixerOpen(&mut h, 3, 0, 0, 0), MMSYSERR_BADDEVICEID);
        assert_eq!(mixerOpen(&mut h, 0, 0, 0, 0), MMSYSERR_NOERROR);
        let mut caps = [0u8; 80];
        assert_eq!(mixerGetDevCapsW(0, caps.as_mut_ptr(), 80), MMSYSERR_NOERROR);
        assert_eq!(caps[76], 1);

        // MIXERLINEW x64 : 284 octets, recherche par type de composant.
        let mut line = [0u8; 284];
        line[..4].copy_from_slice(&284u32.to_le_bytes());
        line[28..32].copy_from_slice(&COMPONENT_SPEAKERS.to_le_bytes());
        assert_eq!(
            mixerGetLineInfoW(h, line.as_mut_ptr(), 0x8000_0003),
            MMSYSERR_NOERROR
        );
        let rd = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        assert_eq!(rd(&line, 12), LINE_ID);
        assert_eq!(rd(&line, 40), 2);
        assert_eq!(mixerGetLineInfoW(h, line.as_mut_ptr(), 1), MIXERR_INVALLINE);

        // MIXERLINECONTROLSW (x64 : 28 octets) : le contrôle de volume.
        let mut ctrl = [0u8; 228];
        let mut lc = [0u8; 28];
        lc[..4].copy_from_slice(&28u32.to_le_bytes());
        lc[4..8].copy_from_slice(&LINE_ID.to_le_bytes());
        lc[8..12].copy_from_slice(&CT_VOLUME.to_le_bytes());
        lc[12..16].copy_from_slice(&1u32.to_le_bytes());
        lc[16..20].copy_from_slice(&228u32.to_le_bytes());
        lc[20..28].copy_from_slice(&(ctrl.as_mut_ptr() as u64).to_le_bytes());
        assert_eq!(
            mixerGetLineControlsW(h, lc.as_mut_ptr(), 2),
            MMSYSERR_NOERROR
        );
        assert_eq!(rd(&ctrl, 4), VOLUME_ID);
        assert_eq!(rd(&ctrl, 184), 0xFFFF);

        // MIXERCONTROLDETAILS (x64 : 32 octets) : écrire puis relire.
        let mut value = 0x8000u32;
        let mut cd = [0u8; 32];
        cd[..4].copy_from_slice(&32u32.to_le_bytes());
        cd[4..8].copy_from_slice(&VOLUME_ID.to_le_bytes());
        cd[8..12].copy_from_slice(&1u32.to_le_bytes());
        cd[20..24].copy_from_slice(&4u32.to_le_bytes());
        cd[24..32].copy_from_slice(&(&mut value as *mut u32 as u64).to_le_bytes());
        assert_eq!(
            mixerSetControlDetails(h, cd.as_mut_ptr(), 0),
            MMSYSERR_NOERROR
        );
        value = 0;
        assert_eq!(
            mixerGetControlDetailsW(h, cd.as_mut_ptr(), 0),
            MMSYSERR_NOERROR
        );
        assert_eq!(value, 0x8000);
        cd[4..8].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(
            mixerGetControlDetailsW(h, cd.as_mut_ptr(), 0),
            MIXERR_INVALCONTROL
        );
        let mut id = 7u32;
        assert_eq!(mixerGetID(h, &mut id, 0x8000_0000), MMSYSERR_NOERROR);
        assert_eq!(id, 0);
        assert_eq!(mixerClose(h), MMSYSERR_NOERROR);
        assert_eq!(mixerClose(1), MMSYSERR_INVALHANDLE);
    }
}

#[test]
fn emetteur_dans_un_magasin() {
    let _teb = crate::process::test_teb();
    if crate::dlls::hostssl::ssl().is_none() {
        return;
    }
    unsafe {
        use crate::dlls::crypt32::{CertFreeCertificateContext, CertOpenStore};
        let name = crate::dlls::crypt32_encode::str_to_name("CN=Autorite banc", 0).unwrap();
        let subject = [name.len() as u64, name.as_ptr() as u64];
        let ca = crate::dlls::crypt32_encode::CertCreateSelfSignCertificate(
            0,
            subject.as_ptr() as usize,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        assert_ne!(ca, 0);
        // Auto-signé : pas d'émetteur à chercher.
        let mut flags = CERT_STORE_SIGNATURE_FLAG;
        assert_eq!(CertGetIssuerCertificateFromStore(0, ca, 0, &mut flags), 0);
        assert_eq!(crate::process::last_error(), CRYPT_E_SELF_SIGNED);
        // Magasin vide : aucun émetteur pour un certificat d'un autre émetteur.
        let store = CertOpenStore(2, 0, 0, 0, 0);
        assert_ne!(store, 0);
        CertFreeCertificateContext(ca);
    }
    assert_eq!(CryptInstallOIDFunctionAddress(0, 1, 0, 0, 0, 0), 1);
}
