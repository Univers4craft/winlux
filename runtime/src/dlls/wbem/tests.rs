use super::*;

#[test]
fn wql_selection_et_filtres() {
    let q = parse_query(
        "SELECT Name, ProcessId FROM win32_process WHERE ProcessId > 0 AND Name LIKE '%'",
    )
    .unwrap();
    assert_eq!(q.class, "Win32_Process");
    assert_eq!(
        q.props.as_deref(),
        Some(&["Name".to_string(), "ProcessId".to_string()][..])
    );
    assert_eq!(
        parse_query("SELECT * FROM Win32_Nothing").unwrap_err(),
        WBEM_E_INVALID_CLASS
    );
    assert_eq!(
        parse_query("SELEC * FROM Win32_BIOS").unwrap_err(),
        WBEM_E_INVALID_QUERY
    );
    assert_eq!(
        parse_query("SELECT * FROM Win32_BIOS WHERE").unwrap_err(),
        WBEM_E_INVALID_QUERY
    );
    assert!(like("Intel(R) Core(TM)", "%core%"));
    assert!(like("C:", "_:"));
    assert!(!like("D:", "C%"));
}

#[test]
fn wmi_classes_du_systeme() {
    let os = run_query("SELECT * FROM Win32_OperatingSystem").unwrap();
    assert_eq!(os.len(), 1);
    let version = os[0].get("Version").unwrap().2.text().unwrap();
    assert!(version.starts_with("10.0."), "{version}");
    assert_eq!(
        os[0].get("OSArchitecture").unwrap().2,
        Val::Str("64-bit".into())
    );

    let me = std::process::id();
    let mine = run_query(&format!(
        "SELECT Name, ProcessId FROM Win32_Process WHERE ProcessId = {me}"
    ))
    .unwrap();
    assert_eq!(mine.len(), 1);
    // Propriétés demandées, plus la clé.
    let names: Vec<&str> = mine[0].props.iter().map(|p| p.0.as_str()).collect();
    assert_eq!(names, ["Handle", "Name", "ProcessId"]);
    assert_eq!(
        mine[0].relpath().unwrap(),
        format!("Win32_Process.Handle=\"{me}\"")
    );
    assert_eq!(
        run_query("SELECT Nope FROM Win32_Process").unwrap_err(),
        WBEM_E_INVALID_QUERY
    );

    let cpus = run_query("SELECT * FROM Win32_Processor").unwrap();
    assert!(!cpus.is_empty());
    assert!(matches!(cpus[0].get("NumberOfLogicalProcessors").unwrap().2, Val::U32(n) if n >= 1));

    let by_path = object_at("\\\\.\\root\\cimv2:Win32_Processor.DeviceID=\"CPU0\"").unwrap();
    assert_eq!(by_path.get("DeviceID").unwrap().2, Val::Str("CPU0".into()));
    assert!(object_at("Win32_OperatingSystem=@").is_ok());
    let class = object_at("Win32_BIOS").unwrap();
    assert!(!class.instance);
    assert_eq!(
        object_at("Win32_Processor.DeviceID=\"CPU99\"").unwrap_err(),
        WBEM_E_NOT_FOUND
    );
    for c in CLASSES {
        assert!(instances(c).is_some(), "{c}");
    }
}

/// Le parcours d'un programme : locator, ConnectServer, ExecQuery, Next,
/// Get dans un VARIANT, GetNames.
#[test]
fn wmi_par_com() {
    unsafe {
        let mut loc: *mut c_void = std::ptr::null_mut();
        assert_eq!(
            create_builtin(
                &*(&CLSID_WBEM_LOCATOR as *const Guid as *const super::super::ole32::Guid),
                &*(&IID_IWBEM_LOCATOR as *const Guid as *const super::super::ole32::Guid),
                &mut loc
            ),
            Some(S_OK)
        );
        let loc = loc as *mut Com;
        let ns: Vec<u16> = "ROOT\\CIMV2\0".encode_utf16().collect();
        let mut svc: *mut Com = std::ptr::null_mut();
        assert_eq!(
            connect_server(
                loc,
                ns.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &mut svc
            ),
            S_OK
        );
        let bad: Vec<u16> = "ROOT\\NOWHERE\0".encode_utf16().collect();
        let mut none: *mut Com = std::ptr::null_mut();
        assert_eq!(
            connect_server(
                loc,
                bad.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &mut none
            ),
            WBEM_E_INVALID_NAMESPACE
        );
        let wql: Vec<u16> = "WQL\0".encode_utf16().collect();
        let q: Vec<u16> = "SELECT * FROM Win32_OperatingSystem\0"
            .encode_utf16()
            .collect();
        let mut en: *mut Com = std::ptr::null_mut();
        assert_eq!(
            exec_query(svc, wql.as_ptr(), q.as_ptr(), 0x30, 0, &mut en),
            S_OK
        );
        let mut objs = [std::ptr::null_mut::<Com>(); 2];
        let mut got = 0u32;
        assert_eq!(e_next(en, -1, 2, objs.as_mut_ptr(), &mut got), S_FALSE);
        assert_eq!(got, 1);
        let mut var = [0u64; 3];
        let mut kind = 0i32;
        let name: Vec<u16> = "Caption\0".encode_utf16().collect();
        assert_eq!(
            o_get(
                objs[0],
                name.as_ptr(),
                0,
                var.as_mut_ptr().cast(),
                &mut kind,
                std::ptr::null_mut()
            ),
            S_OK
        );
        assert_eq!(kind, CIM_STRING);
        assert_eq!(var[0] as u16, VT_BSTR);
        super::super::oleaut32::variant_clear(var.as_mut_ptr().cast());
        let total: Vec<u16> = "TotalVisibleMemorySize\0".encode_utf16().collect();
        assert_eq!(
            o_get(
                objs[0],
                total.as_ptr(),
                0,
                var.as_mut_ptr().cast(),
                &mut kind,
                std::ptr::null_mut()
            ),
            S_OK
        );
        // uint64 : chaîne, comme sous Windows.
        assert_eq!((var[0] as u16, kind), (VT_BSTR, CIM_UINT64));
        super::super::oleaut32::variant_clear(var.as_mut_ptr().cast());
        let class: Vec<u16> = "__CLASS\0".encode_utf16().collect();
        assert_eq!(
            o_get(
                objs[0],
                class.as_ptr(),
                0,
                var.as_mut_ptr().cast(),
                &mut kind,
                std::ptr::null_mut()
            ),
            S_OK
        );
        super::super::oleaut32::variant_clear(var.as_mut_ptr().cast());
        let missing: Vec<u16> = "Nope\0".encode_utf16().collect();
        assert_eq!(
            o_get(
                objs[0],
                missing.as_ptr(),
                0,
                var.as_mut_ptr().cast(),
                &mut kind,
                std::ptr::null_mut()
            ),
            WBEM_E_NOT_FOUND
        );
        let mut names = 0usize;
        assert_eq!(
            o_get_names(
                objs[0],
                std::ptr::null(),
                WBEM_FLAG_NONSYSTEM_ONLY,
                std::ptr::null(),
                &mut names
            ),
            S_OK
        );
        assert_ne!(names, 0);
        // Énumération des propriétés jusqu'à WBEM_S_NO_MORE_DATA.
        assert_eq!(o_begin_enumeration(objs[0], WBEM_FLAG_NONSYSTEM_ONLY), S_OK);
        let mut n = 0;
        loop {
            let mut b: *mut u16 = std::ptr::null_mut();
            let hr = o_next(
                objs[0],
                0,
                &mut b,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if hr == WBEM_S_NO_MORE_DATA {
                break;
            }
            assert_eq!(hr, S_OK);
            super::super::oleaut32::sys_free_string(b);
            n += 1;
        }
        assert!(n > 20);
        release(objs[0]);
        release(en);
        release(svc);
        release(loc);
    }
}
