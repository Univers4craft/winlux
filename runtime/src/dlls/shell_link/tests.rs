use super::*;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe fn new_link(iid: &Guid) -> *mut c_void {
    let mut out: *mut c_void = std::ptr::null_mut();
    assert_eq!(create_builtin(&CLSID_SHELL_LINK, iid, &mut out), Some(S_OK));
    assert!(!out.is_null());
    out
}

unsafe fn read_buf(f: impl FnOnce(*mut u16, i32) -> i32) -> String {
    let mut buf = [0u16; 300];
    assert_eq!(f(buf.as_mut_ptr(), buf.len() as i32), S_OK);
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// #1732 : un raccourci créé par IShellLinkW + IPersistFile::Save est un
/// vrai .lnk, relu à l'identique par IPersistFile::Load.
#[test]
fn raccourci_ecrit_puis_relu() {
    unsafe {
        let dir = std::env::temp_dir().join(format!("winlux-shelllink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = crate::paths::to_windows(&dir.join("Discord.lnk"));

        let link = new_link(&IID_ISHELL_LINK_W);
        let target = "C:\\Users\\d\\AppData\\Local\\Discord\\Update.exe";
        assert_eq!(w_set_path(link, w(target).as_ptr()), S_OK);
        assert_eq!(
            w_set_arguments(link, w("--processStart Discord.exe").as_ptr()),
            S_OK
        );
        assert_eq!(
            w_set_working_directory(link, w("C:\\Users\\d\\AppData\\Local\\Discord").as_ptr()),
            S_OK
        );
        assert_eq!(w_set_description(link, w("Discord").as_ptr()), S_OK);
        assert_eq!(
            w_set_icon_location(link, w("C:\\Users\\d\\app.ico").as_ptr(), 2),
            S_OK
        );
        // IPropertyStore : System.AppUserModel.ID gardé par l'objet.
        let mut props: *mut c_void = std::ptr::null_mut();
        assert_eq!(w_qi(link, &IID_IPROPERTY_STORE, &mut props), S_OK);
        let key = [7u8; 20];
        let id = w("com.squirrel.Discord.Discord");
        let mut pv = [0u8; 24];
        pv[0] = VT_LPWSTR as u8;
        std::ptr::write_unaligned(pv.as_mut_ptr().add(8) as *mut usize, id.as_ptr() as usize);
        assert_eq!(p_set_value(props, key.as_ptr(), pv.as_ptr()), S_OK);
        let mut got = [0u8; 24];
        assert_eq!(p_get_value(props, key.as_ptr(), got.as_mut_ptr()), S_OK);
        assert_eq!(got[0], VT_LPWSTR as u8);
        let s = std::ptr::read_unaligned(got.as_ptr().add(8) as *const usize) as *const u16;
        assert_eq!(super::super::wstr(s), "com.squirrel.Discord.Discord");
        super::super::ole32::co_task_mem_free(s as *mut c_void);
        let mut n = 0;
        p_get_count(props, &mut n);
        assert_eq!(n, 1);
        assert_eq!(p_commit(props), S_OK);
        p_release(props);

        let mut pf: *mut c_void = std::ptr::null_mut();
        assert_eq!(w_qi(link, &IID_IPERSIST_FILE, &mut pf), S_OK);
        assert_eq!(f_is_dirty(pf), S_OK);
        assert_eq!(f_save(pf, w(&file).as_ptr(), 1), S_OK);
        assert_eq!(f_is_dirty(pf), S_FALSE);
        let mut cur: *mut u16 = std::ptr::null_mut();
        assert_eq!(f_get_cur_file(pf, &mut cur), S_OK);
        assert_eq!(super::super::wstr(cur), file);
        super::super::ole32::co_task_mem_free(cur as *mut c_void);
        f_release(pf);
        // Dernière référence : l'objet est libéré.
        assert_eq!(w_release(link), 0);

        let bytes = std::fs::read(dir.join("Discord.lnk")).unwrap();
        assert_eq!(&bytes[..4], &[0x4C, 0, 0, 0]);
        let parsed = parse_link(&bytes).unwrap();
        assert_eq!(parsed.target, target);
        assert_eq!(parsed.args, "--processStart Discord.exe");
        assert_eq!(parsed.icon_index, 2);

        // Relecture par un second objet, demandé par IPersistFile.
        let pf = new_link(&IID_IPERSIST_FILE);
        assert_eq!(f_load(pf, w(&file).as_ptr(), 0), S_OK);
        let mut lw: *mut c_void = std::ptr::null_mut();
        assert_eq!(f_qi(pf, &IID_ISHELL_LINK_W, &mut lw), S_OK);
        let mut fd = vec![0u8; 592];
        assert_eq!(
            read_buf(|b, n| w_get_path(lw, b, n, fd.as_mut_ptr(), 0)),
            target
        );
        assert_eq!(
            read_buf(|b, n| w_get_arguments(lw, b, n)),
            "--processStart Discord.exe"
        );
        assert_eq!(
            read_buf(|b, n| w_get_working_directory(lw, b, n)),
            "C:\\Users\\d\\AppData\\Local\\Discord"
        );
        assert_eq!(read_buf(|b, n| w_get_description(lw, b, n)), "Discord");
        let mut index = 0;
        assert_eq!(
            read_buf(|b, n| w_get_icon_location(lw, b, n, &mut index)),
            "C:\\Users\\d\\app.ico"
        );
        assert_eq!(index, 2);
        let mut show = 0;
        w_get_show_cmd(lw, &mut show);
        assert_eq!(show, 1);
        // Interface ANSI sur le même objet.
        let mut la: *mut c_void = std::ptr::null_mut();
        assert_eq!(w_qi(lw, &IID_ISHELL_LINK_A, &mut la), S_OK);
        let mut buf = [0u8; 300];
        assert_eq!(
            a_get_path(la, buf.as_mut_ptr(), 300, std::ptr::null_mut(), 0),
            S_OK
        );
        assert_eq!(&buf[..target.len()], target.as_bytes());
        a_release(la);
        w_release(lw);
        assert_eq!(f_release(pf), 0);

        // Fichier absent, dossier absent.
        let pf = new_link(&IID_IPERSIST_FILE);
        let missing = crate::paths::to_windows(&dir.join("absent.lnk"));
        assert_eq!(f_load(pf, w(&missing).as_ptr(), 0), HR_FILE_NOT_FOUND);
        let nowhere = crate::paths::to_windows(&dir.join("non/x.lnk"));
        assert_eq!(f_save(pf, w(&nowhere).as_ptr(), 1), HR_PATH_NOT_FOUND);
        let mut other: *mut c_void = std::ptr::null_mut();
        assert_eq!(f_qi(pf, &guid(1, 2, 3, [0; 8]), &mut other), E_NOINTERFACE);
        f_release(pf);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Un .lnk écrit par Windows : liste d'identifiants puis bloc LinkInfo
/// (chemin local en ANSI et en Unicode), chaînes Unicode.
#[test]
fn raccourci_windows_avec_linkinfo() {
    let mut d = Vec::new();
    d.extend_from_slice(&0x4Cu32.to_le_bytes());
    d.extend_from_slice(&[0; 16]);
    let flags = HAS_ID_LIST | HAS_LINK_INFO | HAS_ARGUMENTS | IS_UNICODE;
    d.extend_from_slice(&flags.to_le_bytes());
    d.resize(0x4C, 0);
    d[60] = 7; // SW_SHOWMINNOACTIVE
               // Liste d'identifiants : 6 octets ignorés.
    d.extend_from_slice(&6u16.to_le_bytes());
    d.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
    // LinkInfo à en-tête de 0x24 octets.
    let ansi_path = b"C:\\Old\\a.exe\0";
    let wide_path: Vec<u8> = "C:\\Prog\\é.exe\0"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let header = 0x24usize;
    let local = header;
    let local_u = local + ansi_path.len();
    let size = local_u + wide_path.len();
    let mut info = vec![0u8; header];
    info[0..4].copy_from_slice(&(size as u32).to_le_bytes());
    info[4..8].copy_from_slice(&(header as u32).to_le_bytes());
    info[16..20].copy_from_slice(&(local as u32).to_le_bytes());
    info[28..32].copy_from_slice(&(local_u as u32).to_le_bytes());
    info.extend_from_slice(ansi_path);
    info.extend_from_slice(&wide_path);
    d.extend_from_slice(&info);
    let args: Vec<u16> = "-x".encode_utf16().collect();
    d.extend_from_slice(&(args.len() as u16).to_le_bytes());
    for u in args {
        d.extend_from_slice(&u.to_le_bytes());
    }
    let l = parse_link(&d).unwrap();
    assert_eq!(l.target, "C:\\Prog\\é.exe");
    assert_eq!(l.args, "-x");
    assert_eq!(l.show_cmd, 7);
    // Tronqué ou étranger : refusé.
    assert!(parse_link(&d[..d.len() - 1]).is_none());
    assert!(parse_link(b"MZ\0\0").is_none());
    // Même format que les raccourcis posés par les installateurs.
    let ours = link_bytes(&LinkData {
        target: "C:\\a\\b.exe".into(),
        ..Default::default()
    });
    assert_eq!(parse_link(&ours).unwrap().target, "C:\\a\\b.exe");
}
