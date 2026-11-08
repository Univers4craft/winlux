//! #26 : cabinet.dll (décompression FDI) et lz32.dll (fichiers SZDD).
//! FDICopy extrait un cabinet par les rappels de l'application ;
//! LZOpenFile/LZRead/LZCopy développent un fichier compressé par COMPRESS.

#![no_std]
#![no_main]

use core::ffi::{c_char, c_void};
use winlux_testexe::*;

#[repr(C)]
struct Erf {
    oper: i32,
    kind: i32,
    error: i32,
}

#[repr(C)]
struct Notification {
    cb: i32,
    psz1: *const u8,
    psz2: *const u8,
    psz3: *const u8,
    pv: *mut c_void,
    hf: isize,
    date: u16,
    time: u16,
    attribs: u16,
    set_id: u16,
    i_cabinet: u16,
    i_folder: u16,
    fdie: i32,
}

#[repr(C)]
struct CabinetInfo {
    cb: i32,
    folders: u16,
    files: u16,
    set_id: u16,
    i_cabinet: u16,
    reserve: i32,
    has_prev: i32,
    has_next: i32,
}

#[link(name = "cabinet", kind = "raw-dylib")]
extern "C" {
    fn FDICreate(
        alloc: usize,
        free: usize,
        open: usize,
        read: usize,
        write: usize,
        close: usize,
        seek: usize,
        cpu: i32,
        erf: *mut Erf,
    ) -> usize;
    fn FDIIsCabinet(h: usize, hf: isize, info: *mut CabinetInfo) -> i32;
    fn FDICopy(
        h: usize,
        cabinet: *const c_char,
        path: *const c_char,
        flags: i32,
        notify: usize,
        decrypt: usize,
        pv: *mut c_void,
    ) -> i32;
    fn FDIDestroy(h: usize) -> i32;
}

#[link(name = "lz32", kind = "raw-dylib")]
extern "system" {
    fn LZOpenFileA(name: *const c_char, ofs: *mut u8, style: u16) -> i32;
    fn LZRead(h: i32, buf: *mut u8, n: i32) -> i32;
    fn LZSeek(h: i32, off: i32, origin: i32) -> i32;
    fn LZCopy(src: i32, dst: i32) -> i32;
    fn LZClose(h: i32);
    fn GetExpandedNameA(src: *const c_char, dst: *mut c_char) -> i32;
}

static CAB: [u8; 105] = [
    77, 83, 67, 70, 0, 0, 0, 0, 105, 0, 0, 0, 0, 0, 0, 0, 44, 0, 0, 0, 0, 0, 0, 0, 3, 1, 1, 0, 1,
    0, 0, 0, 52, 18, 0, 0, 70, 0, 0, 0, 1, 0, 0, 0, 27, 0, 0, 0, 0, 0, 0, 0, 0, 0, 104, 89, 0, 96,
    32, 0, 108, 105, 115, 101, 122, 46, 116, 120, 116, 0, 0, 0, 0, 0, 27, 0, 27, 0, 66, 111, 110,
    106, 111, 117, 114, 32, 100, 101, 112, 117, 105, 115, 32, 117, 110, 32, 99, 97, 98, 105, 110,
    101, 116, 13, 10,
];

/// « ABCABCABC » compressé : trois littéraux puis une référence.
static SZDD: [u8; 20] = [
    b'S',
    b'Z',
    b'D',
    b'D',
    0x88,
    0xf0,
    0x27,
    0x33,
    b'A',
    b't',
    9,
    0,
    0,
    0,
    0b0000_0111,
    b'A',
    b'B',
    b'C',
    0xF0,
    0xF3,
];

static mut CLOSED: u32 = 0;
static mut DATE: u16 = 0;

extern "C" fn mem_alloc(_n: u32) -> *mut c_void {
    core::ptr::null_mut()
}
extern "C" fn mem_free(_p: *mut c_void) {}
extern "C" fn file_open(path: *const c_char, _flag: i32, _mode: i32) -> isize {
    let h = unsafe {
        CreateFileA(
            path,
            0x8000_0000,
            1,
            core::ptr::null_mut(),
            3,
            0x80,
            core::ptr::null_mut(),
        )
    };
    h as isize
}
extern "C" fn file_read(hf: isize, buf: *mut c_void, n: u32) -> u32 {
    let mut got = 0u32;
    unsafe { ReadFile(hf as Handle, buf.cast(), n, &mut got, core::ptr::null_mut()) };
    got
}
extern "C" fn file_write(hf: isize, buf: *const c_void, n: u32) -> u32 {
    let mut done = 0u32;
    unsafe {
        WriteFile(
            hf as Handle,
            buf.cast(),
            n,
            &mut done,
            core::ptr::null_mut(),
        )
    };
    done
}
extern "C" fn file_close(hf: isize) -> i32 {
    unsafe { CloseHandle(hf as Handle) };
    0
}
extern "C" fn file_seek(hf: isize, off: i32, origin: i32) -> i32 {
    unsafe { SetFilePointer(hf as Handle, off, core::ptr::null_mut(), origin as u32) as i32 }
}
extern "C" fn notify(kind: i32, n: *mut Notification) -> isize {
    unsafe {
        match kind {
            2 => {
                // fdintCOPY_FILE : un seul fichier, « lisez.txt ».
                if !cstr_eq((*n).psz1.cast(), b"lisez.txt") || (*n).cb != 27 {
                    return -1;
                }
                DATE = (*n).date;
                CreateFileA(
                    c"cab_sortie.txt".as_ptr(),
                    0x4000_0000,
                    0,
                    core::ptr::null_mut(),
                    2,
                    0x80,
                    core::ptr::null_mut(),
                ) as isize
            }
            3 => {
                CloseHandle((*n).hf as Handle);
                CLOSED += 1;
                1
            }
            _ => 0,
        }
    }
}

unsafe fn write_file(name: &core::ffi::CStr, data: &[u8]) {
    let h = CreateFileA(
        name.as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0x80,
        core::ptr::null_mut(),
    );
    let mut n = 0u32;
    WriteFile(
        h,
        data.as_ptr(),
        data.len() as u32,
        &mut n,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
}

unsafe fn read_file(name: &core::ffi::CStr, buf: &mut [u8]) -> usize {
    let h = CreateFileA(
        name.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0x80,
        core::ptr::null_mut(),
    );
    let mut n = 0u32;
    ReadFile(
        h,
        buf.as_mut_ptr(),
        buf.len() as u32,
        &mut n,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    n as usize
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        // --- cabinet.dll ---
        write_file(c"test.cab", &CAB);
        let mut erf = Erf {
            oper: 0,
            kind: 0,
            error: 0,
        };
        let fdi = FDICreate(
            mem_alloc as usize,
            mem_free as usize,
            file_open as usize,
            file_read as usize,
            file_write as usize,
            file_close as usize,
            file_seek as usize,
            1,
            &mut erf,
        );
        check!(fdi != 0, "FDICreate");
        let hf = file_open(c"test.cab".as_ptr(), 0, 0);
        let mut info: CabinetInfo = core::mem::zeroed();
        check!(
            FDIIsCabinet(fdi, hf, &mut info) != 0
                && info.files == 1
                && info.set_id == 0x1234
                && info.cb == 105,
            "FDIIsCabinet lit l'en-tête"
        );
        file_close(hf);
        let copied = FDICopy(
            fdi,
            c"test.cab".as_ptr(),
            c"".as_ptr(),
            0,
            notify as usize,
            0,
            core::ptr::null_mut(),
        );
        check!(
            copied != 0 && CLOSED == 1 && DATE == 0x5968,
            "FDICopy extrait le fichier par les rappels"
        );
        let mut out = [0u8; 64];
        let n = read_file(c"cab_sortie.txt", &mut out);
        check!(
            &out[..n] == b"Bonjour depuis un cabinet\r\n",
            "contenu extrait identique"
        );
        let missing = FDICopy(
            fdi,
            c"absent.cab".as_ptr(),
            c"".as_ptr(),
            0,
            notify as usize,
            0,
            core::ptr::null_mut(),
        );
        check!(
            missing == 0 && erf.oper == 1 && erf.error != 0,
            "cabinet absent : FDIERROR_CABINET_NOT_FOUND"
        );
        check!(FDIDestroy(fdi) != 0, "FDIDestroy");

        // --- lz32.dll ---
        write_file(c"lz_test.tx_", &SZDD);
        let mut ofs = [0u8; 136];
        let h = LZOpenFileA(c"lz_test.tx_".as_ptr(), ofs.as_mut_ptr(), 0);
        check!(h >= 0x400, "LZOpenFileA rend un handle LZ");
        let mut buf = [0u8; 16];
        check!(
            LZRead(h, buf.as_mut_ptr(), 16) == 9 && &buf[..9] == b"ABCABCABC",
            "LZRead développe le fichier SZDD"
        );
        check!(
            LZSeek(h, 3, 0) == 3 && LZRead(h, buf.as_mut_ptr(), 3) == 3 && &buf[..3] == b"ABC",
            "LZSeek"
        );
        LZClose(h);
        let mut name = [0 as c_char; 64];
        check!(
            GetExpandedNameA(c"lz_test.tx_".as_ptr(), name.as_mut_ptr()) == 1
                && cstr_eq(name.as_ptr(), b"lz_test.txt"),
            "GetExpandedNameA rend la lettre manquante"
        );
        let src = LZOpenFileA(c"lz_test.tx_".as_ptr(), ofs.as_mut_ptr(), 0);
        let dst = LZOpenFileA(c"lz_test.txt".as_ptr(), ofs.as_mut_ptr(), 0x1000);
        check!(LZCopy(src, dst) == 9, "LZCopy écrit 9 octets");
        LZClose(src);
        LZClose(dst);
        let n = read_file(c"lz_test.txt", &mut out);
        check!(&out[..n] == b"ABCABCABC", "fichier développé par LZCopy");
        DeleteFileA(c"test.cab".as_ptr());
        DeleteFileA(c"cab_sortie.txt".as_ptr());
        DeleteFileA(c"lz_test.tx_".as_ptr());
        DeleteFileA(c"lz_test.txt".as_ptr());
    }
    finish()
}
