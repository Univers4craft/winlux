//! Conformité des codes d'erreur Win32 (#2111 et suivantes, famille de #2079).
//!
//! Chaque cas pose d'abord un code « poison » (ERROR_INSUFFICIENT_BUFFER),
//! appelle la fonction comme le ferait un programme, puis compare
//! GetLastError à la valeur qu'exige la documentation publique de la
//! fonction (Microsoft Learn) : le bon code après un échec, 0 après un
//! succès quand la documentation le demande, et jamais la valeur d'un appel
//! précédent. Un écart corrigé ne doit pas revenir : chaque correction de
//! la famille ajoute ses cas ici. Lancé par scripts/test-all.d/2111-codes-erreur.sh.
#![cfg(test)]
use crate::dlls::kernel32 as k;
use crate::process::{last_error, set_last_error};

/// Code laissé par un « appel précédent » ; aucune fonction testée ne doit
/// le rendre par hasard.
const POISON: u32 = 122;

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct Sonde {
    ecarts: Vec<String>,
    _teb: crate::process::TestTeb,
}

impl Sonde {
    fn new() -> Self {
        Sonde {
            ecarts: Vec::new(),
            _teb: crate::process::test_teb(),
        }
    }
    fn poison(&self) {
        set_last_error(POISON);
    }
    /// GetLastError doit valoir `attendu` après l'appel.
    fn code(&mut self, cas: &str, retour: impl std::fmt::Debug, attendu: u32) {
        let obtenu = last_error();
        if obtenu != attendu {
            self.ecarts.push(format!(
                "{cas} : GetLastError = {obtenu}, attendu {attendu} (retour {retour:?})"
            ));
        }
    }
    fn vrai(&mut self, cas: &str, ok: bool, detail: String) {
        if !ok {
            self.ecarts.push(format!("{cas} : {detail}"));
        }
    }
    fn fin(self) {
        assert!(
            self.ecarts.is_empty(),
            "codes d'erreur non conformes :\n{}",
            self.ecarts.join("\n")
        );
    }
}

/// Dossier de travail sous /tmp, vu par les programmes comme Z:\tmp\….
struct Dossier(std::path::PathBuf);
impl Dossier {
    fn new(nom: &str) -> Self {
        let p = std::env::temp_dir().join(format!("winlux-codes-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dossier(p)
    }
    fn win(&self, rel: &str) -> Vec<u16> {
        w(&format!(
            "Z:{}\\{rel}",
            self.0.display().to_string().replace('/', "\\")
        ))
    }
}
impl Drop for Dossier {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn codes_d_erreur_des_fichiers_et_dossiers() {
    let mut s = Sonde::new();
    let d = Dossier::new("fichiers");
    std::fs::create_dir_all(d.0.join("plein")).unwrap();
    std::fs::write(d.0.join("plein/x"), b"x").unwrap();
    std::fs::write(d.0.join("a"), b"a").unwrap();
    std::fs::write(d.0.join("b"), b"b").unwrap();
    unsafe {
        s.poison();
        let h = k::create_file_w(d.win("a").as_ptr(), 0x4000_0000, 0, 0, 2, 0, 0);
        s.code("CreateFileW CREATE_ALWAYS sur un fichier existant", h, 183);
        k::close_handle(h);
        s.poison();
        let h = k::create_file_w(d.win("neuf").as_ptr(), 0x4000_0000, 0, 0, 4, 0, 0);
        s.code("CreateFileW OPEN_ALWAYS sur un fichier neuf", h, 0);
        k::close_handle(h);
        s.poison();
        let h = k::create_file_w(d.win("a").as_ptr(), 0x4000_0000, 0, 0, 1, 0, 0);
        s.code("CreateFileW CREATE_NEW sur un fichier existant", h, 80);
        s.poison();
        let h = k::create_file_w(d.win("absent\\x").as_ptr(), 0x8000_0000, 0, 0, 3, 0, 0);
        s.code("CreateFileW, dossier parent absent", h, 3);

        let mut fd = [0u8; 600];
        let find_first = crate::dlls::winfile::find_first_file_w;
        s.poison();
        let r = find_first(d.win("absent\\*").as_ptr(), fd.as_mut_ptr().cast());
        s.code("FindFirstFileW, dossier absent", r, 3);
        s.poison();
        let r = find_first(d.win("*.zzz").as_ptr(), fd.as_mut_ptr().cast());
        s.code("FindFirstFileW sans résultat", r, 2);
        let hf = find_first(d.win("a").as_ptr(), fd.as_mut_ptr().cast());
        s.poison();
        let r = crate::dlls::winfile::find_next_file_w(hf, fd.as_mut_ptr().cast());
        s.code("FindNextFileW en fin de liste", r, 18);
        crate::dlls::winfile::find_close(hf);

        s.poison();
        let r = k::get_file_attributes_w(d.win("absent.txt").as_ptr());
        s.code("GetFileAttributesW, fichier absent", r, 2);
        s.poison();
        let r = k::delete_file_w(d.win("absent.txt").as_ptr());
        s.code("DeleteFileW, fichier absent", r, 2);
        s.poison();
        let r = k::delete_file_w(d.win("plein").as_ptr());
        s.code("DeleteFileW sur un dossier", r, 5);
        s.poison();
        let r = k::create_directory_w(d.win("plein").as_ptr(), 0);
        s.code("CreateDirectoryW, dossier existant", r, 183);
        s.poison();
        let r = k::create_directory_w(d.win("a").as_ptr(), 0);
        s.code("CreateDirectoryW sur un fichier existant", r, 183);
        s.poison();
        let r = k::remove_directory_w(d.win("plein").as_ptr());
        s.code("RemoveDirectoryW, dossier non vide", r, 145);
        s.poison();
        let r = k::remove_directory_w(d.win("a").as_ptr());
        s.code("RemoveDirectoryW sur un fichier", r, 267);
        s.poison();
        let r = k::set_current_directory_w(d.win("absent").as_ptr());
        s.code("SetCurrentDirectoryW, dossier absent", r, 2);
        s.poison();
        let r = k::copy_file_w(d.win("a").as_ptr(), d.win("b").as_ptr(), 1);
        s.code("CopyFileW bFailIfExists, destination existante", r, 80);
        s.poison();
        let r = k::copy_file_w(d.win("absent").as_ptr(), d.win("c").as_ptr(), 0);
        s.code("CopyFileW, source absente", r, 2);
        s.poison();
        let r = k::move_file_ex_w(d.win("a").as_ptr(), d.win("b").as_ptr(), 0);
        s.code("MoveFileExW, destination existante", r, 183);

        let h = k::create_file_w(d.win("a").as_ptr(), 0x8000_0000, 1, 0, 3, 0, 0);
        let mut buf = [0u8; 8];
        let mut n = 0u32;
        k::read_file(h, buf.as_mut_ptr(), 8, &mut n, 0);
        let r = k::read_file(h, buf.as_mut_ptr(), 8, &mut n, 0);
        s.vrai(
            "ReadFile en fin de fichier",
            r == 1 && n == 0,
            format!("retour {r}, {n} octet(s) ; attendu TRUE et 0 octet"),
        );
        s.poison();
        let mut pos = 0i64;
        let r = k::set_file_pointer_ex(h, -10, &mut pos, 0);
        s.code("SetFilePointerEx vers une position négative", r, 131);
        k::close_handle(h);
    }
    s.fin();
}

#[test]
fn codes_d_erreur_des_tubes() {
    let mut s = Sonde::new();
    unsafe {
        // Tube anonyme dont le lecteur est fermé (#2111).
        let (mut rd, mut wr) = (0usize, 0usize);
        assert_eq!(k::create_pipe(&mut rd, &mut wr, std::ptr::null(), 0), 1);
        k::close_handle(rd);
        let mut n = 0u32;
        s.poison();
        let r = k::write_file(wr, b"abc".as_ptr(), 3, &mut n, 0);
        s.code("WriteFile dans un tube sans lecteur", r, 232);
        k::close_handle(wr);
        let (mut rd, mut wr) = (0usize, 0usize);
        assert_eq!(k::create_pipe(&mut rd, &mut wr, std::ptr::null(), 0), 1);
        k::close_handle(wr);
        let mut buf = [0u8; 4];
        s.poison();
        let r = k::read_file(rd, buf.as_mut_ptr(), 4, &mut n, 0);
        s.code("ReadFile dans un tube sans écrivain", r, 109);
        k::close_handle(rd);

        let nom = w(&format!("\\\\.\\pipe\\winlux-codes-{}", std::process::id()));
        let srv = k::create_named_pipe_w(nom.as_ptr(), 3 | 0x0008_0000, 0, 2, 512, 512, 0, 0);
        s.poison();
        let r = k::create_named_pipe_w(nom.as_ptr(), 3 | 0x0008_0000, 0, 2, 512, 512, 0, 0);
        s.code(
            "CreateNamedPipeW FILE_FLAG_FIRST_PIPE_INSTANCE répété",
            r,
            5,
        );
        let cli = k::create_file_w(nom.as_ptr(), 0xC000_0000, 0, 0, 3, 0, 0);
        s.poison();
        let r = k::connect_named_pipe(srv, 0);
        s.code("ConnectNamedPipe, client déjà connecté", r, 535);
        k::close_handle(cli);
        let (mut a, mut t, mut l) = (0u32, 0u32, 0u32);
        s.poison();
        let r = k::peek_named_pipe(srv, std::ptr::null_mut(), 0, &mut a, &mut t, &mut l);
        s.code("PeekNamedPipe, client parti", r, 109);
        k::close_handle(srv);
    }
    s.fin();
}

/// errno Linux → code Windows : plus de retombée sur le générique 31
/// (ERROR_GEN_FAILURE) quand un équivalent exact existe (#2111).
#[test]
fn traduction_des_errno_sans_code_generique() {
    let cas = [
        (libc::EPIPE, 232),
        (libc::ECONNRESET, 109),
        (libc::EMFILE, 4),
        (libc::ENFILE, 4),
        (libc::ENOMEM, 8),
        (libc::EBADF, 6),
        (libc::EIO, 1117),
        (libc::EFBIG, 223),
        (libc::EOPNOTSUPP, 50),
        (libc::ENOSYS, 50),
        (libc::ENOENT, 2),
        (libc::EACCES, 5),
        (libc::EEXIST, 183),
    ];
    for (errno, attendu) in cas {
        let e = std::io::Error::from_raw_os_error(errno);
        assert_eq!(k::io_error_code(&e), attendu, "errno {errno} ({e})");
    }
}

#[test]
fn codes_d_erreur_de_la_synchronisation() {
    let mut s = Sonde::new();
    unsafe {
        s.poison();
        let r = k::open_event_a(0x1F_0003, 0, c"winlux-codes-absent".as_ptr().cast());
        s.code("OpenEventA, objet absent", r, 2);
        let m = k::create_mutex_a(0, 0, std::ptr::null());
        s.poison();
        let r = k::release_mutex(m);
        s.code("ReleaseMutex sans le posséder", r, 288);
        k::close_handle(m);
        let sem = k::create_semaphore_a(0, 1, 1, std::ptr::null());
        s.poison();
        let r = k::release_semaphore(sem, 1, std::ptr::null_mut());
        s.code("ReleaseSemaphore au-delà du maximum", r, 298);
        k::close_handle(sem);
        s.poison();
        let r = k::close_handle(0x7777_0000);
        s.code("CloseHandle, handle invalide", r, 6);
        s.poison();
        let r = k::set_event(0x7777_0000);
        s.code("SetEvent, handle invalide", r, 6);
        s.poison();
        let r = k::wait_for_single_object(0x7777_0000, 0);
        s.code("WaitForSingleObject, handle invalide", r, 6);
        let zero = [0usize; 65];
        s.poison();
        let r = k::wait_for_multiple_objects(0, zero.as_ptr(), 0, 0);
        s.code("WaitForMultipleObjects avec 0 handle", r, 87);
        s.poison();
        let r = k::wait_for_multiple_objects(65, zero.as_ptr(), 0, 0);
        s.code("WaitForMultipleObjects avec 65 handles", r, 87);
        let port = k::create_io_completion_port(usize::MAX, 0, 0, 0);
        let (mut b, mut key) = (0u32, 0usize);
        let mut ov: *mut std::ffi::c_void = std::ptr::null_mut();
        s.poison();
        let r = k::get_queued_completion_status(port, &mut b, &mut key, &mut ov, 0);
        s.code("GetQueuedCompletionStatus, délai écoulé", r, 258);
        k::close_handle(port);
        let mut code = 0u32;
        s.poison();
        let r = k::get_exit_code_thread(0x7777_0000, &mut code);
        s.code("GetExitCodeThread, handle invalide", r, 6);
        let mut dup = 0usize;
        s.poison();
        let r = k::duplicate_handle(usize::MAX, 0x7777_0000, usize::MAX, &mut dup, 0, 0, 2);
        s.code("DuplicateHandle, handle invalide", r, 6);
        s.poison();
        let r = k::open_process(0x1000, 0, 0x7FFF_FFF0);
        s.code("OpenProcess, processus absent", r, 87);
        s.poison();
        let r = k::virtual_free(0x1000 as *mut _, 0, 0x8000);
        s.code("VirtualFree, adresse non réservée", r, 487);
        let i = k::tls_alloc();
        s.poison();
        let r = k::tls_get_value(i);
        s.code("TlsGetValue réussi (documenté : remet 0)", r, 0);
    }
    s.fin();
}

#[test]
fn codes_d_erreur_des_modules_du_texte_et_du_registre() {
    let mut s = Sonde::new();
    unsafe {
        s.poison();
        let r = k::load_library_w(w("winlux-codes-absent.dll").as_ptr());
        s.code("LoadLibraryW, DLL absente", r, 126);
        let k32 = k::load_library_w(w("kernel32.dll").as_ptr());
        s.poison();
        let r = k::get_proc_address(k32, c"WinluxFonctionAbsente".as_ptr().cast());
        s.code("GetProcAddress, nom absent", r, 127);
        s.poison();
        let mut eb = [0u16; 8];
        let r =
            k::get_environment_variable_w(w("WINLUX_CODES_ABSENTE").as_ptr(), eb.as_mut_ptr(), 8);
        s.code("GetEnvironmentVariableW, variable absente", r, 203);

        let api = crate::intl::api::multi_byte_to_wide_char;
        let mut wb = [0u16; 8];
        s.poison();
        let r = api(65001, 8, [0xC3u8, 0x28].as_ptr(), 2, wb.as_mut_ptr(), 8);
        s.code(
            "MultiByteToWideChar, UTF-8 invalide et MB_ERR_INVALID_CHARS",
            r,
            1113,
        );
        // Ici le poison serait la réponse attendue : on part de 0.
        set_last_error(0);
        let r = api(65001, 0, b"abcdef".as_ptr(), 6, wb.as_mut_ptr(), 2);
        s.code("MultiByteToWideChar, tampon trop court", r, 122);
        let src: Vec<u16> = "abcdef".encode_utf16().collect();
        let mut cb = [0u8; 2];
        set_last_error(0);
        let r = crate::intl::api::wide_char_to_multi_byte(
            65001,
            0,
            src.as_ptr(),
            6,
            cb.as_mut_ptr(),
            2,
            std::ptr::null(),
            std::ptr::null_mut(),
        );
        s.code("WideCharToMultiByte, tampon trop court", r, 122);

        let reg = crate::dlls::advapi32::reg_open_key_ex_w;
        let mut hk = 0usize;
        let r = reg(
            0x8000_0001,
            w("Software\\WinluxCodesAbsente").as_ptr(),
            0,
            0x2_0019,
            &mut hk,
        );
        s.vrai(
            "RegOpenKeyExW, clé absente",
            r == 2,
            format!("retour {r}, attendu 2"),
        );
        let mut disp = 0u32;
        let cle = w(&format!("Software\\WinluxCodes{}", std::process::id()));
        crate::dlls::advapi32::reg_create_key_ex_w(
            0x8000_0001,
            cle.as_ptr(),
            0,
            std::ptr::null(),
            0,
            0xF_003F,
            0,
            &mut hk,
            &mut disp,
        );
        let val = w("abcdefghij");
        crate::dlls::advapi32::reg_set_value_ex_w(
            hk,
            w("v").as_ptr(),
            0,
            1,
            val.as_ptr().cast(),
            22,
        );
        let q = crate::dlls::advapi32::reg_query_value_ex_w;
        let (mut kind, mut db) = (0u32, [0u8; 4]);
        let mut sz = 4u32;
        let r = q(
            hk,
            w("v").as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            db.as_mut_ptr(),
            &mut sz,
        );
        s.vrai(
            "RegQueryValueExW, tampon trop court",
            r == 234 && sz == 22,
            format!("retour {r}, taille {sz} ; attendu 234 (ERROR_MORE_DATA) et 22"),
        );
        let mut sz = 0u32;
        let r = q(
            hk,
            w("v").as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            std::ptr::null_mut(),
            &mut sz,
        );
        s.vrai(
            "RegQueryValueExW sans tampon",
            r == 0 && sz == 22,
            format!("retour {r}, taille {sz} ; attendu 0 et 22"),
        );
        let mut sz = 4u32;
        let r = q(
            hk,
            w("absente").as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            db.as_mut_ptr(),
            &mut sz,
        );
        s.vrai(
            "RegQueryValueExW, valeur absente",
            r == 2,
            format!("retour {r}, attendu 2"),
        );
        crate::dlls::advapi32::reg_close_key(hk);
        crate::dlls::advapi32::reg_delete_key_w(0x8000_0001, cle.as_ptr());
    }
    s.fin();
}
