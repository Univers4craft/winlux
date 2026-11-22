//! setupapi.dll : `SetupIterateCabinetW` (#2713).
//!
//! D'après Microsoft Learn (SetupIterateCabinetW, PSP_FILE_CALLBACK_W,
//! « Creating a Cabinet Callback Routine », SPFILENOTIFY_FILEINCABINET,
//! SPFILENOTIFY_FILEEXTRACTED) : la fonction parcourt chaque fichier du
//! cabinet et envoie SPFILENOTIFY_FILEINCABINET au rappel (Param1 =
//! FILE_IN_CABINET_INFO, Param2 = nom du cabinet). FILEOP_DOIT extrait le
//! fichier vers FullTargetName puis envoie SPFILENOTIFY_FILEEXTRACTED
//! (Param1 = FILEPATHS : Source = cabinet, Target = fichier extrait) ;
//! FILEOP_SKIP passe au suivant ; FILEOP_ABORT arrête. Un code d'erreur
//! rendu par le rappel après FILEEXTRACTED arrête le parcours : la fonction
//! rend FALSE et GetLastError donne ce code.
//!
//! Constantes et dispositions d'après windows-sys 0.61.2 (MIT/Apache-2.0) :
//! SPFILENOTIFY_FILEINCABINET 0x11, SPFILENOTIFY_FILEEXTRACTED 0x13,
//! FILEOP_ABORT 0, FILEOP_DOIT 1, FILEOP_SKIP 2. FILE_IN_CABINET_INFO_W et
//! FILEPATHS_W sont `packed(1)` en x86 (FullTargetName à +18, FILEPATHS
//! 16 octets) et alignés naturellement en x64 (FullTargetName à +22,
//! FILEPATHS 24 octets).
//!
//! SPFILENOTIFY_NEEDNEWCABINET n'est jamais envoyé : `Cabinet::parse`
//! refuse les cabinets répartis sur plusieurs disques.

#![allow(clippy::missing_safety_doc, non_snake_case)]

use crate::msi::cab::Cabinet;
use crate::process::set_last_error;

const SPFILENOTIFY_FILEINCABINET: u32 = 0x11;
const SPFILENOTIFY_FILEEXTRACTED: u32 = 0x13;
const FILEOP_ABORT: u32 = 0;
const FILEOP_DOIT: u32 = 1;
const FILEOP_SKIP: u32 = 2;

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INVALID_DATA: u32 = 13;
const ERROR_WRITE_FAULT: u32 = 29;
/// Inférence, non documentée par Learn : code laissé quand le rappel rend
/// FILEOP_ABORT.
const ERROR_OPERATION_ABORTED: u32 = 995;

const MAX_PATH: usize = 260;

type PspFileCallbackW = unsafe extern "win64" fn(usize, u32, usize, usize) -> u32;

/// Décalages de FILE_IN_CABINET_INFO_W et FILEPATHS_W selon l'appelant.
#[derive(Clone, Copy)]
struct Layout {
    ps: usize,
    file_size: usize,
    win32_error: usize,
    dos_date: usize,
    full_target: usize,
    filepaths_error: usize,
}

fn layout(ps: usize) -> Layout {
    if ps == 4 {
        Layout {
            ps,
            file_size: 4,
            win32_error: 8,
            dos_date: 12,
            full_target: 18,
            filepaths_error: 8,
        }
    } else {
        Layout {
            ps,
            file_size: 8,
            win32_error: 12,
            dos_date: 16,
            full_target: 22,
            filepaths_error: 16,
        }
    }
}

fn put(b: &mut [u8], at: usize, v: &[u8]) {
    b[at..at + v.len()].copy_from_slice(v);
}

fn put_ptr(b: &mut [u8], at: usize, ps: usize, v: usize) {
    if ps == 4 {
        put(b, at, &(v as u32).to_le_bytes());
    } else {
        put(b, at, &(v as u64).to_le_bytes());
    }
}

/// Zone de travail passée au rappel : sur la pile du fil, donc sous 4 Go
/// pour un programme x86 (comme FDICopy, cabinet_lz.rs).
#[repr(C, align(8))]
struct Scratch {
    /// FILE_IN_CABINET_INFO_W (538 octets en x86, 544 en x64).
    info: [u8; 544],
    /// FILEPATHS_W (16 ou 24 octets).
    paths: [u8; 24],
    /// NameInCabinet, terminé par un zéro.
    name: [u16; 1024],
}

unsafe fn notify_file_in_cabinet(
    handler: PspFileCallbackW,
    context: usize,
    cabinet: *const u16,
    l: Layout,
    s: &mut Scratch,
    name: &str,
    size: usize,
    (date, time, attribs): (u16, u16, u16),
) -> u32 {
    s.name = [0; 1024];
    for (d, c) in s.name.iter_mut().zip(name.encode_utf16().take(1023)) {
        *d = c;
    }
    s.info = [0; 544];
    put_ptr(&mut s.info, 0, l.ps, s.name.as_ptr() as usize);
    put(&mut s.info, l.file_size, &(size as u32).to_le_bytes());
    put(&mut s.info, l.dos_date, &date.to_le_bytes());
    put(&mut s.info, l.dos_date + 2, &time.to_le_bytes());
    put(&mut s.info, l.dos_date + 4, &attribs.to_le_bytes());
    handler(
        context,
        SPFILENOTIFY_FILEINCABINET,
        s.info.as_mut_ptr() as usize,
        cabinet as usize,
    )
}

/// FullTargetName écrit par le rappel.
fn full_target(l: Layout, s: &Scratch) -> String {
    let raw = &s.info[l.full_target..l.full_target + 2 * MAX_PATH];
    let wide: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&wide)
}

pub unsafe extern "win64" fn SetupIterateCabinetW(
    cabinet: *const u16,
    _reserved: u32,
    handler: Option<PspFileCallbackW>,
    context: usize,
) -> i32 {
    let Some(handler) = handler else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if cabinet.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let path = crate::dlls::wstr(cabinet);
    let data = match std::fs::read(crate::paths::to_linux(&path)) {
        Ok(d) => d,
        Err(e) => {
            crate::trace!("SetupIterateCabinetW : « {path} » illisible : {e}");
            set_last_error(ERROR_FILE_NOT_FOUND);
            return 0;
        }
    };
    let cab = match Cabinet::parse(&data) {
        Ok(c) => c,
        Err(e) => {
            crate::trace!("SetupIterateCabinetW : « {path} » : {e}");
            set_last_error(ERROR_INVALID_DATA);
            return 0;
        }
    };
    iterate(
        &cab,
        cabinet,
        handler,
        context,
        crate::wow32::pointer_size(),
    )
}

/// Parcours du cabinet ; `ps` donne la disposition des structures.
unsafe fn iterate(
    cab: &Cabinet,
    cabinet: *const u16,
    handler: PspFileCallbackW,
    context: usize,
    ps: usize,
) -> i32 {
    let l = layout(ps);
    let mut s = Scratch {
        info: [0; 544],
        paths: [0; 24],
        name: [0; 1024],
    };
    let meta: Vec<(String, (u16, u16, u16), usize)> = cab
        .files
        .iter()
        .map(|f| (f.name.clone(), (f.date, f.time, f.attribs), f.size))
        .collect();
    let mut next = 0usize;
    let mut failure = 0u32;
    let result = cab.extract(&mut |name, bytes| {
        // Les fichiers sont rendus dans l'ordre de la table, dossier par dossier.
        let dos = meta
            .iter()
            .skip(next)
            .position(|(n, _, _)| n == name)
            .map(|i| {
                next += i + 1;
                meta[next - 1].1
            })
            .unwrap_or_default();
        match notify_file_in_cabinet(handler, context, cabinet, l, &mut s, name, bytes.len(), dos) {
            FILEOP_DOIT => {}
            FILEOP_SKIP => return Ok(()),
            FILEOP_ABORT => {
                failure = ERROR_OPERATION_ABORTED;
                return Err("abandon demandé par le rappel".into());
            }
            other => {
                failure = other;
                return Err(format!("rappel : {other}"));
            }
        }
        let target = full_target(l, &s);
        let mut error = 0u32;
        if let Err(e) = std::fs::write(crate::paths::to_linux(&target), bytes) {
            crate::trace!("SetupIterateCabinetW : écriture de « {target} » : {e}");
            error = ERROR_WRITE_FAULT;
        }
        put(&mut s.info, l.win32_error, &error.to_le_bytes());
        s.paths = [0; 24];
        let target_at = s.info.as_ptr() as usize + l.full_target;
        put_ptr(&mut s.paths, 0, l.ps, target_at);
        put_ptr(&mut s.paths, l.ps, l.ps, cabinet as usize);
        put(&mut s.paths, l.filepaths_error, &error.to_le_bytes());
        let r = handler(
            context,
            SPFILENOTIFY_FILEEXTRACTED,
            s.paths.as_mut_ptr() as usize,
            0,
        );
        if r != 0 {
            failure = r;
            return Err(format!("rappel après extraction : {r}"));
        }
        if error != 0 {
            failure = error;
            return Err("extraction".into());
        }
        Ok(())
    });
    match result {
        Ok(()) => {
            set_last_error(0);
            1
        }
        Err(e) => {
            crate::trace!("SetupIterateCabinetW : {e}");
            set_last_error(if failure != 0 {
                failure
            } else {
                ERROR_INVALID_DATA
            });
            0
        }
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetupIterateCabinetW" => fn SetupIterateCabinetW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<(u32, String, u32)>> = Mutex::new(Vec::new());

    fn wide_at(p: usize) -> String {
        unsafe { crate::dlls::wstr(p as *const u16) }
    }

    /// Rappel x64 : extrait « garde.txt » vers le chemin donné par le
    /// contexte, ignore le reste.
    unsafe extern "win64" fn rappel(ctx: usize, n: u32, p1: usize, p2: usize) -> u32 {
        let l = layout(8);
        match n {
            SPFILENOTIFY_FILEINCABINET => {
                let info = p1 as *mut u8;
                let name = wide_at(*(info as *const usize));
                let size = *(info.add(l.file_size) as *const u32);
                SEEN.lock().unwrap().push((n, name.clone(), size));
                assert!(wide_at(p2).ends_with("essai.cab"));
                if name != "garde.txt" {
                    return FILEOP_SKIP;
                }
                let target = wide_at(ctx);
                let dst = info.add(l.full_target) as *mut u16;
                for (i, c) in target.encode_utf16().chain([0]).enumerate() {
                    *dst.add(i) = c;
                }
                FILEOP_DOIT
            }
            SPFILENOTIFY_FILEEXTRACTED => {
                let paths = p1 as *const u8;
                let target = wide_at(*(paths as *const usize));
                let source = wide_at(*(paths.add(8) as *const usize));
                let err = *(paths.add(l.filepaths_error) as *const u32);
                assert!(source.ends_with("essai.cab"));
                SEEN.lock().unwrap().push((n, target, err));
                0
            }
            _ => 0xdead,
        }
    }

    unsafe extern "win64" fn abandon(_: usize, _: u32, _: usize, _: usize) -> u32 {
        FILEOP_ABORT
    }

    #[test]
    fn cabinet_parcouru_et_extrait_2713() {
        let _teb = crate::process::test_teb();
        let dir = std::env::temp_dir().join(format!("winlux-2713-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cab = crate::msi::cab::tests::build_mszip(&[
            ("ignore.bin", b"rien"),
            ("garde.txt", b"contenu garde"),
        ]);
        let cab_path = dir.join("essai.cab");
        std::fs::write(&cab_path, &cab).unwrap();
        let out = dir.join("sortie.txt");
        let w = |s: &std::path::Path| -> Vec<u16> {
            s.to_str().unwrap().encode_utf16().chain([0]).collect()
        };
        let (wcab, wout) = (w(&cab_path), w(&out));
        unsafe {
            let ok = SetupIterateCabinetW(wcab.as_ptr(), 0, Some(rappel), wout.as_ptr() as usize);
            assert_eq!(ok, 1);
        }
        assert_eq!(std::fs::read(&out).unwrap(), b"contenu garde");
        let seen = SEEN.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![
                (SPFILENOTIFY_FILEINCABINET, "ignore.bin".into(), 4),
                (SPFILENOTIFY_FILEINCABINET, "garde.txt".into(), 13),
                (SPFILENOTIFY_FILEEXTRACTED, out.to_str().unwrap().into(), 0),
            ]
        );
        unsafe {
            let ok = SetupIterateCabinetW(wcab.as_ptr(), 0, Some(abandon), 0);
            assert_eq!(ok, 0);
            assert_eq!(crate::process::last_error(), ERROR_OPERATION_ABORTED);
            let absent: Vec<u16> = w(&dir.join("absent.cab"));
            assert_eq!(SetupIterateCabinetW(absent.as_ptr(), 0, Some(rappel), 0), 0);
            assert_eq!(crate::process::last_error(), ERROR_FILE_NOT_FOUND);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disposition_x86_des_structures_2713() {
        // FILE_IN_CABINET_INFO_W x86 packed(1) : 4+4+4+2+2+2 = 18 avant
        // FullTargetName ; FILEPATHS_W x86 : Target, Source, Win32Error, Flags.
        let l = layout(4);
        assert_eq!(
            (l.file_size, l.win32_error, l.dos_date, l.full_target),
            (4, 8, 12, 18)
        );
        assert_eq!(l.filepaths_error, 8);
        assert_eq!(l.full_target + 2 * MAX_PATH, 538);
        let l = layout(8);
        assert_eq!(
            (l.file_size, l.win32_error, l.dos_date, l.full_target),
            (8, 12, 16, 22)
        );
        assert_eq!(l.filepaths_error, 16);
        let mut s = Scratch {
            info: [0; 544],
            paths: [0; 24],
            name: [0; 1024],
        };
        let l = layout(4);
        unsafe extern "win64" fn lit(_: usize, _: u32, p1: usize, _: usize) -> u32 {
            let b = std::slice::from_raw_parts(p1 as *const u8, 18);
            assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 13);
            assert_eq!(u16::from_le_bytes(b[12..14].try_into().unwrap()), 0x5021);
            assert_eq!(u16::from_le_bytes(b[14..16].try_into().unwrap()), 0x6000);
            assert_eq!(u16::from_le_bytes(b[16..18].try_into().unwrap()), 0x20);
            FILEOP_SKIP
        }
        let r = unsafe {
            notify_file_in_cabinet(
                lit,
                0,
                std::ptr::null(),
                l,
                &mut s,
                "a",
                13,
                (0x5021, 0x6000, 0x20),
            )
        };
        assert_eq!(r, FILEOP_SKIP);
        // NameInCabinet en 4 octets à +0.
        let p = u32::from_le_bytes(s.info[0..4].try_into().unwrap()) as usize;
        assert_eq!(p, s.name.as_ptr() as usize & 0xFFFF_FFFF);
    }
}
