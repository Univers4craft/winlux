use super::*;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe fn display(mk: usize) -> Result<String, i32> {
    let mut name: *mut u16 = std::ptr::null_mut();
    let hr = mk_get_display_name(mk, 0, 0, &mut name);
    if hr < 0 {
        return Err(hr);
    }
    let s = wide(name);
    super::super::ole32::co_task_mem_free(name.cast());
    Ok(s)
}

#[test]
fn monikers_table_des_objets_actifs_et_liaison() {
    unsafe {
        let path = w("C:\\Essai\\lot1511-moniker.doc");
        let mut file = 0usize;
        assert_eq!(create_file_moniker(path.as_ptr(), &mut file), S_OK);
        assert_eq!(display(file).unwrap(), "C:\\Essai\\lot1511-moniker.doc");
        let mut sys = 0u32;
        assert_eq!(mk_is_system_moniker(file, &mut sys), S_OK);
        assert_eq!(sys, MKSYS_FILEMONIKER);

        let (delim, item) = (w("!"), w("Feuille1"));
        let mut it = 0usize;
        assert_eq!(
            create_item_moniker(delim.as_ptr(), item.as_ptr(), &mut it),
            S_OK
        );
        assert_eq!(display(it).unwrap(), "!Feuille1");
        assert_eq!(mk_is_equal(file, it), S_FALSE);

        // Un objet quelconque : le contexte de liaison lui-même.
        let mut bind = 0usize;
        assert_eq!(create_bind_ctx(0, &mut bind), S_OK);
        let mut ptr = 0usize;
        assert_eq!(create_pointer_moniker(bind, &mut ptr), S_OK);
        assert_eq!(display(ptr), Err(E_NOTIMPL));
        let mut got = 0usize;
        assert_eq!(mk_bind_to_object(ptr, 0, 0, &IID_IBIND_CTX, &mut got), S_OK);
        assert_eq!(got, bind);
        bc_release(got);

        // Pas encore actif : la liaison échoue proprement.
        assert_eq!(mk_is_running(file, 0, 0, 0), S_FALSE);
        assert_eq!(
            mk_bind_to_object(file, bind, 0, &IID_IUNKNOWN, &mut got),
            MK_E_CANTOPENFILE
        );

        let mut rot = 0usize;
        assert_eq!(get_running_object_table(0, &mut rot), S_OK);
        let mut cookie = 0u32;
        assert_eq!(rot_register(rot, 0, bind, file, &mut cookie), S_OK);
        assert_eq!(rot_is_running(rot, file), S_OK);

        // Même nom, autre casse, autre moniker : même objet.
        let other = w("c:\\essai\\LOT1511-moniker.doc");
        let mut file2 = 0usize;
        assert_eq!(create_file_moniker(other.as_ptr(), &mut file2), S_OK);
        assert_eq!(mk_is_equal(file, file2), S_OK);
        assert_eq!(rot_get_object(rot, file2, &mut got), S_OK);
        assert_eq!(got, bind);
        bc_release(got);

        // MkParseDisplayName puis CoGetObject passent par la table.
        let mut eaten = 0u32;
        let mut parsed = 0usize;
        assert_eq!(
            mk_parse_display_name_api(bind, other.as_ptr(), &mut eaten, &mut parsed),
            S_OK
        );
        assert_eq!(eaten as usize, other.len() - 1);
        mk_release(parsed);
        assert_eq!(
            co_get_object(path.as_ptr(), std::ptr::null(), &IID_IBIND_CTX, &mut got),
            S_OK
        );
        assert_eq!(got, bind);
        bc_release(got);

        // Énumération : au moins notre entrée.
        let mut en = 0usize;
        assert_eq!(rot_enum_running(rot, &mut en), S_OK);
        let mut seen = false;
        loop {
            let mut m = 0usize;
            let mut n = 0u32;
            if en_next(en, 1, &mut m, &mut n) != S_OK {
                break;
            }
            seen |= m == file;
            release(m);
        }
        assert!(seen);
        en_release(en);

        assert_eq!(rot_revoke(rot, cookie), S_OK);
        assert_eq!(rot_revoke(rot, cookie), E_INVALIDARG);
        assert_eq!(rot_is_running(rot, file2), S_FALSE);

        // Noms que Winlux ne sait pas lier : MK_E_SYNTAX, sans planter.
        for name in [
            "winmgmts:",
            "clsid:00000000-0000-0000-0000-000000000000:",
            "",
        ] {
            let n = w(name);
            let mut out = 1usize;
            assert_eq!(
                mk_parse_display_name_api(bind, n.as_ptr(), &mut eaten, &mut out),
                MK_E_SYNTAX
            );
            assert_eq!((out, eaten), (0, 0));
            assert_eq!(
                co_get_object(n.as_ptr(), std::ptr::null(), &IID_IUNKNOWN, &mut out),
                MK_E_SYNTAX
            );
        }

        mk_release(file);
        mk_release(file2);
        mk_release(it);
        mk_release(ptr);
        assert_eq!(bc_release(bind), 0);
    }
}

#[test]
fn contexte_de_liaison() {
    unsafe {
        let mut bind = 0usize;
        assert_eq!(create_bind_ctx(0, &mut bind), S_OK);
        let mut other = 0usize;
        assert_eq!(create_bind_ctx(0, &mut other), S_OK);

        // BIND_OPTS (16 octets) : grfMode par défaut STGM_READWRITE.
        let mut opts = [0u32; 4];
        opts[0] = 16;
        assert_eq!(bc_get_bind_options(bind, opts.as_mut_ptr().cast()), S_OK);
        assert_eq!(opts[2], 2);
        opts[1] = 1;
        assert_eq!(bc_set_bind_options(bind, opts.as_ptr().cast()), S_OK);
        let mut back = [16u32, 0, 0, 0];
        assert_eq!(bc_get_bind_options(bind, back.as_mut_ptr().cast()), S_OK);
        assert_eq!(back[1], 1);

        let key = w("Cle");
        assert_eq!(bc_register_object_param(bind, key.as_ptr(), other), S_OK);
        let mut got = 0usize;
        assert_eq!(bc_get_object_param(bind, key.as_ptr(), &mut got), S_OK);
        assert_eq!(got, other);
        bc_release(got);
        assert_eq!(bc_revoke_object_param(bind, key.as_ptr()), S_OK);
        assert_eq!(bc_get_object_param(bind, key.as_ptr(), &mut got), E_FAIL);

        assert_eq!(bc_register_object_bound(bind, other), S_OK);
        assert_eq!(bc_revoke_object_bound(bind, other), S_OK);
        assert_eq!(bc_revoke_object_bound(bind, other), MK_E_NOTBOUND);
        assert_eq!(bc_register_object_bound(bind, other), S_OK);
        assert_eq!(bc_release_bound_objects(bind), S_OK);

        let mut rot = 0usize;
        assert_eq!(bc_get_running_object_table(bind, &mut rot), S_OK);
        assert_eq!(rot, &ROT_OBJECT as *const Rot as usize);
        assert_eq!(bc_release(other), 0);
        assert_eq!(bc_release(bind), 0);
    }
}
