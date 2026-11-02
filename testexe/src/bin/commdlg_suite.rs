//! Test PE des boîtes communes de comdlg32 : « Ouvrir », « Enregistrer
//! sous », « Couleur » et « Police », appelées comme le fait PuTTY (clé
//! privée, journal, couleurs du terminal, police à espacement fixe).
//!
//! Sans affichage, les variables WINLUX_FILEDLG / WINLUX_CHOOSECOLOR /
//! WINLUX_CHOOSEFONT (posées ici par SetEnvironmentVariableW) font le choix
//! de l'utilisateur. Avec `WINLUX_CONTROLS_SHOW=<ms>`, chaque boîte reste
//! affichée ce temps-là (capture d'écran) puis est validée par Entrée.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use core::ffi::{c_char, c_void};
use winlux_testexe::*;

#[link(name = "kernel32", kind = "raw-dylib")]
extern "C" {
    fn GetEnvironmentVariableA(name: *const c_char, buf: *mut c_char, size: u32) -> u32;
    fn GetTempPathW(size: u32, buf: *mut u16) -> u32;
}

#[link(name = "comdlg32", kind = "raw-dylib")]
extern "C" {
    fn GetOpenFileNameW(ofn: *mut OpenFileName) -> i32;
    fn GetSaveFileNameW(ofn: *mut OpenFileName) -> i32;
    fn GetOpenFileNameA(ofn: *mut OpenFileName) -> i32;
    fn ChooseColorW(cc: *mut ChooseColor) -> i32;
    fn ChooseFontW(cf: *mut ChooseFont) -> i32;
    fn CommDlgExtendedError() -> u32;
}

#[repr(C)]
struct OpenFileName {
    size: u32,
    owner: Handle,
    instance: Handle,
    filter: *const u16,
    custom_filter: *mut u16,
    max_cust_filter: u32,
    filter_index: u32,
    file: *mut u16,
    max_file: u32,
    file_title: *mut u16,
    max_file_title: u32,
    initial_dir: *const u16,
    title: *const u16,
    flags: u32,
    file_offset: u16,
    file_extension: u16,
    def_ext: *const u16,
    cust_data: isize,
    hook: usize,
    template: *const u16,
    reserved_ptr: *mut c_void,
    reserved: u32,
    flags_ex: u32,
}

#[repr(C)]
struct ChooseColor {
    size: u32,
    owner: Handle,
    instance: Handle,
    result: u32,
    custom: *mut u32,
    flags: u32,
    cust_data: isize,
    hook: usize,
    template: *const u16,
}

#[repr(C)]
struct LogFontW {
    height: i32,
    width: i32,
    escapement: i32,
    orientation: i32,
    weight: i32,
    italic: u8,
    underline: u8,
    strikeout: u8,
    charset: u8,
    out_precision: u8,
    clip_precision: u8,
    quality: u8,
    pitch_and_family: u8,
    face: [u16; 32],
}

#[repr(C)]
struct ChooseFont {
    size: u32,
    owner: Handle,
    dc: Handle,
    logfont: *mut LogFontW,
    point_size: i32,
    flags: u32,
    colors: u32,
    cust_data: isize,
    hook: usize,
    template: *const u16,
    instance: Handle,
    style: *mut u16,
    font_type: u16,
    size_min: i32,
    size_max: i32,
}

static mut SHOW_MS: u32 = 0;
static mut OWNER: Handle = core::ptr::null_mut();

unsafe extern "win64" fn wnd_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    DefWindowProcA(hwnd, msg, wparam, lparam)
}

/// Valide la boîte affichée comme un utilisateur : touche Entrée.
unsafe extern "win64" fn press_enter(hwnd: Handle, _msg: u32, id: usize, _time: u32) {
    KillTimer(hwnd, id);
    PostMessageW(GetFocus(), 0x0100, 0x0D, 0);
}

/// Avec affichage : Entrée après SHOW_MS millisecondes.
unsafe fn arm() {
    if SHOW_MS > 0 {
        SetTimer(OWNER, 7, SHOW_MS, press_enter as *const () as usize);
    }
}

fn w<const N: usize>(s: &[u8; N]) -> [u16; N] {
    let mut out = [0u16; N];
    for (o, &b) in out.iter_mut().zip(s.iter()) {
        *o = b as u16;
    }
    out
}

fn len(s: &[u16]) -> usize {
    s.iter().position(|&c| c == 0).unwrap_or(s.len())
}

fn ends_with(s: &[u16], tail: &[u8]) -> bool {
    let n = len(s);
    n >= tail.len()
        && s[n - tail.len()..n]
            .iter()
            .zip(tail)
            .all(|(&a, &b)| a == b as u16)
}

unsafe fn ofn(file: &mut [u16], dir: *const u16, filter: *const u16) -> OpenFileName {
    OpenFileName {
        size: core::mem::size_of::<OpenFileName>() as u32,
        owner: OWNER,
        instance: core::ptr::null_mut(),
        filter,
        custom_filter: core::ptr::null_mut(),
        max_cust_filter: 0,
        filter_index: 1,
        file: file.as_mut_ptr(),
        max_file: file.len() as u32,
        file_title: core::ptr::null_mut(),
        max_file_title: 0,
        initial_dir: dir,
        title: core::ptr::null(),
        flags: 0,
        file_offset: 0,
        file_extension: 0,
        def_ext: core::ptr::null(),
        cust_data: 0,
        hook: 0,
        template: core::ptr::null(),
        reserved_ptr: core::ptr::null_mut(),
        reserved: 0,
        flags_ex: 0,
    }
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let mut wait = [0u8; 16];
        let n = GetEnvironmentVariableA(
            c"WINLUX_CONTROLS_SHOW".as_ptr(),
            wait.as_mut_ptr().cast(),
            16,
        );
        for &b in &wait[..n as usize] {
            if b.is_ascii_digit() {
                SHOW_MS = SHOW_MS * 10 + (b - b'0') as u32;
            }
        }
        let class = c"WinluxCommdlgSuite";
        let wc = WndClassExA {
            size: core::mem::size_of::<WndClassExA>() as u32,
            style: 0,
            wnd_proc: wnd_proc as *const () as usize,
            cls_extra: 0,
            wnd_extra: 0,
            instance: core::ptr::null_mut(),
            icon: core::ptr::null_mut(),
            cursor: core::ptr::null_mut(),
            background: GetSysColorBrush(15),
            menu_name: core::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: core::ptr::null_mut(),
        };
        RegisterClassExA(&wc);
        OWNER = CreateWindowExA(
            0,
            class.as_ptr(),
            c"PuTTY".as_ptr(),
            0x00CF_0000,
            40,
            40,
            300,
            200,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        check!(!OWNER.is_null(), "fenetre proprietaire");

        // Fichier de test dans %TEMP%.
        let mut temp = [0u16; 260];
        let tl = GetTempPathW(260, temp.as_mut_ptr()) as usize;
        check!(tl > 0, "GetTempPathW");
        let name = b"winlux-cle-test.ppk";
        let mut path = [0u16; 300];
        path[..tl].copy_from_slice(&temp[..tl]);
        for (i, &b) in name.iter().enumerate() {
            path[tl + i] = b as u16;
        }
        let h = CreateFileW(
            path.as_ptr(),
            0x4000_0000,
            0,
            core::ptr::null_mut(),
            2,
            0x80,
            core::ptr::null_mut(),
        );
        CloseHandle(h);

        let filter = w(b"Cle privee PuTTY (*.ppk)\0*.ppk\0Tous les fichiers (*.*)\0*.*\0\0");
        let env_file = w(b"WINLUX_FILEDLG\0");
        let env_value = w(b"winlux-cle-test.ppk\0");
        SetEnvironmentVariableW(env_file.as_ptr(), env_value.as_ptr());

        // « Ouvrir » : clé privée qui doit exister.
        let mut file = [0u16; 300];
        for (i, &b) in name.iter().enumerate() {
            file[i] = b as u16;
        }
        let mut o = ofn(&mut file, temp.as_ptr(), filter.as_ptr());
        o.flags = 0x1000 | 0x800; // FILEMUSTEXIST | PATHMUSTEXIST
        arm();
        check!(GetOpenFileNameW(&mut o) == 1, "GetOpenFileNameW");
        check!(ends_with(&file, b"\\winlux-cle-test.ppk"), "chemin choisi");
        check!(file[1] == b':' as u16, "chemin Windows (X:)");
        check!(
            o.file_offset as usize == len(&file) - name.len(),
            "nFileOffset"
        );
        check!(
            o.file_extension as usize == len(&file) - 3,
            "nFileExtension"
        );

        // Fichier absent avec FILEMUSTEXIST : la boîte refuse puis s'annule.
        if SHOW_MS == 0 {
            let missing = w(b"absent-introuvable.ppk\0");
            SetEnvironmentVariableW(env_file.as_ptr(), missing.as_ptr());
            let mut file2 = [0u16; 300];
            let mut o2 = ofn(&mut file2, temp.as_ptr(), filter.as_ptr());
            o2.flags = 0x1000;
            check!(GetOpenFileNameW(&mut o2) == 0, "fichier absent refuse");
            check!(CommDlgExtendedError() == 0, "annulation sans erreur");
            SetEnvironmentVariableW(env_file.as_ptr(), env_value.as_ptr());

            // Tampon trop petit : FNERR_BUFFERTOOSMALL.
            let mut small = [0u16; 8];
            let mut o3 = ofn(&mut small, temp.as_ptr(), filter.as_ptr());
            check!(GetOpenFileNameW(&mut o3) == 0, "tampon trop petit");
            check!(CommDlgExtendedError() == 0x3003, "FNERR_BUFFERTOOSMALL");

            // Version ANSI.
            let mut file_a = [0u8; 300];
            let mut oa = ofn(&mut [0u16; 1], core::ptr::null(), core::ptr::null());
            oa.file = file_a.as_mut_ptr().cast();
            oa.max_file = 300;
            let mut temp_a = [0u8; 261];
            for i in 0..tl {
                temp_a[i] = temp[i] as u8;
            }
            oa.initial_dir = temp_a.as_ptr().cast();
            check!(GetOpenFileNameA(&mut oa) == 1, "GetOpenFileNameA");
            let n = file_a.iter().position(|&c| c == 0).unwrap_or(0);
            check!(
                file_a[..n].ends_with(b"\\winlux-cle-test.ppk"),
                "chemin ANSI"
            );
        }

        // « Enregistrer sous » : extension par défaut ajoutée.
        let log_name = w(b"session-putty\0");
        SetEnvironmentVariableW(env_file.as_ptr(), log_name.as_ptr());
        let mut out = [0u16; 300];
        out[..log_name.len()].copy_from_slice(&log_name);
        let log_filter = w(b"Journaux (*.log)\0*.log\0\0");
        let def = w(b"log\0");
        let mut s = ofn(&mut out, temp.as_ptr(), log_filter.as_ptr());
        s.def_ext = def.as_ptr();
        s.flags = 0x2; // OVERWRITEPROMPT
        arm();
        check!(GetSaveFileNameW(&mut s) == 1, "GetSaveFileNameW");
        check!(
            ends_with(&out, b"\\session-putty.log"),
            "extension par defaut"
        );

        // Couleur.
        let env = w(b"WINLUX_CHOOSECOLOR\0");
        let val = w(b"FF8000\0");
        SetEnvironmentVariableW(env.as_ptr(), val.as_ptr());
        let mut custom = [0x00FF_FFFFu32; 16];
        let mut cc = ChooseColor {
            size: core::mem::size_of::<ChooseColor>() as u32,
            owner: OWNER,
            instance: core::ptr::null_mut(),
            result: 0x0000_80FF,
            custom: custom.as_mut_ptr(),
            flags: 0x1 | 0x2, // RGBINIT | FULLOPEN
            cust_data: 0,
            hook: 0,
            template: core::ptr::null(),
        };
        arm();
        check!(ChooseColorW(&mut cc) == 1, "ChooseColorW");
        check!(cc.result == 0x0000_80FF, "couleur choisie (COLORREF)");

        // Police à espacement fixe, comme PuTTY.
        let env = w(b"WINLUX_CHOOSEFONT\0");
        let val = w(b",12,gras\0");
        SetEnvironmentVariableW(env.as_ptr(), val.as_ptr());
        let mut lf: LogFontW = core::mem::zeroed();
        lf.height = -13;
        lf.weight = 400;
        let face = w(b"Courier New\0");
        lf.face[..face.len()].copy_from_slice(&face);
        let mut cf = ChooseFont {
            size: core::mem::size_of::<ChooseFont>() as u32,
            owner: OWNER,
            dc: core::ptr::null_mut(),
            logfont: &mut lf,
            point_size: 0,
            flags: 0x4000 | 0x10000 | 0x40 | 0x1, // FIXEDPITCHONLY | FORCEFONTEXIST | INITTOLOGFONTSTRUCT | SCREENFONTS
            colors: 0,
            cust_data: 0,
            hook: 0,
            template: core::ptr::null(),
            instance: core::ptr::null_mut(),
            style: core::ptr::null_mut(),
            font_type: 0,
            size_min: 0,
            size_max: 0,
        };
        arm();
        check!(ChooseFontW(&mut cf) == 1, "ChooseFontW");
        if SHOW_MS == 0 {
            check!(cf.point_size == 120, "iPointSize 12 pt");
            check!(lf.height == -16, "lfHeight pour 12 pt");
            check!(lf.weight == 700, "gras");
        } else {
            check!(cf.point_size == 100, "iPointSize 10 pt");
        }
        check!(lf.face[0] != 0, "nom de police rendu");

        DeleteFileW(path.as_ptr());
        ExitProcess(if FAILURES == 0 { 0 } else { 1 });
    }
}
