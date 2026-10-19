#![no_std]
#![no_main]

use core::ffi::{c_char, c_void};
use core::ptr::null_mut;
use winlux_testexe::*;

extern "system" fn compare_i32(a: *const c_void, b: *const c_void) -> i32 {
    let x = unsafe { *(a as *const i32) };
    let y = unsafe { *(b as *const i32) };
    if x < y { -1 } else if x > y { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        let mut buf = [0 as c_char; 64];
        strcpy(buf.as_mut_ptr(), c"abc".as_ptr());
        strcat(buf.as_mut_ptr(), c"def".as_ptr());
        let joined = strcmp(buf.as_ptr(), c"abcdef".as_ptr()) == 0;
        let sized = strncmp(c"abcdef".as_ptr(), c"abcxyz".as_ptr(), 3) == 0;
        let found = *strchr(buf.as_ptr(), b'd' as i32) == b'd' as c_char;
        let sub = *strstr(buf.as_ptr(), c"cde".as_ptr()) == b'c' as c_char;
        let upper = strcmp(strupr(buf.as_mut_ptr()), c"ABCDEF".as_ptr()) == 0;
        let lower = strcmp(strlwr(buf.as_mut_ptr()), c"abcdef".as_ptr()) == 0;
        let ci = stricmp(c"AbC".as_ptr(), c"aBc".as_ptr()) == 0;
        check!(joined && sized && found && sub && upper && lower && ci, "chaînes strcpy/strcat/strchr/strstr/casse");

        let duplicated = strdup(c"dup".as_ptr());
        let dup_ok = strcmp(duplicated, c"dup".as_ptr()) == 0;
        free(duplicated as *mut c_void);
        let mut text = [0 as c_char; 32];
        strcpy(text.as_mut_ptr(), c"un,deux;;trois".as_ptr());
        let mut ctx: *mut c_char = null_mut();
        let mut count = 0;
        let mut tok = strtok_s(text.as_mut_ptr(), c",;".as_ptr(), &mut ctx);
        while !tok.is_null() {
            count += 1;
            tok = strtok_s(null_mut(), c",;".as_ptr(), &mut ctx);
        }
        check!(dup_ok && count == 3, "strdup et strtok_s");

        let mut end: *mut c_char = null_mut();
        let handled = strtol(c"0x1f".as_ptr(), &mut end, 0) == 31
            && strtol(c"777".as_ptr(), &mut end, 8) == 511
            && strtoul(c"ff".as_ptr(), &mut end, 16) == 255
            && strtoll(c"-900000000000".as_ptr(), &mut end, 10) == -900_000_000_000
            && atol(c"  42abc".as_ptr()) == 42
            && strtod(c"3.5e2 reste".as_ptr(), &mut end) > 349.9
            && strtod(c"3.5e2 reste".as_ptr(), &mut end) < 350.1
            && *end == b' ' as c_char;
        check!(handled, "strtol/strtoul/strtoll/strtod avec pointeur de fin");

        _itoa(-255, buf.as_mut_ptr(), 10);
        let itoa_dec = strcmp(buf.as_ptr(), c"-255".as_ptr()) == 0;
        _itoa(255, buf.as_mut_ptr(), 16);
        let itoa_hex = strcmp(buf.as_ptr(), c"ff".as_ptr()) == 0;
        _i64toa(4_000_000_000, buf.as_mut_ptr(), 10);
        let i64 = strcmp(buf.as_ptr(), c"4000000000".as_ptr()) == 0;
        check!(itoa_dec && itoa_hex && i64, "_itoa et _i64toa");

        let ctype = isalpha(b'A' as i32) != 0
            && isdigit(b'7' as i32) != 0
            && isspace(b' ' as i32) != 0
            && isspace(b'x' as i32) == 0
            && isxdigit(b'F' as i32) != 0
            && toupper(b'a' as i32) == b'A' as i32
            && tolower(b'Z' as i32) == b'z' as i32;
        check!(ctype, "classification et conversion de caractères");

        let math = (sqrt(9.0) - 3.0).abs() < 1e-9
            && (pow(2.0, 10.0) - 1024.0).abs() < 1e-9
            && (fabs(-2.5) - 2.5).abs() < 1e-12
            && (floor(2.9) - 2.0).abs() < 1e-12
            && abs(-7) == 7;
        check!(math, "mathématiques sqrt/pow/fabs/floor/abs");

        let mut data = [5i32, 1, 4, 2, 3];
        qsort(data.as_mut_ptr() as *mut c_void, data.len(), core::mem::size_of::<i32>(), compare_i32 as *const () as usize);
        let sorted = data == [1, 2, 3, 4, 5];
        let key = 4i32;
        let hit = bsearch(&key as *const i32 as *const c_void, data.as_ptr() as *const c_void, data.len(), core::mem::size_of::<i32>(), compare_i32 as *const () as usize);
        let found_key = !hit.is_null() && *(hit as *const i32) == 4;
        check!(sorted && found_key, "qsort et bsearch avec comparateur");

        let mut line = [0 as c_char; 32];
        let written = _snprintf(line.as_mut_ptr(), 32, c"%d-%s-%.2f".as_ptr(), 7, c"ok".as_ptr(), 1.5);
        let formatted = written == 9 && strcmp(line.as_ptr(), c"7-ok-1.50".as_ptr()) == 0;
        let mut small = [0 as c_char; 4];
        let truncated = _snprintf(small.as_mut_ptr(), 4, c"%s".as_ptr(), c"abcdef".as_ptr()) == -1
            && strcmp(small.as_ptr(), c"abc".as_ptr()) == 0;
        let needed = snprintf(null_mut(), 0, c"%d".as_ptr(), 1234) == 4;
        check!(formatted && truncated && needed, "_snprintf/snprintf avec troncature");

        let wfmt = [b'%' as u16, b's' as u16, b'-' as u16, b'%' as u16, b'd' as u16, 0];
        let warg = [b'o' as u16, b'k' as u16, 0];
        let wexpected = [b'o' as u16, b'k' as u16, b'-' as u16, b'7' as u16, 0];
        let mut wbuf = [0u16; 32];
        let wwritten = _snwprintf(wbuf.as_mut_ptr(), 32, wfmt.as_ptr(), warg.as_ptr(), 7);
        let wide_format = wwritten == 4 && wcscmp(wbuf.as_ptr(), wexpected.as_ptr()) == 0;
        _itow(-255, wbuf.as_mut_ptr(), 10);
        let wminus = [b'-' as u16, b'2' as u16, b'5' as u16, b'5' as u16, 0];
        let wide_itoa = wcscmp(wbuf.as_ptr(), wminus.as_ptr()) == 0;
        check!(wide_format && wide_itoa, "_snwprintf et _itow");

        let mut pid_text = [0 as c_char; 16];
        _itoa(GetCurrentProcessId() as i32, pid_text.as_mut_ptr(), 10);
        let mut stream_path = [0 as c_char; 160];
        sprintf(stream_path.as_mut_ptr(), c"Z:\\tmp\\winlux-stream-%s.txt".as_ptr(), pid_text.as_ptr());
        let stream = fopen(stream_path.as_ptr(), c"w+".as_ptr());
        let wrote = if stream.is_null() {
            false
        } else {
            fprintf(stream, c"%s=%d\n".as_ptr(), c"alpha".as_ptr(), 41) == 9
                && fprintf(stream, c"%s=%d\n".as_ptr(), c"beta".as_ptr(), 42) == 8
                && fflush(stream) == 0
        };
        let positioned = !stream.is_null() && ftell(stream) == 17 && fseek(stream, 0, 0) == 0;
        let mut line = [0 as c_char; 64];
        let line1 = !stream.is_null() && !fgets(line.as_mut_ptr(), 64, stream).is_null()
            && strcmp(line.as_ptr(), c"alpha=41\n".as_ptr()) == 0;
        let line2 = !stream.is_null() && !fgets(line.as_mut_ptr(), 64, stream).is_null()
            && strcmp(line.as_ptr(), c"beta=42\n".as_ptr()) == 0;
        let ended = !stream.is_null() && fgets(line.as_mut_ptr(), 64, stream).is_null() && feof(stream) != 0;
        let closed = !stream.is_null() && fclose(stream) == 0;
        let removed = remove(stream_path.as_ptr()) == 0;
        check!(wrote && positioned && line1 && line2 && ended && closed && removed, "fopen/fprintf/fgets/fseek/fclose");

        let mut num1 = 0i32;
        let mut num2 = 0i32;
        let mut real = 0f64;
        let parsed = sscanf(c"12 -7 3.5".as_ptr(), c"%d %i %lf".as_ptr(), &mut num1, &mut num2, &mut real) == 3
            && num1 == 12 && num2 == -7 && (real - 3.5).abs() < 1e-9;
        let mut word = [0 as c_char; 16];
        let mut value = 0i32;
        let sets = sscanf(c"alpha=41".as_ptr(), c"%[a-z]=%d".as_ptr(), word.as_mut_ptr(), &mut value) == 2
            && strcmp(word.as_ptr(), c"alpha".as_ptr()) == 0
            && value == 41;
        check!(parsed && sets, "sscanf avec entiers, flottants et jeux de caractères");

        let mut pid2 = [0 as c_char; 16];
        _itoa(GetCurrentProcessId() as i32, pid2.as_mut_ptr(), 10);
        let mut scan_path = [0 as c_char; 160];
        sprintf(scan_path.as_mut_ptr(), c"Z:\\tmp\\winlux-scan-%s.txt".as_ptr(), pid2.as_ptr());
        let sf = fopen(scan_path.as_ptr(), c"w+".as_ptr());
        let mut scan_ok = false;
        if !sf.is_null() {
            fprintf(sf, c"%d %s\n".as_ptr(), 99, c"ligne".as_ptr());
            fflush(sf);
            fseek(sf, 0, 0);
            let mut got_n = 0i32;
            let mut got_s = [0 as c_char; 16];
            scan_ok = fscanf(sf, c"%d %s".as_ptr(), &mut got_n, got_s.as_mut_ptr()) == 2
                && got_n == 99
                && strcmp(got_s.as_ptr(), c"ligne".as_ptr()) == 0;
            fclose(sf);
            remove(scan_path.as_ptr());
        }
        check!(scan_ok, "fscanf sur un flux FILE*");
    }
    finish()
}
