#![no_std]
#![no_main]

use core::ffi::c_char;
use core::ptr::null_mut;
use winlux_testexe::*;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let mut path = [0 as c_char; 128];
        sprintf(path.as_mut_ptr(), c"Z:\\tmp\\winlux-find-%u.txt".as_ptr(), GetCurrentProcessId());
        let file = CreateFileA(path.as_ptr(), 0xC000_0000, 7, null_mut(), 2, 0, null_mut());
        let payload = b"find-test";
        let mut written = 0;
        let created = file as usize != usize::MAX && WriteFile(file, payload.as_ptr(), payload.len() as u32, &mut written, null_mut()) != 0;
        let mut data: Win32FindDataA = core::mem::zeroed();
        let search = FindFirstFileA(path.as_ptr(), &mut data);
        let found = search as usize != usize::MAX && data.size_low == payload.len() as u32 && strlen(data.file_name.as_ptr()) != 0;
        let end = found && FindNextFileA(search, &mut data) == 0 && GetLastError() == 2;
        let closed_search = if search as usize != usize::MAX { FindClose(search) != 0 } else { false };
        let mut wide_path = [0u16; 128];
        let mut path_len = 0usize;
        while path_len < path.len() && path[path_len] != 0 { wide_path[path_len] = path[path_len] as u8 as u16; path_len += 1; }
        let mut wide_data: Win32FindDataW = core::mem::zeroed();
        let wide_search = FindFirstFileW(wide_path.as_ptr(), &mut wide_data);
        let wide_found = wide_search as usize != usize::MAX && wide_data.size_low == payload.len() as u32 && wide_data.file_name[0] != 0;
        let wide_end = wide_found && FindNextFileW(wide_search, &mut wide_data) == 0;
        let wide_closed = if wide_search as usize != usize::MAX { FindClose(wide_search) != 0 } else { false };
        let closed_file = if file as usize != usize::MAX { CloseHandle(file) != 0 } else { false };
        let deleted = DeleteFileA(path.as_ptr()) != 0;
        check!(created && written == payload.len() as u32, "création du fichier de recherche");
        check!(found, "FindFirstFileA nom et taille");
        check!(end, "FindNextFileA fin d'énumération");
        check!(wide_found && wide_end && wide_closed, "FindFirst/NextFileW");
        check!(closed_search && closed_file && deleted, "FindClose et nettoyage");
    }
    finish()
}
