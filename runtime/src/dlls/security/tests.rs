//! Droits Linux → sécurité Windows.

use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn sid_texte_et_binaire() {
    for s in [
        "S-1-1-0",
        "S-1-5-32-544",
        "S-1-5-21-1-2-3-1000",
        "S-1-16-12288",
    ] {
        assert_eq!(sid_text(&sid_bytes(s).unwrap()).as_deref(), Some(s));
    }
    assert_eq!(
        sid_bytes("S-1-5-32-544").unwrap(),
        [1, 2, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0, 0x20, 2, 0, 0]
    );
    assert!(sid_bytes("S-2-5").is_none());
    assert!(sid_bytes("pas un SID").is_none());
    assert!(sid_text(&[1, 3, 0, 0]).is_none());
    assert_eq!(uid_sid(0), "S-1-5-18");
    assert!(uid_sid(1000).ends_with("-1000"));
    assert!(uid_sid(33).ends_with(&format!("-{}", 33 + 0x40000)));
    assert_eq!(uid_sid(1000), uid_sid(1000), "stable");
    assert_ne!(uid_sid(1000), uid_sid(1001));
}

#[test]
fn bits_rwx_en_droits_windows() {
    assert_eq!(rights_of(7, true, false), FILE_ALL_ACCESS);
    assert_eq!(
        rights_of(5, false, true),
        FILE_GENERIC_READ | FILE_GENERIC_EXECUTE
    );
    assert_eq!(rights_of(4, false, false), FILE_GENERIC_READ);
    assert_eq!(rights_of(0, false, false), 0);
    let owner_rw = rights_of(6, true, false);
    assert_eq!(owner_rw & FILE_GENERIC_WRITE, FILE_GENERIC_WRITE);
    assert_ne!(owner_rw & WRITE_DAC, 0);
    assert_eq!(
        owner_rw & FILE_GENERIC_EXECUTE & !FILE_GENERIC_READ & !FILE_GENERIC_WRITE,
        0
    );
    // Dossier rwx pour le groupe : il peut supprimer ce qu'il contient.
    assert_ne!(rights_of(7, false, true) & FILE_DELETE_CHILD, 0);
}

#[test]
fn descripteur_d_un_fichier_linux() {
    let dir = std::env::temp_dir().join(format!("winlux-sd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("notes.txt");
    std::fs::write(&f, b"x").unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o640)).unwrap();
    let meta = std::fs::metadata(&f).unwrap();

    let sd = file_descriptor(&f, 1 | 2 | 4).unwrap();
    unsafe {
        assert_eq!(IsValidSecurityDescriptor(sd.as_ptr()), 1);
        assert_eq!(GetSecurityDescriptorLength(sd.as_ptr()) as usize, sd.len());
        let mut owner = std::ptr::null_mut();
        assert_eq!(
            GetSecurityDescriptorOwner(sd.as_ptr(), &mut owner, std::ptr::null_mut()),
            1
        );
        assert_eq!(
            sid_text(sid_at(owner).unwrap()).unwrap(),
            uid_sid(meta.uid())
        );
        let mut group = std::ptr::null_mut();
        GetSecurityDescriptorGroup(sd.as_ptr(), &mut group, std::ptr::null_mut());
        assert_eq!(
            sid_text(sid_at(group).unwrap()).unwrap(),
            gid_sid(meta.gid())
        );
        let (mut present, mut dacl) = (0, std::ptr::null_mut());
        assert_eq!(
            GetSecurityDescriptorDacl(sd.as_ptr(), &mut present, &mut dacl, std::ptr::null_mut()),
            1
        );
        assert_eq!(present, 1);
        let mut info = [0u32; 3];
        assert_eq!(GetAclInformation(dacl, info.as_mut_ptr(), 12, 2), 1);
        // propriétaire rw-, groupe r--, autres --- (pas d'entrée), + SYSTEM si non root.
        let expected = if meta.uid() == 0 { 2 } else { 3 };
        assert_eq!(info[0], expected);
        let mut ace = std::ptr::null_mut();
        assert_eq!(GetAce(dacl, 1, &mut ace), 1);
        let mask = std::ptr::read_unaligned(ace.add(4) as *const u32);
        assert_eq!(mask, FILE_GENERIC_READ);
        assert_eq!(
            sid_text(sid_at(ace.add(8)).unwrap()).unwrap(),
            gid_sid(meta.gid())
        );
        assert_eq!(GetAce(dacl, 9, &mut ace), 0);
    }
    // Seulement le propriétaire demandé : pas de DACL.
    let only_owner = file_descriptor(&f, 1).unwrap();
    assert_eq!(
        u32::from_le_bytes(only_owner[16..20].try_into().unwrap()),
        0
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn jeton_du_processus() {
    unsafe {
        let mut tok = 0usize;
        assert_eq!(OpenProcessToken(usize::MAX, 8, &mut tok), 1);
        let mut need = 0u32;
        assert_eq!(
            GetTokenInformation(tok, 1, std::ptr::null_mut(), 0, &mut need),
            0
        );
        let mut buf = vec![0u8; need as usize];
        assert_eq!(
            GetTokenInformation(tok, 1, buf.as_mut_ptr(), need, &mut need),
            1
        );
        let sid_ptr = std::ptr::read_unaligned(buf.as_ptr() as *const *const u8);
        assert_eq!(
            sid_ptr,
            buf.as_ptr().add(16),
            "le PSID pointe dans le tampon"
        );
        assert_eq!(
            sid_text(sid_at(sid_ptr).unwrap()).unwrap(),
            uid_sid(libc::geteuid())
        );

        let mut elev = [0u8; 4];
        assert_eq!(
            GetTokenInformation(tok, 20, elev.as_mut_ptr(), 4, &mut need),
            1
        );
        assert_eq!(u32::from_le_bytes(elev), elevated() as u32);

        let mut groups = vec![0u8; 512];
        assert_eq!(
            GetTokenInformation(tok, 2, groups.as_mut_ptr(), 512, &mut need),
            1
        );
        let count = u32::from_le_bytes(groups[0..4].try_into().unwrap()) as usize;
        assert!(count >= 4);
        let first = std::ptr::read_unaligned(groups.as_ptr().add(8) as *const *const u8);
        assert_eq!(sid_text(sid_at(first).unwrap()).unwrap(), EVERYONE);

        let everyone = sid_bytes(EVERYONE).unwrap();
        let admins = sid_bytes(ADMINISTRATORS).unwrap();
        let mut member = 0;
        assert_eq!(CheckTokenMembership(0, everyone.as_ptr(), &mut member), 1);
        assert_eq!(member, 1);
        assert_eq!(CheckTokenMembership(0, admins.as_ptr(), &mut member), 1);
        assert_eq!(
            member,
            elevated() as i32,
            "administrateur seulement une fois élevé"
        );
        assert_eq!(IsUserAnAdmin(), elevated() as i32);
        assert_eq!(
            GetTokenInformation(tok, 999, groups.as_mut_ptr(), 512, &mut need),
            0
        );
    }
}

#[test]
fn sid_alloues_et_convertis() {
    unsafe {
        let nt = [0u8, 0, 0, 0, 0, 5];
        let mut sid = std::ptr::null_mut();
        assert_eq!(
            AllocateAndInitializeSid(nt.as_ptr(), 2, 32, 544, 0, 0, 0, 0, 0, 0, &mut sid),
            1
        );
        assert_eq!(GetLengthSid(sid), 16);
        let admins = sid_bytes(ADMINISTRATORS).unwrap();
        assert_eq!(EqualSid(sid, admins.as_ptr()), 1);
        assert_eq!(*GetSidSubAuthority(sid, 1), 544);
        let mut text = std::ptr::null_mut();
        assert_eq!(ConvertSidToStringSidW(sid, &mut text), 1);
        assert_eq!(crate::dlls::wstr(text), "S-1-5-32-544");
        crate::dlls::kernel32_misc::local_free(text.cast());
        FreeSid(sid.cast());
        let alias: Vec<u16> = "BU\0".encode_utf16().collect();
        let mut users = std::ptr::null_mut();
        assert_eq!(ConvertStringSidToSidW(alias.as_ptr(), &mut users), 1);
        assert_eq!(sid_text(sid_at(users).unwrap()).unwrap(), USERS);
        FreeSid(users.cast());
        let mut luid = [0u32; 2];
        let name: Vec<u16> = "SeShutdownPrivilege\0".encode_utf16().collect();
        assert_eq!(
            LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), luid.as_mut_ptr()),
            1
        );
        assert_eq!(luid, [19, 0]);
    }
}
