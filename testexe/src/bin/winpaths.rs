//! Chemins Windows ↔ Linux vus par un vrai .exe : dossiers connus, accents,
//! noms interdits sous Windows, casse, chemins longs, liens, lecture seule.
//!
//! `scripts/test-runtime.sh` lance ce test avec un dossier personnel jetable
//! dont le dossier Documents XDG s'appelle « Mes documents », y dépose
//! `réunion 10:30.txt`, puis vérifie côté Linux ce que l'appli a écrit.
#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr::null_mut;
use winlux_testexe::*;

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    fn SHGetKnownFolderPath(id: *const u8, flags: u32, token: Handle, out: *mut *mut u16) -> i32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn ExpandEnvironmentStringsW(src: *const u16, dst: *mut u16, size: u32) -> u32;
    fn CreateSymbolicLinkW(link: *const u16, target: *const u16, flags: u32) -> u8;
    fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
}

/// FOLDERID_Documents {FDD39AD0-238F-46AF-ADB4-6C85480369C7}.
const FOLDERID_DOCUMENTS: [u8; 16] = [
    0xD0, 0x9A, 0xD3, 0xFD, 0x8F, 0x23, 0xAF, 0x46, 0xAD, 0xB4, 0x6C, 0x85, 0x48, 0x03, 0x69, 0xC7,
];

/// Petit tampon UTF-16 terminé par zéro.
struct W {
    buf: [u16; 1024],
    len: usize,
}

impl W {
    fn new() -> W {
        W {
            buf: [0; 1024],
            len: 0,
        }
    }
    fn of(s: &str) -> W {
        let mut w = W::new();
        w.push(s);
        w
    }
    fn push(&mut self, s: &str) -> &mut W {
        for c in s.encode_utf16() {
            self.buf[self.len] = c;
            self.len += 1;
        }
        self.buf[self.len] = 0;
        self
    }
    fn push_w(&mut self, w: &[u16]) -> &mut W {
        for &c in w.iter().take_while(|&&c| c != 0) {
            self.buf[self.len] = c;
            self.len += 1;
        }
        self.buf[self.len] = 0;
        self
    }
    fn ptr(&self) -> *const u16 {
        self.buf.as_ptr()
    }
    fn slice(&self) -> &[u16] {
        &self.buf[..self.len]
    }
    fn from_ptr(p: *const u16) -> W {
        let mut w = W::new();
        unsafe {
            let mut i = 0;
            while *p.add(i) != 0 && i < 1023 {
                w.buf[i] = *p.add(i);
                i += 1;
            }
            w.len = i;
        }
        w
    }
}

unsafe fn write_file(path: &W, data: &[u8]) -> bool {
    let h = CreateFileW(path.ptr(), 0x4000_0000, 0, null_mut(), 2, 0x80, null_mut());
    if h as usize == usize::MAX {
        return false;
    }
    let mut n = 0;
    let ok = WriteFile(h, data.as_ptr(), data.len() as u32, &mut n, null_mut()) != 0;
    CloseHandle(h);
    ok && n == data.len() as u32
}

unsafe fn read_file(path: &W, out: &mut [u8]) -> Option<usize> {
    let h = CreateFileW(path.ptr(), 0x8000_0000, 1, null_mut(), 3, 0x80, null_mut());
    if h as usize == usize::MAX {
        return None;
    }
    let mut n = 0;
    let ok = ReadFile(h, out.as_mut_ptr(), out.len() as u32, &mut n, null_mut()) != 0;
    CloseHandle(h);
    ok.then_some(n as usize)
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        // 1. Profil et dossiers connus.
        let mut profile = W::new();
        profile.len = GetEnvironmentVariableW(W::of("USERPROFILE").ptr(), profile.buf.as_mut_ptr(), 1024) as usize;
        check!(
            profile.len > 9 && profile.slice().starts_with(W::of("C:\\Users\\").slice()),
            "USERPROFILE = C:\\Users\\<nom>"
        );
        let mut raw: *mut u16 = null_mut();
        let hr = SHGetKnownFolderPath(FOLDERID_DOCUMENTS.as_ptr(), 0, null_mut(), &mut raw);
        let docs = if hr == 0 { W::from_ptr(raw) } else { W::new() };
        if !raw.is_null() {
            CoTaskMemFree(raw as *mut c_void);
        }
        let mut expected = W::new();
        expected.push_w(profile.slice()).push("\\Documents");
        check!(hr == 0 && docs.slice() == expected.slice(), "SHGetKnownFolderPath(Documents)");

        let mut expanded = W::new();
        let n = ExpandEnvironmentStringsW(W::of("%USERPROFILE%\\AppData").ptr(), expanded.buf.as_mut_ptr(), 1024);
        expanded.len = n.saturating_sub(1) as usize;
        let mut appdata = W::new();
        appdata.push_w(profile.slice()).push("\\AppData");
        check!(expanded.slice() == appdata.slice(), "ExpandEnvironmentStringsW(%USERPROFILE%)");

        // 2. Accents via les API « W » : le fichier doit arriver dans
        //    ~/Mes documents avec son vrai nom (vérifié par le script).
        let mut report = W::new();
        report.push_w(docs.slice()).push("\\Été 2026 — rapport.txt");
        check!(write_file(&report, b"bonjour"), "CreateFileW avec accents dans Documents");

        // 3. Casse : on relit en majuscules, séparateurs mélangés.
        let mut upper = W::new();
        upper.push_w(profile.slice()).push("/DOCUMENTS//ÉTÉ 2026 — RAPPORT.TXT");
        let mut buf = [0u8; 32];
        check!(read_file(&upper, &mut buf) == Some(7) && &buf[..7] == b"bonjour", "relecture insensible à la casse");

        // 4. Fichier Linux au nom interdit sous Windows (« : »).
        let mut pattern = W::new();
        pattern.push_w(docs.slice()).push("\\réunion*");
        let mut data: Win32FindDataW = core::mem::zeroed();
        let search = FindFirstFileW(pattern.ptr(), &mut data);
        let listed = W::from_ptr(data.file_name.as_ptr());
        let found = search as usize != usize::MAX;
        if found {
            FindClose(search);
        }
        let no_colon = !listed.slice().contains(&(b':' as u16));
        let mut colon = W::new();
        colon.push_w(docs.slice()).push("\\").push_w(listed.slice());
        let mut buf = [0u8; 32];
        check!(
            found && no_colon && read_file(&colon, &mut buf) == Some(5) && &buf[..5] == b"linux",
            "nom Linux avec « : » listé puis ouvert"
        );

        // 5. GetFullPathName travaille sur le texte.
        let mut full = W::new();
        let mut messy = W::new();
        messy.push_w(profile.slice()).push("/../");
        // Remonte puis redescend dans le profil par son nom.
        let name_start = profile.slice().iter().rposition(|&c| c == b'\\' as u16).unwrap_or(0) + 1;
        messy.push_w(&profile.slice()[name_start..]).push("\\.\\Documents//a.txt. ");
        full.len = GetFullPathNameW(messy.ptr(), 1024, full.buf.as_mut_ptr(), null_mut()) as usize;
        let mut want = W::new();
        want.push_w(docs.slice()).push("\\a.txt");
        check!(full.slice() == want.slice(), "GetFullPathNameW (.., ., //, point final)");

        // 6. Périphérique NUL.
        check!(write_file(&W::of("NUL"), b"rien"), "écriture dans NUL");

        // 7. Liens symboliques : attribut REPARSE_POINT, suppression du lien seul.
        let mut target = W::new();
        target.push_w(docs.slice()).push("\\cible");
        let mut link = W::new();
        link.push_w(docs.slice()).push("\\lien");
        let made = CreateDirectoryW(target.ptr(), null_mut()) != 0
            && CreateSymbolicLinkW(link.ptr(), W::of("cible").ptr(), 1) != 0;
        let attrs = GetFileAttributesW(link.ptr());
        check!(made && attrs != u32::MAX && attrs & 0x410 == 0x410, "lien vers dossier = REPARSE_POINT|DIRECTORY");
        check!(
            RemoveDirectoryW(link.ptr()) != 0 && GetFileAttributesW(target.ptr()) & 0x10 != 0,
            "RemoveDirectoryW supprime le lien, pas la cible"
        );
        RemoveDirectoryW(target.ptr());

        // 8. Chemin long (> 260 caractères) avec \\?\ puis sans.
        let mut long = W::new();
        long.push("\\\\?\\").push_w(docs.slice());
        let mut ok = true;
        for _ in 0..8 {
            long.push("\\un-dossier-avec-un-nom-plutot-long");
            ok &= CreateDirectoryW(long.ptr(), null_mut()) != 0;
        }
        long.push("\\fin.txt");
        ok &= long.len > 300 && write_file(&long, b"x");
        let mut plain = W::new();
        plain.push_w(&long.slice()[4..]);
        check!(ok && GetFileAttributesW(plain.ptr()) != u32::MAX, "chemin de plus de 260 caractères");

        // 9. Lecture seule : vraie protection, et retour sans « écriture pour tous ».
        let ro = SetFileAttributesW(report.ptr(), 1) != 0 && GetFileAttributesW(report.ptr()) & 1 != 0;
        let h = CreateFileW(report.ptr(), 0x4000_0000, 0, null_mut(), 3, 0x80, null_mut());
        let refused = h as usize == usize::MAX && GetLastError() == 5;
        let back = SetFileAttributesW(report.ptr(), 0x80) != 0 && GetFileAttributesW(report.ptr()) & 1 == 0;
        check!(ro && refused && back, "lecture seule posée, respectée puis retirée");
    }
    finish()
}
