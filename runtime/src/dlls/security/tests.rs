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

        // TokenLogonSid (#747) : un seul groupe, le SID de session S-1-5-5-….
        let mut logon = vec![0u8; 128];
        assert_eq!(
            GetTokenInformation(tok, 28, logon.as_mut_ptr(), 128, &mut need),
            1
        );
        assert_eq!(u32::from_le_bytes(logon[0..4].try_into().unwrap()), 1);
        let sid = std::ptr::read_unaligned(logon.as_ptr().add(8) as *const *const u8);
        assert!(sid_text(sid_at(sid).unwrap())
            .unwrap()
            .starts_with("S-1-5-5-"));
        let attrs = u32::from_le_bytes(logon[16..20].try_into().unwrap());
        assert_ne!(attrs & SE_GROUP_LOGON_ID, 0);

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

#[test]
fn absolute_descriptor_and_explicit_acl() {
    unsafe {
        let mut sd = [0u8; 40];
        assert_eq!(InitializeSecurityDescriptor(sd.as_mut_ptr(), 1), 1);
        let owner = sid_bytes(&uid_sid(1000)).unwrap();
        // EXPLICIT_ACCESS_W : GENERIC_ALL accordé au SID du propriétaire.
        let mut entry = [0u8; 48];
        entry[0..4].copy_from_slice(&0x1000_0000u32.to_le_bytes());
        entry[4..8].copy_from_slice(&1u32.to_le_bytes()); // GRANT_ACCESS
        entry[40..48].copy_from_slice(&(owner.as_ptr() as usize).to_le_bytes());
        let mut acl: *mut u8 = std::ptr::null_mut();
        assert_eq!(
            SetEntriesInAclW(1, entry.as_ptr(), std::ptr::null(), &mut acl),
            0
        );
        assert_eq!(std::ptr::read_unaligned(acl.add(4) as *const u16), 1);
        assert_eq!(SetSecurityDescriptorDacl(sd.as_mut_ptr(), 1, acl, 0), 1);
        assert_eq!(
            SetSecurityDescriptorOwner(sd.as_mut_ptr(), owner.as_ptr() as *mut u8, 0),
            1
        );
        let (mut present, mut dacl, mut defaulted) = (0, std::ptr::null_mut(), 1);
        assert_eq!(
            GetSecurityDescriptorDacl(sd.as_ptr(), &mut present, &mut dacl, &mut defaulted),
            1
        );
        assert_eq!((present, dacl), (1, acl));
        let mut got: *mut u8 = std::ptr::null_mut();
        assert_eq!(
            GetSecurityDescriptorOwner(sd.as_ptr(), &mut got, std::ptr::null_mut()),
            1
        );
        assert_eq!(EqualSid(got, owner.as_ptr()), 1);
    }
}

/// Le courtier du bac à sable de Chromium pose un jeton sur le thread de
/// l'enfant puis le relit avec OpenThreadToken ; sans jeton, il tue
/// l'enfant (#767).
#[test]
fn jeton_pose_sur_un_thread_puis_relu() {
    let _teb = crate::process::test_teb();
    let thread = 0x7a5e_0001usize;
    let mut t = 0usize;
    unsafe {
        assert_eq!(open_thread_token(thread, &mut t), 0);
        assert_eq!(crate::process::last_error(), 1008);
        assert_eq!(SetThreadToken(&thread, TOKEN_HANDLE), 1);
        assert_eq!(open_thread_token(thread, &mut t), 1);
        assert_eq!(t, TOKEN_HANDLE);
        assert_eq!(SetThreadToken(&thread, 0), 1);
        assert_eq!(open_thread_token(thread, &mut t), 0);
        // Thread courant : ImpersonateSelf puis RevertToSelf.
        assert_eq!(ImpersonateSelf(2), 1);
        assert_eq!(open_thread_token(CURRENT_THREAD_PSEUDO, &mut t), 1);
        assert_eq!(RevertToSelf(), 1);
        assert_eq!(open_thread_token(CURRENT_THREAD_PSEUDO, &mut t), 0);
    }
}

/// Chromium ferme l'objet Job de chaque enfant et vérifie que CloseHandle
/// réussit (#767).
#[test]
fn objet_job_se_ferme_une_fois() {
    let _teb = crate::process::test_teb();
    let job = unsafe { crate::dlls::jobs::create_job_object_w(0, std::ptr::null()) };
    assert_eq!(crate::dlls::kernel32::close_handle(job), 1);
    assert_eq!(crate::dlls::kernel32::close_handle(job), 0);
}

/// Lancement d'un renderer en bac à sable (#1352) : TokenPrivileges tient
/// dans plus de 16 octets et AddMandatoryAce ajoute une ACE d'étiquette.
#[test]
fn privileges_du_jeton_et_ace_obligatoire() {
    let _teb = crate::process::test_teb();
    unsafe {
        let mut tok = 0usize;
        assert_eq!(OpenProcessToken(usize::MAX, 8, &mut tok), 1);
        let mut need = 0u32;
        assert_eq!(
            GetTokenInformation(tok, 3, std::ptr::null_mut(), 0, &mut need),
            0
        );
        assert!(need >= 0x10, "TOKEN_PRIVILEGES de {need} octets");
        let mut buf = vec![0u8; need as usize];
        assert_eq!(
            GetTokenInformation(tok, 3, buf.as_mut_ptr(), need, &mut need),
            1
        );
        let count = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
        assert_eq!(count, 5);
        let privs: Vec<(u32, u32)> = (0..count)
            .map(|i| {
                let at = 4 + i * 12;
                (
                    u32::from_le_bytes(buf[at..at + 4].try_into().unwrap()),
                    u32::from_le_bytes(buf[at + 8..at + 12].try_into().unwrap()),
                )
            })
            .collect();
        assert!(privs.contains(&(23, 3)), "SeChangeNotifyPrivilege actif");

        let label = [0u8, 0, 0, 0, 0, 16];
        let mut low = std::ptr::null_mut();
        assert_eq!(
            AllocateAndInitializeSid(label.as_ptr(), 1, 4096, 0, 0, 0, 0, 0, 0, 0, &mut low),
            1
        );
        let mut acl = [0u8; 64];
        assert_eq!(InitializeAcl(acl.as_mut_ptr(), 64, 2), 1);
        assert_eq!(AddMandatoryAce(acl.as_mut_ptr(), 2, 0, 1, low), 1);
        assert_eq!(u16::from_le_bytes([acl[4], acl[5]]), 1);
        assert_eq!(acl[8], 0x11, "SYSTEM_MANDATORY_LABEL_ACE_TYPE");
        assert_eq!(u32::from_le_bytes(acl[12..16].try_into().unwrap()), 1);
        FreeSid(low.cast());
    }
}

/// Comptes bien connus résolus comme sous Windows (#1639) : WiX
/// (WixQueryOsWellKnownSID) les cherche avant d'écrire des ACL.
#[test]
fn comptes_bien_connus_resolus() {
    for (kind, name, domain, use_) in [
        (23u32, "LOCAL SERVICE", "NT AUTHORITY", 5u32),
        (24, "NETWORK SERVICE", "NT AUTHORITY", 5),
        (28, "Guests", "BUILTIN", 4),
        (58, "Performance Log Users", "BUILTIN", 4),
    ] {
        let mut sid = [0u8; 68];
        let mut size = sid.len() as u32;
        assert_eq!(
            unsafe { CreateWellKnownSid(kind, std::ptr::null(), sid.as_mut_ptr(), &mut size) },
            1,
            "{kind}"
        );
        let (mut n, mut d) = ([0u16; 64], [0u16; 64]);
        let (mut nl, mut dl, mut u) = (64u32, 64u32, 0u32);
        let ok = unsafe {
            LookupAccountSidW(
                std::ptr::null(),
                sid.as_ptr(),
                n.as_mut_ptr(),
                &mut nl,
                d.as_mut_ptr(),
                &mut dl,
                &mut u,
            )
        };
        assert_eq!(ok, 1, "{kind}");
        assert_eq!(String::from_utf16_lossy(&n[..nl as usize]), name);
        assert_eq!(String::from_utf16_lossy(&d[..dl as usize]), domain);
        assert_eq!(u, use_);
    }
}
