//! Élévation (UAC) et droits des fichiers vus par un .exe dont le manifeste
//! demande `requireAdministrator` (lié avec /manifestuac par
//! `scripts/build-tests.sh`).
//!
//! `scripts/test-runtime.sh` le lance deux fois : avec `WINLUX_UAC=allow`
//! (l'appli doit se voir administrateur et ses fichiers rester à
//! l'utilisateur) puis `WINLUX_UAC=deny` (le programme ne doit pas démarrer :
//! code 1223, ERROR_CANCELLED).
#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr::null_mut;
use winlux_testexe::*;

#[link(name = "advapi32", kind = "raw-dylib")]
extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(token: Handle, class: u32, buf: *mut u8, len: u32, ret: *mut u32) -> i32;
    fn CheckTokenMembership(token: Handle, sid: *const u8, member: *mut i32) -> i32;
    fn EqualSid(a: *const u8, b: *const u8) -> i32;
    fn GetNamedSecurityInfoW(
        name: *const u16,
        kind: u32,
        info: u32,
        owner: *mut *mut u8,
        group: *mut *mut u8,
        dacl: *mut *mut u8,
        sacl: *mut *mut u8,
        sd: *mut *mut u8,
    ) -> u32;
    fn GetAce(acl: *const u8, index: u32, ace: *mut *mut u8) -> i32;
}

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn LocalFree(p: *mut c_void) -> *mut c_void;
}

/// S-1-5-32-544 (Administrateurs).
const ADMINISTRATORS: [u8; 16] = [1, 2, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0, 0x20, 2, 0, 0];

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        check!(IsUserAnAdmin() == 1, "IsUserAnAdmin après accord UAC");
        let mut token: Handle = null_mut();
        let opened = OpenProcessToken(GetCurrentProcess(), 8, &mut token) != 0;
        let mut elevation = [0u8; 4];
        let mut len = 0u32;
        let elevated = opened
            && GetTokenInformation(token, 20, elevation.as_mut_ptr(), 4, &mut len) != 0
            && u32::from_le_bytes(elevation) == 1;
        check!(elevated, "TokenElevation = 1");
        let mut member = 0;
        check!(
            CheckTokenMembership(null_mut(), ADMINISTRATORS.as_ptr(), &mut member) != 0 && member == 1,
            "membre des Administrateurs"
        );

        // Fichier créé par l'appli « administrateur » : il appartient à
        // l'utilisateur (même SID que le jeton) et lui donne le contrôle total.
        let mut user = [0u8; 128];
        let got_user = GetTokenInformation(token, 1, user.as_mut_ptr(), 128, &mut len) != 0;
        let user_sid = core::ptr::read_unaligned(user.as_ptr() as *const *const u8);
        let mut name = [0u16; 64];
        for (i, c) in "Z:\\tmp\\winlux-uac-test.txt".encode_utf16().enumerate() {
            name[i] = c;
        }
        let h = CreateFileW(name.as_ptr(), 0x4000_0000, 0, null_mut(), 2, 0x80, null_mut());
        let created = h as usize != usize::MAX;
        if created {
            CloseHandle(h);
        }
        let (mut owner, mut dacl, mut sd) = (null_mut(), null_mut(), null_mut());
        let rc = GetNamedSecurityInfoW(name.as_ptr(), 1, 1 | 4, &mut owner, null_mut(), &mut dacl, null_mut(), &mut sd);
        let owned = created && got_user && rc == 0 && EqualSid(owner, user_sid) != 0;
        check!(owned, "le fichier créé appartient à l'utilisateur");
        let mut ace: *mut u8 = null_mut();
        let full = rc == 0
            && GetAce(dacl, 0, &mut ace) != 0
            && core::ptr::read_unaligned(ace.add(4) as *const u32) & 0x0012_0116 == 0x0012_0116;
        check!(full, "le propriétaire peut écrire (ACL tirée des droits Linux)");
        if !sd.is_null() {
            LocalFree(sd as *mut c_void);
        }
        DeleteFileW(name.as_ptr());
    }
    finish()
}
