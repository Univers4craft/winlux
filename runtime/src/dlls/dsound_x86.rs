//! DirectSound pour les programmes 32 bits (#778).
//!
//! Comme Direct3D 9 (`d3d9_x86`) : un programme x86 reçoit des tables de
//! passerelles stdcall construites d'après la signature de chaque méthode.
//! Les cases de pointeur rendues (objets, zones de `Lock`) passent par les
//! cases de 8 octets de la passerelle ; DSBUFFERDESC, DSCBUFFERDESC et
//! DSBPOSITIONNOTIFY, qui portent un pointeur ou un handle, sont convertis.

use super::*;
use crate::dlls::legacy_com::{rd32, x86, x86_table};

const UNKNOWN: [&str; 3] = ["ip", "", ""];

fn with(rest: &[&'static str]) -> Vec<&'static str> {
    UNKNOWN.iter().chain(rest).copied().collect()
}

fn signatures(name: &str) -> Vec<&'static str> {
    match name {
        "IDirectSound8" => with(&["ipi", "i", "ip", "ii", "", "i", "i", "i", "i"]),
        "IDirectSoundBuffer8" => with(&[
            "i", "ii", "iii", "i", "i", "i", "i", "ii", "iipipii", "iii", "i", "i", "i", "i", "i",
            "", "iiii", "", "iii", "iii", "iiip",
        ]),
        "IDirectSoundNotify" | "IDirectSoundCaptureNotify" => with(&["ii"]),
        "IDirectSound3DBuffer" => with(&[
            "i", "ii", "i", "i", "i", "i", "i", "i", "i", "ii", "iii", "iiii", "ii", "ii", "ii",
            "ii", "iiii", "iiii",
        ]),
        "IDirectSound3DListener" => with(&[
            "i", "i", "i", "ii", "i", "i", "i", "ii", "ii", "ii", "iiiiiii", "iiii", "ii", "iiii",
            "",
        ]),
        "IDirectSoundCapture" => with(&["ipi", "i", "i"]),
        "IDirectSoundCaptureBuffer8" => with(&[
            "i", "ii", "iii", "i", "ii", "iipipii", "i", "", "iiii", "iiip", "ii",
        ]),
        _ => Vec::new(),
    }
}

/// DSBUFFERDESC x86 (20 ou 36 octets) → disposition 64 bits (40 octets).
unsafe fn buffer_desc(p: usize) -> [u64; 5] {
    let mut d = [0u64; 5];
    let b = d.as_mut_ptr() as *mut u8;
    let size = rd32(p);
    (b as *mut u32).write(40);
    std::ptr::copy_nonoverlapping((p + 4) as *const u8, b.add(4), 12);
    (b.add(16) as *mut u64).write(rd32(p + 16) as u64);
    if size >= 36 {
        std::ptr::copy_nonoverlapping((p + 20) as *const u8, b.add(24), 16);
    }
    d
}
/// DSCBUFFERDESC x86 (20 ou 28 octets) → disposition 64 bits (40 octets).
unsafe fn capture_desc(p: usize) -> [u64; 5] {
    let mut d = [0u64; 5];
    let b = d.as_mut_ptr() as *mut u8;
    let size = rd32(p);
    (b as *mut u32).write(if size >= 28 { 40 } else { 24 });
    std::ptr::copy_nonoverlapping((p + 4) as *const u8, b.add(4), 12);
    (b.add(16) as *mut u64).write(rd32(p + 16) as u64);
    if size >= 28 {
        (b.add(24) as *mut u32).write(rd32(p + 20));
        (b.add(32) as *mut u64).write(rd32(p + 24) as u64);
    }
    d
}
/// DSBPOSITIONNOTIFY x86 { DWORD dwOffset; HANDLE hEventNotify; } (8 octets)
/// → 16 octets.
unsafe fn notify_list(count: u32, list: usize) -> Vec<u64> {
    let mut v = Vec::with_capacity(count as usize * 2);
    for i in 0..count as usize {
        v.push(rd32(list + 8 * i) as u64);
        v.push(rd32(list + 8 * i + 4) as u64);
    }
    v
}

unsafe extern "win64" fn create_sound_buffer(
    this: *mut Com<Device>,
    desc: usize,
    out: *mut *mut c_void,
    outer: *mut c_void,
) -> i32 {
    if desc == 0 {
        return E_INVALIDARG;
    }
    let d = buffer_desc(desc);
    ds_create_sound_buffer(this, d.as_ptr() as *const u8, out, outer)
}
unsafe extern "win64" fn create_capture_buffer(
    this: *mut Com<Capture>,
    desc: usize,
    out: *mut *mut c_void,
    outer: *mut c_void,
) -> i32 {
    if desc == 0 {
        return E_INVALIDARG;
    }
    let d = capture_desc(desc);
    dsc_create_buffer(this, d.as_ptr() as *const u8, out, outer)
}
unsafe extern "win64" fn set_notify(this: *mut Iface, count: u32, list: usize) -> i32 {
    if count > 0 && list == 0 {
        return E_INVALIDARG;
    }
    let v = notify_list(count, list);
    notify_set(this, count, v.as_ptr() as *const u8)
}
unsafe extern "win64" fn set_capture_notify(this: *mut Iface, count: u32, list: usize) -> i32 {
    if count > 0 && list == 0 {
        return E_INVALIDARG;
    }
    let v = notify_list(count, list);
    capn_set(this, count, v.as_ptr() as *const u8)
}

fn override_for(name: &str, i: usize) -> Option<usize> {
    let f = |p: *const ()| Some(p as usize);
    match (name, i) {
        ("IDirectSound8", 3) => f(create_sound_buffer as *const ()),
        ("IDirectSoundCapture", 3) => f(create_capture_buffer as *const ()),
        ("IDirectSoundNotify", 3) => f(set_notify as *const ()),
        ("IDirectSoundCaptureNotify", 3) => f(set_capture_notify as *const ()),
        _ => None,
    }
}

/// Table à donner à un objet DirectSound selon l'ABI du programme.
pub(super) fn abi<const N: usize>(vt: &'static VTable<N>, name: &'static str) -> *const () {
    if !x86() {
        return vt.ptr();
    }
    let sig = signatures(name);
    let methods: Vec<(usize, usize, &str)> =
        vt.0.iter()
            .enumerate()
            .map(|(i, &f)| {
                (
                    override_for(name, i).unwrap_or(f as usize),
                    f as usize,
                    sig.get(i).copied().unwrap_or(""),
                )
            })
            .collect();
    x86_table(name, &methods) as *const ()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_completes() {
        assert_eq!(signatures("IDirectSound8").len(), DS_VT.0.len());
        assert_eq!(signatures("IDirectSoundBuffer8").len(), BUF_VT.0.len());
        assert_eq!(signatures("IDirectSoundNotify").len(), NOTIFY_VT.0.len());
        assert_eq!(signatures("IDirectSound3DBuffer").len(), B3D_VT.0.len());
        assert_eq!(
            signatures("IDirectSound3DListener").len(),
            LISTENER_VT.0.len()
        );
        assert_eq!(signatures("IDirectSoundCapture").len(), DSC_VT.0.len());
        assert_eq!(
            signatures("IDirectSoundCaptureBuffer8").len(),
            CAPBUF_VT.0.len()
        );
        // Lock : deux cases de pointeur rendues.
        assert_eq!(signatures("IDirectSoundBuffer8")[11], "iipipii");
    }

    #[test]
    fn descriptions_x86() {
        let mut x = [0u32; 9];
        x[0] = 36;
        x[1] = 0x80;
        x[2] = 4096;
        x[4] = 0x1234_5678;
        x[5] = 0xAABB_CCDD;
        unsafe {
            let d = buffer_desc(x.as_ptr() as usize);
            let b = d.as_ptr() as *const u32;
            assert_eq!((*b, *b.add(1), *b.add(2)), (40, 0x80, 4096));
            assert_eq!(*(b.add(4) as *const u64), 0x1234_5678);
            assert_eq!(*b.add(6), 0xAABB_CCDD);
            let n = notify_list(2, [10u32, 7, 20, 8].as_ptr() as usize);
            assert_eq!(n, vec![10, 7, 20, 8]);
        }
    }
}
