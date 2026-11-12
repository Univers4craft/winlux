//! Primitives du registre de la mscorlib Winlux (#2020) : la classe interne
//! `Microsoft.Win32.RegistreNatif` de notre mscorlib appelle ici les
//! fonctions d'advapi32 (RegOpenKeyExW, RegQueryValueExW, RegEnumKeyExW…).
//! Un programme .NET lit donc le même registre, avec la même vue 32/64 bits
//! (`KEY_WOW64_32KEY`, `KEY_WOW64_64KEY`), qu'un programme natif. Toute la
//! logique de `RegistryKey` (conversion des valeurs, exceptions) est en C#.
//!
//! Convention : une primitive qui échoue rend le code d'erreur Win32 en
//! négatif.

use super::bcl::{alloc_utf16, managed_utf16};
use super::exceptions::ClrException;
use super::gc;
use super::il::{Interpreter, Value};
use super::types;
use crate::dlls::advapi32 as api;

const ERROR_MORE_DATA: i32 = 234;
const ERROR_NO_MORE_ITEMS: i32 = 259;
/// Le tableau managé : longueur à +0, données à +8.
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

/// Chaîne managée en UTF-16 terminé par un zéro (null donne la chaîne vide,
/// qui désigne la valeur par défaut d'une clé).
fn wide(args: &[Value], i: usize) -> Vec<u16> {
    let mut w = managed_utf16(arg_ref(args, i));
    w.push(0);
    w
}

fn code(r: i32) -> i32 {
    if r == 0 {
        0
    } else {
        -r.abs()
    }
}

fn string_array(items: &[Vec<u16>]) -> Value {
    let arr = gc::with_gc(|gc| gc.alloc_array_of(types::TYPE_STRING, 8, items.len()));
    for (i, s) in items.iter().enumerate() {
        if let Value::Ref(p) = alloc_utf16(s) {
            unsafe { *(arr.add(DATA + i * 8) as *mut *mut u8) = p };
        }
    }
    Value::Ref(arr)
}

/// Noms rendus un par un par RegEnumKeyExW ou RegEnumValueW.
unsafe fn enumerate(h: usize, values: bool) -> Result<Vec<Vec<u16>>, i32> {
    let mut names = Vec::new();
    let mut buf = vec![0u16; 16384];
    for index in 0.. {
        let mut size = buf.len() as u32;
        let r = if values {
            api::reg_enum_value_w(
                h,
                index,
                buf.as_mut_ptr(),
                &mut size,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } else {
            api::reg_enum_key_ex_w(
                h,
                index,
                buf.as_mut_ptr(),
                &mut size,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        match r {
            0 => names.push(buf[..(size as usize).min(buf.len())].to_vec()),
            ERROR_NO_MORE_ITEMS => break,
            e => return Err(e),
        }
    }
    Ok(names)
}

impl Interpreter {
    pub(super) fn registry_call(
        &mut self,
        namespace: &str,
        class: &str,
        name: &str,
        args: &[Value],
    ) -> Option<Result<Value, ClrException>> {
        if (namespace, class) != ("Microsoft.Win32", "RegistreNatif") {
            return None;
        }
        Some(Ok(unsafe { natif(name, args) }?))
    }
}

/// # Safety
/// Les arguments viennent des déclarations `extern` de `RegistreNatif`.
unsafe fn natif(name: &str, args: &[Value]) -> Option<Value> {
    let h = arg_i64(args, 0) as usize;
    Some(match name {
        // Ouvrir(racine, sous-clé, accès) et Creer(…) : le handle ou -erreur.
        "Ouvrir" | "Creer" => {
            let sub = wide(args, 1);
            let access = arg_i64(args, 2) as u32;
            let mut key = 0usize;
            let r = if name == "Ouvrir" {
                api::reg_open_key_ex_w(h, sub.as_ptr(), 0, access, &mut key)
            } else {
                let mut disposition = 0u32;
                api::reg_create_key_ex_w(
                    h,
                    sub.as_ptr(),
                    0,
                    std::ptr::null(),
                    0,
                    access,
                    0,
                    &mut key,
                    &mut disposition,
                )
            };
            Value::I64(if r == 0 {
                key as i64
            } else {
                i64::from(code(r))
            })
        }
        "Fermer" => Value::I32(code(api::reg_close_key(h))),
        "Vider" => Value::I32(code(api::reg_flush_key(h))),
        // Type(h, nom) : REG_* de la valeur, ou -erreur (2 : absente).
        "Type" => {
            let value = wide(args, 1);
            let mut kind = 0u32;
            let r = api::reg_query_value_ex_w(
                h,
                value.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            Value::I32(if r == 0 { kind as i32 } else { code(r) })
        }
        // Lire(h, nom) : les octets de la valeur, ou null si elle manque.
        "Lire" => {
            let value = wide(args, 1);
            let mut size = 0u32;
            let mut kind = 0u32;
            let mut data: Vec<u8> = Vec::new();
            loop {
                let r = api::reg_query_value_ex_w(
                    h,
                    value.as_ptr(),
                    std::ptr::null_mut(),
                    &mut kind,
                    if data.is_empty() {
                        std::ptr::null_mut()
                    } else {
                        data.as_mut_ptr()
                    },
                    &mut size,
                );
                match r {
                    0 if data.len() >= size as usize => break,
                    0 | ERROR_MORE_DATA => data.resize(size.max(1) as usize, 0),
                    _ => return Some(Value::Null),
                }
            }
            data.truncate(size as usize);
            let arr = gc::with_gc(|gc| gc.alloc_array_of(types::TYPE_BYTE, 1, data.len()));
            std::ptr::copy_nonoverlapping(data.as_ptr(), arr.add(DATA), data.len());
            Value::Ref(arr)
        }
        // Ecrire(h, nom, type, octets).
        "Ecrire" => {
            let value = wide(args, 1);
            let kind = arg_i64(args, 2) as u32;
            let arr = arg_ref(args, 3);
            let (ptr, len) = if arr.is_null() {
                (std::ptr::null(), 0)
            } else if gc::type_of(arr) != Some(types::TYPE_ARRAY)
                || !matches!(
                    gc::array_elem_type(arr),
                    types::TYPE_BYTE | types::TYPE_SBYTE
                )
            {
                // Joignable par réflexion : seul un vrai tableau d'octets
                // a sa longueur à +0 (#2017).
                return Some(Value::I32(-87));
            } else {
                (arr.add(DATA) as *const u8, *(arr as *const u64) as u32)
            };
            Value::I32(code(api::reg_set_value_ex_w(
                h,
                value.as_ptr(),
                0,
                kind,
                ptr,
                len,
            )))
        }
        "SupprimerValeur" => {
            let value = wide(args, 1);
            Value::I32(code(api::reg_delete_value_w(h, value.as_ptr())))
        }
        "SupprimerCle" => {
            let sub = wide(args, 1);
            Value::I32(code(api::reg_delete_key_ex_w(
                h,
                sub.as_ptr(),
                arg_i64(args, 2) as u32,
                0,
            )))
        }
        "SupprimerArbre" => {
            let sub = wide(args, 1);
            Value::I32(code(api::reg_delete_tree_w(h, sub.as_ptr())))
        }
        "SousCles" | "Valeurs" => match enumerate(h, name == "Valeurs") {
            Ok(names) => string_array(&names),
            Err(_) => Value::Null,
        },
        _ => return None,
    })
}
