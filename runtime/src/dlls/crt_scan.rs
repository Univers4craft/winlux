//! Entrées formatées de la CRT : `scanf`, `sscanf`, `fscanf` et leurs
//! variantes `v*`. Le moteur lit le format étape par étape et écrit les
//! champs convertis via les pointeurs variadiques.

use std::ffi::c_void;

#[derive(Clone, Copy, PartialEq)]
enum Size {
    None,
    H,
    HH,
    L,
    LL,
    I64,
}

fn size_bytes(s: Size) -> usize {
    match s {
        Size::HH => 1,
        Size::H => 2,
        Size::None | Size::L => 4,
        Size::LL | Size::I64 => 8,
    }
}

struct Scan<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Scan<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.data.len() && self.data[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn read_int(&mut self, base: i32) -> Option<(u64, bool)> {
        self.skip_ws();
        let start = self.pos;
        let mut neg = false;
        if let Some(&c) = self.data.get(self.pos) {
            if c == b'+' {
                self.pos += 1;
            } else if c == b'-' {
                neg = true;
                self.pos += 1;
            }
        }
        let mut radix = if base == 0 { 10 } else { base as u32 };
        if (radix == 16 || base == 0)
            && self.data.get(self.pos) == Some(&b'0')
            && matches!(self.data.get(self.pos + 1), Some(&b'x') | Some(&b'X'))
        {
            radix = 16;
            self.pos += 2;
        } else if base == 0 && self.data.get(self.pos) == Some(&b'0') {
            radix = 8;
        }
        let start_digits = self.pos;
        let mut value: u64 = 0;
        while let Some(&c) = self.data.get(self.pos) {
            let d = match c {
                b'0'..=b'9' => (c - b'0') as u32,
                b'a'..=b'z' => (c - b'a') as u32 + 10,
                b'A'..=b'Z' => (c - b'A') as u32 + 10,
                _ => break,
            };
            if d >= radix {
                break;
            }
            value = value.wrapping_mul(radix as u64).wrapping_add(d as u64);
            self.pos += 1;
        }
        let _ = radix;
        if self.pos == start_digits {
            self.pos = start;
            None
        } else {
            Some((value, neg))
        }
    }

    fn read_float(&mut self) -> Option<f64> {
        self.skip_ws();
        let start = self.pos;
        if matches!(self.data.get(self.pos), Some(&b'+') | Some(&b'-')) {
            self.pos += 1;
        }
        let mut seen = false;
        while matches!(self.data.get(self.pos), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
            seen = true;
        }
        if self.data.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            while matches!(self.data.get(self.pos), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
                seen = true;
            }
        }
        if seen && matches!(self.data.get(self.pos), Some(&b'e') | Some(&b'E')) {
            let mut q = self.pos + 1;
            if matches!(self.data.get(q), Some(&b'+') | Some(&b'-')) {
                q += 1;
            }
            if matches!(self.data.get(q), Some(c) if c.is_ascii_digit()) {
                self.pos = q;
                while matches!(self.data.get(self.pos), Some(c) if c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
        }
        if !seen {
            self.pos = start;
            return None;
        }
        let text = std::str::from_utf8(&self.data[start..self.pos]).ok()?;
        text.parse().ok()
    }

    fn skip_ws_here(&mut self) {
        while matches!(self.data.get(self.pos), Some(c) if c.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }
}

struct Spec {
    suppress: bool,
    width: usize,
    size: Size,
    conv: u8,
}

unsafe fn write_raw(ptr: *mut u8, raw: u64, nbytes: usize) {
    if ptr.is_null() {
        return;
    }
    for i in 0..nbytes {
        *ptr.add(i) = (raw >> (8 * i)) as u8;
    }
}

unsafe fn store_int(va: &mut crate::thunks::VaList, s: &Spec, value: u64, neg: bool) -> bool {
    let ptr = if s.suppress {
        std::ptr::null_mut()
    } else {
        va.next_u64() as *mut u8
    };
    let signed = matches!(s.conv, b'd' | b'i');
    let raw = if signed && neg {
        (value as i64).wrapping_neg() as u64
    } else {
        value
    };
    write_raw(ptr, raw, size_bytes(s.size));
    !s.suppress
}

unsafe fn store_float(va: &mut crate::thunks::VaList, s: &Spec, value: f64) -> bool {
    let ptr = if s.suppress {
        std::ptr::null_mut()
    } else {
        va.next_u64() as *mut u8
    };
    if matches!(s.size, Size::L) {
        write_raw(ptr, value.to_bits(), 8);
    } else {
        write_raw(ptr, (value as f32).to_bits() as u64, 4);
    }
    !s.suppress
}

/// Écrit une suite de caractères, étroite ou large selon le modificateur.
unsafe fn store_string(va: &mut crate::thunks::VaList, s: &Spec, chars: &[u8]) -> bool {
    if s.suppress {
        return false;
    }
    let ptr = va.next_u64() as *mut u8;
    if ptr.is_null() {
        return false;
    }
    if s.size == Size::L {
        let wp = ptr as *mut u16;
        for (i, &c) in chars.iter().enumerate() {
            *wp.add(i) = c as u16;
        }
        *wp.add(chars.len()) = 0;
    } else {
        std::ptr::copy_nonoverlapping(chars.as_ptr(), ptr, chars.len());
        *ptr.add(chars.len()) = 0;
    }
    true
}

fn match_set(c: u8, set: &[u8]) -> bool {
    let mut i = 0;
    let negate = !set.is_empty() && (set[0] == b'^');
    if negate {
        i = 1;
    }
    let mut found = false;
    while i < set.len() {
        if i + 2 < set.len() && set[i + 1] == b'-' {
            if c >= set[i] && c <= set[i + 2] {
                found = true;
            }
            i += 3;
        } else {
            if c == set[i] {
                found = true;
            }
            i += 1;
        }
    }
    found != negate
}

/// Moteur commun : renvoie le nombre de champs affectés.
unsafe fn scan(bytes: &[u8], fmt: &[u8], va: &mut crate::thunks::VaList) -> i32 {
    let mut inp = Scan {
        data: bytes,
        pos: 0,
    };
    let mut f = 0;
    let mut assigned = 0;
    while f < fmt.len() {
        let c = fmt[f];
        if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
            inp.skip_ws_here();
            f += 1;
            continue;
        }
        if c != b'%' {
            if inp.data.get(inp.pos) == Some(&c) {
                inp.pos += 1;
                f += 1;
                continue;
            }
            break;
        }
        f += 1;
        if fmt.get(f) == Some(&b'%') {
            if inp.data.get(inp.pos) == Some(&b'%') {
                inp.pos += 1;
                f += 1;
                continue;
            }
            break;
        }
        let mut s = Spec {
            suppress: false,
            width: usize::MAX,
            size: Size::None,
            conv: 0,
        };
        if fmt.get(f) == Some(&b'*') {
            s.suppress = true;
            f += 1;
        }
        let wstart = f;
        while matches!(fmt.get(f), Some(c) if c.is_ascii_digit()) {
            f += 1;
        }
        if f > wstart {
            let w = std::str::from_utf8(&fmt[wstart..f]).unwrap_or("");
            s.width = w.parse().unwrap_or(usize::MAX);
        }
        while let Some(&m) = fmt.get(f) {
            match m {
                b'h' => {
                    s.size = if s.size == Size::H { Size::HH } else { Size::H };
                    f += 1;
                }
                b'l' => {
                    s.size = if s.size == Size::L { Size::LL } else { Size::L };
                    f += 1;
                }
                b'I' if fmt.get(f + 1) == Some(&b'6') && fmt.get(f + 2) == Some(&b'4') => {
                    s.size = Size::I64;
                    f += 3;
                }
                b'j' | b'z' | b't' | b'L' => {
                    s.size = Size::I64;
                    f += 1;
                }
                _ => break,
            }
        }
        let conv = match fmt.get(f) {
            Some(&c) => c,
            None => break,
        };
        s.conv = conv;
        f += 1;

        let ok = match conv {
            b'd' | b'i' => match inp.read_int(if conv == b'i' { 0 } else { 10 }) {
                Some((v, n)) => store_int(va, &s, v, n),
                None => break,
            },
            b'u' | b'o' | b'x' | b'X' => {
                let base = match conv {
                    b'o' => 8,
                    b'u' => 10,
                    _ => 16,
                };
                match inp.read_int(base) {
                    Some((v, _)) => store_int(va, &s, v, false),
                    None => break,
                }
            }
            b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'a' | b'A' => match inp.read_float() {
                Some(v) => store_float(va, &s, v),
                None => break,
            },
            b'c' => {
                let want = if s.width == usize::MAX { 1 } else { s.width };
                let mut chars = Vec::new();
                while chars.len() < want {
                    match inp.data.get(inp.pos) {
                        Some(&ch) => {
                            chars.push(ch);
                            inp.pos += 1;
                        }
                        None => break,
                    }
                }
                if chars.is_empty() {
                    break;
                }
                store_string(va, &s, &chars)
            }
            b's' => {
                let mut chars = Vec::new();
                inp.skip_ws_here();
                while chars.len() < s.width {
                    match inp.data.get(inp.pos) {
                        Some(&ch) if !ch.is_ascii_whitespace() => {
                            chars.push(ch);
                            inp.pos += 1;
                        }
                        _ => break,
                    }
                }
                if chars.is_empty() {
                    break;
                }
                store_string(va, &s, &chars)
            }
            b'[' => {
                let set_start = f;
                if fmt.get(f) == Some(&b'^') {
                    f += 1;
                }
                if fmt.get(f) == Some(&b']') {
                    f += 1;
                }
                while let Some(&m) = fmt.get(f) {
                    if m == b']' {
                        break;
                    }
                    f += 1;
                }
                let set = &fmt[set_start..f];
                f += 1;
                let mut chars = Vec::new();
                while chars.len() < s.width {
                    match inp.data.get(inp.pos) {
                        Some(&ch) if match_set(ch, set) => {
                            chars.push(ch);
                            inp.pos += 1;
                        }
                        _ => break,
                    }
                }
                if chars.is_empty() {
                    break;
                }
                store_string(va, &s, &chars)
            }
            b'n' => {
                if !s.suppress {
                    let ptr = va.next_u64() as *mut u8;
                    write_raw(
                        ptr,
                        inp.pos as u64,
                        if s.size == Size::I64 || s.size == Size::LL {
                            8
                        } else {
                            4
                        },
                    );
                }
                continue;
            }
            _ => break,
        };
        if ok {
            assigned += 1;
        }
    }
    assigned
}

unsafe fn nul_terminated(p: *const u8) -> &'static [u8] {
    if p.is_null() {
        return &[];
    }
    std::slice::from_raw_parts(p, crate::printf::narrow_len(p))
}

unsafe extern "win64" fn imp_sscanf(args: *const u64) -> i32 {
    let mut v = crate::thunks::VaList::new(args);
    let buf = v.next_u64() as *const u8;
    let fmt = v.next_ptr::<u8>();
    scan(nul_terminated(buf), nul_terminated(fmt), &mut v)
}
unsafe extern "win64" fn imp_scanf(args: *const u64) -> i32 {
    let mut v = crate::thunks::VaList::new(args);
    let fmt = v.next_ptr::<u8>();
    let data = read_all_stdin();
    scan(&data, nul_terminated(fmt), &mut v)
}
unsafe extern "win64" fn imp_fscanf(args: *const u64) -> i32 {
    let mut v = crate::thunks::VaList::new(args);
    let stream = v.next_u64() as *mut c_void;
    let fmt = v.next_ptr::<u8>();
    let data = crate::dlls::crt_stdio::read_to_end(stream);
    scan(&data, nul_terminated(fmt), &mut v)
}
unsafe extern "win64" fn imp_vsscanf(args: *const u64) -> i32 {
    let buf = *args as *const u8;
    let fmt = *args.add(1) as *const u8;
    let va = *args.add(2) as *const u64;
    let mut v = crate::thunks::VaList::new(va);
    scan(nul_terminated(buf), nul_terminated(fmt), &mut v)
}
unsafe extern "win64" fn imp_vfscanf(args: *const u64) -> i32 {
    let stream = *args as *mut c_void;
    let fmt = *args.add(1) as *const u8;
    let va = *args.add(2) as *const u64;
    let data = crate::dlls::crt_stdio::read_to_end(stream);
    let mut v = crate::thunks::VaList::new(va);
    scan(&data, nul_terminated(fmt), &mut v)
}

fn read_all_stdin() -> Vec<u8> {
    use std::io::Read;
    let mut data = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut data);
    data
}

crate::varargs_thunk!(sscanf_thunk => imp_sscanf);
crate::varargs_thunk!(scanf_thunk => imp_scanf);
crate::varargs_thunk!(fscanf_thunk => imp_fscanf);
crate::varargs_thunk!(vsscanf_thunk => imp_vsscanf);
crate::varargs_thunk!(vfscanf_thunk => imp_vfscanf);

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "sscanf" => fn sscanf_thunk, "scanf" => fn scanf_thunk, "fscanf" => fn fscanf_thunk,
        "vsscanf" => fn vsscanf_thunk, "vfscanf" => fn vfscanf_thunk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyse_entiers_et_flottants() {
        unsafe {
            let mut out_i = [0i32; 2];
            let args = [
                out_i.as_mut_ptr() as u64,
                (out_i.as_mut_ptr().add(1)) as u64,
            ];
            let n = scan(
                b"12 -7",
                b"%d %i",
                &mut crate::thunks::VaList::new(args.as_ptr()),
            );
            assert_eq!(n, 2);
            assert_eq!(out_i, [12, -7]);
            let mut f = 0f64;
            let fargs = [&mut f as *mut f64 as u64];
            let n = scan(
                b"3.5e2",
                b"%lf",
                &mut crate::thunks::VaList::new(fargs.as_ptr()),
            );
            assert_eq!(n, 1);
            assert!((f - 350.0).abs() < 1e-9);
            let mut hex = 0u32;
            let hargs = [&mut hex as *mut u32 as u64];
            let n = scan(
                b"0x1f",
                b"%x",
                &mut crate::thunks::VaList::new(hargs.as_ptr()),
            );
            assert_eq!(n, 1);
            assert_eq!(hex, 0x1f);
        }
    }

    #[test]
    fn analyse_chaines_jeux_et_largeur() {
        unsafe {
            let mut word = [0u8; 16];
            let mut set = [0u8; 16];
            let args = [word.as_mut_ptr() as u64, set.as_mut_ptr() as u64];
            let n = scan(
                b"abc,def",
                b"%s %[a-z]",
                &mut crate::thunks::VaList::new(args.as_ptr()),
            );
            assert_eq!(n, 1);
            assert_eq!(
                std::ffi::CStr::from_ptr(word.as_ptr() as *const i8).to_bytes(),
                b"abc,def"
            );
            let mut two = [0u8; 4];
            let args = [two.as_mut_ptr() as u64];
            let n = scan(
                b"abcde",
                b"%2s",
                &mut crate::thunks::VaList::new(args.as_ptr()),
            );
            assert_eq!(n, 1);
            assert_eq!(
                std::ffi::CStr::from_ptr(two.as_ptr() as *const i8).to_bytes(),
                b"ab"
            );
            let mut digits = [0u8; 8];
            let args = [digits.as_mut_ptr() as u64];
            let n = scan(
                b"12ab",
                b"%[0-9]",
                &mut crate::thunks::VaList::new(args.as_ptr()),
            );
            assert_eq!(n, 1);
            assert_eq!(
                std::ffi::CStr::from_ptr(digits.as_ptr() as *const i8).to_bytes(),
                b"12"
            );
        }
    }

    #[test]
    fn assignation_supprimee_et_compteur() {
        unsafe {
            let mut count = 0i32;
            let args = [&mut count as *mut i32 as u64];
            let n = scan(
                b"42",
                b"%*d%n",
                &mut crate::thunks::VaList::new(args.as_ptr()),
            );
            assert_eq!(n, 0);
            assert_eq!(count, 2);
        }
    }
}
