//! Pont P/Invoke : un `[DllImport]` managé est résolu dans la table d'exports
//! des DLL intégrées de Winlux (`dlls::lookup`) puis appelé avec la convention
//! Win64, exactement comme le ferait un programme natif.

use super::bcl;
use super::il::Value;
use super::metadata::ElementType;

pub struct Import<'a> {
    pub module: &'a str,
    pub name: &'a str,
    /// CharSet.Unicode / Auto : chaînes en UTF-16 et suffixe « W ».
    pub unicode: bool,
}

/// Appelle la fonction native. L'erreur nomme l'exception .NET à lever.
pub fn call(
    import: &Import,
    args: &[Value],
    params: &[ElementType],
    ret: &ElementType,
) -> Result<Value, (&'static str, String)> {
    let dll = crate::dlls::normalize(import.module);
    if !crate::dlls::is_builtin(&dll) {
        return Err((
            "DllNotFoundException",
            format!("Unable to load DLL '{}'", import.module),
        ));
    }
    let suffixed = format!("{}{}", import.name, if import.unicode { 'W' } else { 'A' });
    let Some(addr) =
        crate::dlls::lookup(&dll, import.name).or_else(|| crate::dlls::lookup(&dll, &suffixed))
    else {
        return Err((
            "EntryPointNotFoundException",
            format!(
                "Unable to find an entry point named '{}' in DLL '{}'",
                import.name, import.module
            ),
        ));
    };

    // Les tampons des chaînes marshalées doivent vivre jusqu'au retour.
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    let mut raw = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        let param = params.get(i);
        if matches!(param, Some(ElementType::R4 | ElementType::R8)) {
            return Err((
                "NotSupportedException",
                format!("{} : paramètre flottant non pris en charge", import.name),
            ));
        }
        let word = match (arg, param) {
            (Value::Ref(p), Some(ElementType::String)) => {
                let text = bcl::read_managed_string(*p);
                let buf = if import.unicode {
                    text.encode_utf16()
                        .chain([0])
                        .flat_map(u16::to_le_bytes)
                        .collect()
                } else {
                    text.bytes().chain([0]).collect()
                };
                buffers.push(buf);
                buffers.last().unwrap().as_ptr() as usize
            }
            // Tableau de valeurs blittables : on passe l'adresse du premier élément.
            (Value::Ref(p), Some(ElementType::SzArray(_))) if !p.is_null() => *p as usize + 8,
            (Value::Null, _) => 0,
            (v, _) => v.as_i64() as usize,
        };
        raw.push(word);
    }

    crate::trace!("P/Invoke {}!{} {:x?}", dll, import.name, raw);
    let r = unsafe { invoke(addr, &raw) }.ok_or((
        "NotSupportedException",
        format!("{} : trop d'arguments ({})", import.name, raw.len()),
    ))?;

    Ok(match ret {
        ElementType::Void => Value::Null,
        ElementType::I8
        | ElementType::U8
        | ElementType::IntPtr
        | ElementType::UIntPtr
        | ElementType::Ptr(_) => Value::I64(r as i64),
        ElementType::String if r != 0 => {
            let text = if import.unicode {
                let p = r as *const u16;
                let len = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
            } else {
                unsafe { std::ffi::CStr::from_ptr(r as *const std::ffi::c_char) }
                    .to_string_lossy()
                    .into_owned()
            };
            bcl::alloc_managed_string(&text)
        }
        ElementType::String => Value::Null,
        _ => Value::I32(r as i32),
    })
}

unsafe fn invoke(addr: usize, a: &[usize]) -> Option<usize> {
    type F0 = extern "win64" fn() -> usize;
    type F1 = extern "win64" fn(usize) -> usize;
    type F2 = extern "win64" fn(usize, usize) -> usize;
    type F3 = extern "win64" fn(usize, usize, usize) -> usize;
    type F4 = extern "win64" fn(usize, usize, usize, usize) -> usize;
    type F5 = extern "win64" fn(usize, usize, usize, usize, usize) -> usize;
    type F6 = extern "win64" fn(usize, usize, usize, usize, usize, usize) -> usize;
    type F7 = extern "win64" fn(usize, usize, usize, usize, usize, usize, usize) -> usize;
    type F8 = extern "win64" fn(usize, usize, usize, usize, usize, usize, usize, usize) -> usize;
    type F9 =
        extern "win64" fn(usize, usize, usize, usize, usize, usize, usize, usize, usize) -> usize;
    type F10 = extern "win64" fn(
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    ) -> usize;
    type F11 = extern "win64" fn(
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    ) -> usize;
    type F12 = extern "win64" fn(
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    ) -> usize;
    let p = addr as *const ();
    Some(match *a {
        [] => std::mem::transmute::<*const (), F0>(p)(),
        [a0] => std::mem::transmute::<*const (), F1>(p)(a0),
        [a0, a1] => std::mem::transmute::<*const (), F2>(p)(a0, a1),
        [a0, a1, a2] => std::mem::transmute::<*const (), F3>(p)(a0, a1, a2),
        [a0, a1, a2, a3] => std::mem::transmute::<*const (), F4>(p)(a0, a1, a2, a3),
        [a0, a1, a2, a3, a4] => std::mem::transmute::<*const (), F5>(p)(a0, a1, a2, a3, a4),
        [a0, a1, a2, a3, a4, a5] => std::mem::transmute::<*const (), F6>(p)(a0, a1, a2, a3, a4, a5),
        [a0, a1, a2, a3, a4, a5, a6] => {
            std::mem::transmute::<*const (), F7>(p)(a0, a1, a2, a3, a4, a5, a6)
        }
        [a0, a1, a2, a3, a4, a5, a6, a7] => {
            std::mem::transmute::<*const (), F8>(p)(a0, a1, a2, a3, a4, a5, a6, a7)
        }
        [a0, a1, a2, a3, a4, a5, a6, a7, a8] => {
            std::mem::transmute::<*const (), F9>(p)(a0, a1, a2, a3, a4, a5, a6, a7, a8)
        }
        [a0, a1, a2, a3, a4, a5, a6, a7, a8, a9] => {
            std::mem::transmute::<*const (), F10>(p)(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9)
        }
        [a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10] => {
            std::mem::transmute::<*const (), F11>(p)(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10)
        }
        [a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11] => {
            std::mem::transmute::<*const (), F12>(p)(
                a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11,
            )
        }
        _ => return None,
    })
}
