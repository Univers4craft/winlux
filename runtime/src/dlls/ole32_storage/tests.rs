use super::*;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

const RW: u32 = STGM_READWRITE | 0x10;
const CLSID_ESSAI: Guid = Guid {
    data1: 0x1235_0001,
    data2: 0x5a5a,
    data3: 0x4e11,
    data4: [1, 2, 3, 4, 5, 6, 7, 8],
};

unsafe fn call0(obj: usize, n: usize) -> i32 {
    let f: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(slot(obj, n));
    f(obj)
}
unsafe fn create(stg: usize, name: &str, storage: bool) -> usize {
    let n = w(name);
    let f: unsafe extern "win64" fn(usize, *const u16, u32, u32, u32, *mut usize) -> i32 =
        std::mem::transmute(slot(stg, if storage { 5 } else { 3 }));
    let mut out = 0;
    assert_eq!(
        f(stg, n.as_ptr(), RW | STGM_CREATE, 0, 0, &mut out),
        S_OK,
        "{name}"
    );
    out
}
unsafe fn open_stream(stg: usize, name: &str, mode: u32) -> Result<usize, i32> {
    let n = w(name);
    let f: unsafe extern "win64" fn(usize, *const u16, usize, u32, u32, *mut usize) -> i32 =
        std::mem::transmute(slot(stg, 4));
    let mut out = 0;
    let hr = f(stg, n.as_ptr(), 0, mode, 0, &mut out);
    if hr < 0 {
        Err(hr)
    } else {
        Ok(out)
    }
}
unsafe fn open_storage(stg: usize, name: &str) -> usize {
    let n = w(name);
    let f: unsafe extern "win64" fn(usize, *const u16, usize, u32, usize, u32, *mut usize) -> i32 =
        std::mem::transmute(slot(stg, 6));
    let mut out = 0;
    assert_eq!(f(stg, n.as_ptr(), 0, 0x10, 0, 0, &mut out), S_OK);
    out
}
unsafe fn write(stm: usize, data: &[u8]) -> i32 {
    let f: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stm, 4));
    let mut n = 0;
    f(stm, data.as_ptr(), data.len() as u32, &mut n)
}
unsafe fn read_all(stm: usize) -> Vec<u8> {
    let f: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stm, 3));
    let mut out = Vec::new();
    let mut buf = [0u8; 1000];
    loop {
        let mut n = 0;
        assert_eq!(f(stm, buf.as_mut_ptr(), 1000, &mut n), S_OK);
        if n == 0 {
            return out;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
}
unsafe fn names(stg: usize) -> Vec<String> {
    let f: unsafe extern "win64" fn(usize, u32, usize, u32, *mut usize) -> i32 =
        std::mem::transmute(slot(stg, 11));
    let mut e = 0;
    assert_eq!(f(stg, 0, 0, 0, &mut e), S_OK);
    let next: unsafe extern "win64" fn(usize, u32, *mut u8, *mut u32) -> i32 =
        std::mem::transmute(slot(e, 3));
    let mut out = Vec::new();
    loop {
        let mut st = [0u64; 10];
        let mut got = 0;
        next(e, 1, st.as_mut_ptr().cast(), &mut got);
        if got == 0 {
            break;
        }
        let p = st[0] as *mut u16;
        out.push(wide(p).unwrap());
        super::super::ole32::co_task_mem_free(p.cast());
    }
    call0(e, 2);
    out.sort();
    out
}

fn motif(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 7 + i / 251) as u8).collect()
}

/// Docfile écrit sur disque, reconnu par StgIsStorageFile, puis relu :
/// CLSID, petit flux (mini-flux), grand flux, sous-stockage, énumération.
#[test]
fn docfile_ecrit_puis_relu_et_reconnu() {
    unsafe {
        let dir = std::env::temp_dir().join(format!("winlux-stg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("essai.doc");
        let _ = std::fs::remove_file(&file);
        let name = w(file.to_str().unwrap());

        let mut stg = 0;
        assert_eq!(
            stg_create_docfile(name.as_ptr(), RW | STGM_CREATE, 0, &mut stg),
            S_OK
        );
        assert_eq!(write_class_stg(stg, &CLSID_ESSAI), S_OK);
        let petit = create(stg, "Petit", false);
        assert_eq!(write(petit, b"bonjour"), S_OK);
        let grand = create(stg, "Contenu", false);
        assert_eq!(write(grand, &motif(10_000)), S_OK);
        let sous = create(stg, "Sous", true);
        let dedans = create(sous, "Dedans", false);
        assert_eq!(write(dedans, &motif(100)), S_OK);
        // Nom invalide et doublon sans STGM_CREATE.
        let bad = w("a/b");
        let f: unsafe extern "win64" fn(usize, *const u16, u32, u32, u32, *mut usize) -> i32 =
            std::mem::transmute(slot(stg, 3));
        let mut x = 0;
        assert_eq!(f(stg, bad.as_ptr(), RW, 0, 0, &mut x), STG_E_INVALIDNAME);
        let dup = w("petit");
        assert_eq!(
            f(stg, dup.as_ptr(), RW, 0, 0, &mut x),
            STG_E_FILEALREADYEXISTS
        );
        for o in [dedans, sous, grand, petit] {
            call0(o, 2);
        }
        assert_eq!(stg_commit(stg, 0), S_OK);
        call0(stg, 2);

        assert_eq!(stg_is_storage_file(name.as_ptr()), S_OK);
        let texte = dir.join("texte.txt");
        std::fs::write(&texte, vec![b'x'; 600]).unwrap();
        let tname = w(texte.to_str().unwrap());
        assert_eq!(stg_is_storage_file(tname.as_ptr()), S_FALSE);
        let mut no = 0;
        assert_eq!(
            stg_open_storage(tname.as_ptr(), 0, 0x10, 0, 0, &mut no),
            STG_E_FILEALREADYEXISTS
        );
        let absent = w(dir.join("absent.doc").to_str().unwrap());
        assert_eq!(stg_is_storage_file(absent.as_ptr()), STG_E_FILENOTFOUND);

        let mut ro = 0;
        assert_eq!(
            stg_open_storage(name.as_ptr(), 0, 0x20, 0, 0, &mut ro),
            S_OK
        );
        let mut clsid = Guid::default();
        assert_eq!(read_class_stg(ro, &mut clsid), S_OK);
        assert_eq!(clsid, CLSID_ESSAI);
        assert_eq!(names(ro), ["Contenu", "Petit", "Sous"]);
        let s = open_stream(ro, "PETIT", 0x10).unwrap();
        assert_eq!(read_all(s), b"bonjour");
        assert_eq!(write(s, b"!"), STG_E_ACCESSDENIED);
        call0(s, 2);
        let s = open_stream(ro, "Contenu", 0x10).unwrap();
        assert_eq!(read_all(s), motif(10_000));
        call0(s, 2);
        assert_eq!(open_stream(ro, "Absent", 0x10), Err(STG_E_FILENOTFOUND));
        assert_eq!(open_stream(ro, "Contenu", RW), Err(STG_E_ACCESSDENIED));
        let sub = open_storage(ro, "Sous");
        let s = open_stream(sub, "Dedans", 0x10).unwrap();
        assert_eq!(read_all(s), motif(100));
        call0(s, 2);
        call0(sub, 2);
        call0(ro, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// En mémoire : ILockBytes sur HGLOBAL, écriture, relecture, transaction.
#[test]
fn docfile_en_memoire_et_transaction() {
    unsafe {
        let mut lb = 0;
        assert_eq!(create_ilockbytes_on_hglobal(0, 1, &mut lb), S_OK);
        let mut h = 0;
        assert_eq!(get_hglobal_from_ilockbytes(lb, &mut h), S_OK);
        assert_ne!(h, 0);
        let mut stg = 0;
        assert_eq!(
            stg_create_docfile_on_ilockbytes(lb, RW | STGM_CREATE, 0, &mut stg),
            S_OK
        );
        assert_eq!(stg_is_storage_ilockbytes(lb), S_OK);
        let s = create(stg, "Flux", false);
        assert_eq!(write(s, b"abc"), S_OK);
        assert_eq!(write_class_stm(s, &CLSID_ESSAI), S_OK);
        call0(s, 2);
        assert_eq!(stg_commit(stg, 0), S_OK);
        call0(stg, 2);

        let mut t = 0;
        assert_eq!(
            stg_open_storage_on_ilockbytes(lb, 0, RW | STGM_TRANSACTED, 0, 0, &mut t),
            S_OK
        );
        let s = open_stream(t, "Flux", RW).unwrap();
        let data = read_all(s);
        assert_eq!(&data[..3], b"abc");
        let mut g = Guid::default();
        let seek: unsafe extern "win64" fn(usize, i64, u32, *mut u64) -> i32 =
            std::mem::transmute(slot(s, 5));
        assert_eq!(seek(s, 3, 0, std::ptr::null_mut()), S_OK);
        assert_eq!(read_class_stm(s, &mut g), S_OK);
        assert_eq!(g, CLSID_ESSAI);
        call0(s, 2);
        let s = create(t, "Annule", false);
        call0(s, 2);
        assert_eq!(stg_revert(t), S_OK);
        assert_eq!(names(t), ["Flux"]);
        let s = create(t, "Garde", false);
        call0(s, 2);
        assert_eq!(stg_commit(t, 0), S_OK);
        call0(t, 2);

        let mut r = 0;
        assert_eq!(
            stg_open_storage_on_ilockbytes(lb, 0, 0x10, 0, 0, &mut r),
            S_OK
        );
        assert_eq!(names(r), ["Flux", "Garde"]);
        call0(r, 2);
        call0(lb, 2);
    }
}

/// Aller-retour du format sur un document plus gros que la DIFAT de
/// l'en-tête (plus de 109 secteurs de FAT) et avec beaucoup d'entrées.
#[test]
fn format_cfb_aller_retour_et_donnees_corrompues() {
    let mut root = Entry::storage("Root Entry");
    root.clsid = CLSID_ESSAI;
    let kids = root.children_mut().unwrap();
    for i in 0..300 {
        kids.push(Entry::stream(&format!("f{i}"), motif(i * 13)));
    }
    kids.push(Entry::stream("Gros", motif(8 << 20)));
    let mut sub = Entry::storage("Sous");
    sub.children_mut()
        .unwrap()
        .push(Entry::stream("Vide", Vec::new()));
    kids.push(sub);
    let bytes = write_cfb(&root);
    assert!(rd32(&bytes, 44) > 109, "DIFAT utilisée");
    let back = parse_cfb(&bytes).unwrap();
    assert_eq!(back.clsid, CLSID_ESSAI);
    assert_eq!(back.children().len(), 302);
    for i in [0usize, 1, 77, 299] {
        let e = find(&back, &[format!("f{i}")]).unwrap();
        assert!(matches!(&e.kind, Kind::Stream(d) if *d == motif(i * 13)));
    }
    assert!(
        matches!(&find(&back, &["gros".into()]).unwrap().kind, Kind::Stream(d) if d.len() == 8 << 20)
    );
    assert!(find(&back, &["Sous".into(), "Vide".into()]).is_some());

    // Données non fiables : jamais de panique.
    assert_eq!(
        parse_cfb(b"pas un docfile").err(),
        Some(STG_E_FILEALREADYEXISTS)
    );
    let small = write_cfb(&Entry::storage("Root Entry"));
    for cut in [512, 600, 1000] {
        let _ = parse_cfb(&small[..cut.min(small.len())]);
    }
    let mut cyc = small.clone();
    let first_dir = rd32(&cyc, 48) as usize;
    let fat0 = rd32(&cyc, 76) as usize;
    let at = (fat0 + 1) * 512 + first_dir * 4;
    cyc[at..at + 4].copy_from_slice(&(first_dir as u32).to_le_bytes());
    assert_eq!(parse_cfb(&cyc).err(), Some(STG_E_DOCFILECORRUPT));
    let mut x = 0x1235u32;
    for _ in 0..200 {
        let mut b = small.clone();
        for _ in 0..8 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let i = (x as usize >> 4) % b.len();
            b[i] = (x >> 24) as u8;
        }
        let _ = parse_cfb(&b);
    }
}
