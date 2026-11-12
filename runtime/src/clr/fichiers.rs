//! Primitives de fichiers de la mscorlib Winlux (#2012) : la classe interne
//! `System.IO.Natif` de notre mscorlib appelle ici les fonctions de kernel32
//! (CreateFileW, ReadFile, FindFirstFileW…). Un programme .NET voit donc les
//! mêmes chemins Windows (« C:\… », « Program Files (x86) » pour un 32 bits),
//! les mêmes codes d'erreur et les mêmes règles de partage qu'un programme
//! natif. Toute la logique (FileStream, File, Directory, Path) est en C#.
//!
//! Convention : une primitive qui échoue rend le code d'erreur Win32 en
//! négatif ; le C# le change en exception .NET.

use super::bcl::{alloc_utf16, managed_utf16};
use super::exceptions::ClrException;
use super::gc;
use super::il::{Interpreter, Value};
use super::types;
use crate::dlls::kernel32::{self, FileTime, Win32FileAttributeData};
use crate::dlls::winfile::{self, Win32FindDataW};

const INVALID_HANDLE: usize = usize::MAX;
/// Le tableau d'octets managé : longueur à +0, données à +8.
const DATA: usize = 8;

fn arg_ref(args: &[Value], i: usize) -> *mut u8 {
    match args.get(i) {
        Some(Value::Ref(p)) => *p,
        _ => std::ptr::null_mut(),
    }
}

fn arg_i64(args: &[Value], i: usize) -> i64 {
    args.get(i).map_or(0, |v| v.as_i64())
}

/// Chaîne managée en UTF-16 terminé par un zéro, pour kernel32.
fn wide(args: &[Value], i: usize) -> Vec<u16> {
    let mut w = managed_utf16(arg_ref(args, i));
    w.push(0);
    w
}

fn last_error() -> i64 {
    match crate::process::last_error() {
        0 => -31, // ERROR_GEN_FAILURE : échec sans code
        e => -i64::from(e),
    }
}

/// Résultat BOOL de kernel32 : 0 ou le code d'erreur en négatif.
fn status(ok: i32) -> Value {
    Value::I32(if ok != 0 { 0 } else { last_error() as i32 })
}

fn filetime_ticks(t: &FileTime) -> i64 {
    (i64::from(t.high) << 32) | i64::from(t.low)
}

/// Morceau `[off, off + count)` d'un tableau d'octets, vérifié.
fn byte_span(arr: *mut u8, off: i64, count: i64) -> Option<*mut u8> {
    if arr.is_null() || off < 0 || count < 0 {
        return None;
    }
    let len = unsafe { *(arr as *const u64) } as i64;
    if off + count > len {
        return None;
    }
    Some(unsafe { arr.add(DATA + off as usize) })
}

fn string_array(items: &[Vec<u16>]) -> Value {
    let arr = gc::with_gc(|gc| gc.alloc_array_of(types::TYPE_STRING, 8, items.len()));
    for (i, s) in items.iter().enumerate() {
        let Value::Ref(p) = alloc_utf16(s) else {
            continue;
        };
        unsafe { *(arr.add(DATA + i * 8) as *mut *mut u8) = p };
    }
    Value::Ref(arr)
}

impl Interpreter {
    pub(super) fn file_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, ClrException>> {
        if (namespace, class) != ("System.IO", "Natif") {
            return None;
        }
        Some(Ok(unsafe { natif(name, args) }?))
    }
}

/// # Safety
/// Les arguments viennent des déclarations `extern` de `System.IO.Natif`.
unsafe fn natif(name: &str, args: &[Value]) -> Option<Value> {
    Some(match name {
        // Ouvrir(chemin, disposition, accès, partage, options) : le handle ou
        // -erreur. La disposition est celle de CreateFileW (FileMode 1 à 5
        // ont les mêmes valeurs ; Append est changé en OPEN_ALWAYS en C#).
        "Ouvrir" => {
            let path = wide(args, 0);
            crate::process::set_last_error(0);
            let h = kernel32::create_file_w(
                path.as_ptr(),
                arg_i64(args, 2) as u32,
                arg_i64(args, 3) as u32,
                0,
                arg_i64(args, 1) as u32,
                arg_i64(args, 4) as u32,
                0,
            );
            Value::I64(if h == INVALID_HANDLE || h == 0 {
                last_error()
            } else {
                h as i64
            })
        }
        "Lire" | "Ecrire" => {
            let h = arg_i64(args, 0) as usize;
            let count = arg_i64(args, 3);
            let Some(p) = byte_span(arg_ref(args, 1), arg_i64(args, 2), count) else {
                return Some(Value::I32(-87));
            };
            let mut done = 0u32;
            let ok = if name == "Lire" {
                kernel32::read_file(h, p, count as u32, &mut done, 0)
            } else {
                kernel32::write_file(h, p, count as u32, &mut done, 0)
            };
            if ok == 0 {
                // Fin d'un tube : 0 octet, comme .NET.
                match crate::process::last_error() {
                    109 => Value::I32(0),
                    _ => Value::I32(last_error() as i32),
                }
            } else {
                Value::I32(done as i32)
            }
        }
        // Positionner(h, déplacement, origine) : nouvelle position ou -erreur.
        "Positionner" => {
            let mut pos = 0i64;
            let ok = kernel32::set_file_pointer_ex(
                arg_i64(args, 0) as usize,
                arg_i64(args, 1),
                &mut pos,
                arg_i64(args, 2) as u32,
            );
            Value::I64(if ok != 0 { pos } else { last_error() })
        }
        "Taille" => {
            let mut size = 0i64;
            let ok = kernel32::get_file_size_ex(arg_i64(args, 0) as usize, &mut size);
            Value::I64(if ok != 0 { size } else { last_error() })
        }
        // FixerTaille(h, longueur) : la position courante est gardée, sauf
        // si elle dépasse la nouvelle fin (comme FileStream.SetLength).
        "FixerTaille" => {
            let h = arg_i64(args, 0) as usize;
            let len = arg_i64(args, 1);
            let mut pos = 0i64;
            if kernel32::set_file_pointer_ex(h, 0, &mut pos, 1) == 0
                || kernel32::set_file_pointer_ex(h, len, std::ptr::null_mut(), 0) == 0
                || kernel32::set_end_of_file(h) == 0
            {
                return Some(Value::I32(last_error() as i32));
            }
            kernel32::set_file_pointer_ex(h, pos.min(len), std::ptr::null_mut(), 0);
            Value::I32(0)
        }
        "Vider" => status(kernel32::flush_file_buffers(arg_i64(args, 0) as usize)),
        "Fermer" => status(kernel32::close_handle(arg_i64(args, 0) as usize)),
        // Infos(chemin) : [attributs, création, accès, écriture, taille] en
        // graduations FILETIME, ou null si le chemin n'existe pas.
        "Infos" => {
            let path = wide(args, 0);
            let mut data: Win32FileAttributeData = std::mem::zeroed();
            if kernel32::get_file_attributes_ex_w(path.as_ptr(), 0, &mut data) == 0 {
                return Some(Value::Null);
            }
            let values = [
                i64::from(data.attributes),
                filetime_ticks(&data.creation_time),
                filetime_ticks(&data.access_time),
                filetime_ticks(&data.write_time),
                (i64::from(data.size_high) << 32) | i64::from(data.size_low),
            ];
            let arr = gc::with_gc(|gc| gc.alloc_array_of(types::TYPE_INT64, 8, values.len()));
            for (i, v) in values.iter().enumerate() {
                *(arr.add(DATA + i * 8) as *mut i64) = *v;
            }
            Value::Ref(arr)
        }
        "FixerAttributs" => {
            let path = wide(args, 0);
            status(kernel32::set_file_attributes_w(
                path.as_ptr(),
                arg_i64(args, 1) as u32,
            ))
        }
        // FixerDateEcriture(chemin, graduations FILETIME).
        "FixerDateEcriture" => {
            let path = wide(args, 0);
            let h = kernel32::create_file_w(path.as_ptr(), 0x100, 7, 0, 3, 0x0200_0000, 0);
            if h == INVALID_HANDLE {
                return Some(Value::I32(last_error() as i32));
            }
            let t = arg_i64(args, 1) as u64;
            let ft = FileTime {
                low: t as u32,
                high: (t >> 32) as u32,
            };
            let r = status(kernel32::set_file_time(
                h,
                std::ptr::null(),
                std::ptr::null(),
                &ft,
            ));
            kernel32::close_handle(h);
            r
        }
        "CreerDossier" => {
            let path = wide(args, 0);
            status(kernel32::create_directory_w(path.as_ptr(), 0))
        }
        "SupprimerDossier" => {
            let path = wide(args, 0);
            status(kernel32::remove_directory_w(path.as_ptr()))
        }
        "SupprimerFichier" => {
            let path = wide(args, 0);
            status(kernel32::delete_file_w(path.as_ptr()))
        }
        // Deplacer(source, destination, remplacer).
        "Deplacer" => {
            let (src, dst) = (wide(args, 0), wide(args, 1));
            let flags = if arg_i64(args, 2) != 0 { 1 | 2 } else { 2 };
            status(kernel32::move_file_ex_w(src.as_ptr(), dst.as_ptr(), flags))
        }
        // Copier(source, destination, remplacer).
        "Copier" => {
            let (src, dst) = (wide(args, 0), wide(args, 1));
            let fail_if_exists = i32::from(arg_i64(args, 2) == 0);
            status(kernel32::copy_file_w(
                src.as_ptr(),
                dst.as_ptr(),
                fail_if_exists,
            ))
        }
        // Lister(motif complet, quoi) : noms des entrées (sans « . » ni
        // « .. »), quoi = 1 fichiers, 2 dossiers, 3 les deux. Un dossier
        // absent rend null.
        "Lister" => {
            let pattern = wide(args, 0);
            let what = arg_i64(args, 1);
            let mut data: Win32FindDataW = std::mem::zeroed();
            let h = winfile::find_first_file_w(pattern.as_ptr(), &mut data);
            if h == INVALID_HANDLE {
                return Some(match crate::process::last_error() {
                    2 | 18 => string_array(&[]),
                    _ => Value::Null,
                });
            }
            let mut names = Vec::new();
            loop {
                let raw = data.file_name;
                let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
                let name = &raw[..end];
                let attributes = data.attributes;
                let is_dir = attributes & 0x10 != 0;
                let dots = name == [b'.' as u16] || name == [b'.' as u16, b'.' as u16];
                if !dots && what & if is_dir { 2 } else { 1 } != 0 {
                    names.push(name.to_vec());
                }
                if winfile::find_next_file_w(h, &mut data) == 0 {
                    break;
                }
            }
            winfile::find_close(h);
            string_array(&names)
        }
        "CheminComplet" => {
            let path = wide(args, 0);
            let mut buf = vec![0u16; 1024];
            let mut n = kernel32::get_full_path_name_w(
                path.as_ptr(),
                buf.len() as u32,
                buf.as_mut_ptr(),
                std::ptr::null_mut(),
            ) as usize;
            if n > buf.len() {
                buf.resize(n + 1, 0);
                n = kernel32::get_full_path_name_w(
                    path.as_ptr(),
                    buf.len() as u32,
                    buf.as_mut_ptr(),
                    std::ptr::null_mut(),
                ) as usize;
            }
            if n == 0 || n > buf.len() {
                return Some(Value::Null);
            }
            alloc_utf16(&buf[..n])
        }
        "DossierTemp" => {
            let mut buf = vec![0u16; 1024];
            let n = kernel32::get_temp_path_w(buf.len() as u32, buf.as_mut_ptr()) as usize;
            alloc_utf16(&buf[..n.min(buf.len())])
        }
        "DossierCourant" => {
            let mut buf = vec![0u16; 1024];
            let n = kernel32::get_current_directory_w(buf.len() as u32, buf.as_mut_ptr()) as usize;
            alloc_utf16(&buf[..n.min(buf.len())])
        }
        "FixerDossierCourant" => {
            let path = wide(args, 0);
            status(kernel32::set_current_directory_w(path.as_ptr()))
        }
        _ => return None,
    })
}
