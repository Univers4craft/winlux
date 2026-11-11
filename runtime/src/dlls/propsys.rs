//! propsys.dll — système de propriétés Windows. VariantToString et
//! VariantToStringAlloc d'après la documentation Microsoft Learn (#1597).
#![allow(non_snake_case)]

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;

pub unsafe extern "win64" fn PSGetPropertyKeyFromName(_name: *const u16, _key: usize) -> i32 {
    E_NOTIMPL
}
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
/// STRSAFE_E_INSUFFICIENT_BUFFER : tampon trop court, texte tronqué.
const STRSAFE_E_INSUFFICIENT_BUFFER: i32 = 0x8007_007Au32 as i32;
const VT_BSTR: u16 = 8;

/// Texte d'un VARIANT, converti en BSTR par VariantChangeType (#1597).
/// VT_EMPTY donne la chaîne vide.
unsafe fn variant_text(var: usize) -> Result<Vec<u16>, i32> {
    if var == 0 {
        return Err(E_INVALIDARG);
    }
    if *(var as *const u16) == 0 {
        return Ok(Vec::new());
    }
    // VARIANT de 24 octets (16 en x86), aligné ; la donnée est à +8.
    let mut tmp = [0u64; 3];
    let dest = tmp.as_mut_ptr().cast();
    let hr = super::oleaut32_var::change_type(dest, var as *const _, 0x400, 0, VT_BSTR);
    if hr < 0 {
        return Err(hr);
    }
    let b = tmp[1] as usize as *const u16;
    let text = if b.is_null() {
        Vec::new()
    } else {
        let len = super::oleaut32::sys_string_len(b) as usize;
        std::slice::from_raw_parts(b, len).to_vec()
    };
    super::oleaut32::variant_clear(dest);
    Ok(text)
}

/// VariantToString : texte dans un tampon de `cch` caractères, zéro final
/// compris ; trop court, le texte est tronqué et l'erreur
/// STRSAFE_E_INSUFFICIENT_BUFFER rendue.
pub unsafe extern "win64" fn VariantToString(var: usize, buf: *mut u16, cch: u32) -> i32 {
    if buf.is_null() || cch == 0 {
        return E_INVALIDARG;
    }
    let text = match variant_text(var) {
        Ok(t) => t,
        Err(hr) => {
            *buf = 0;
            return hr;
        }
    };
    let n = text.len().min(cch as usize - 1);
    std::ptr::copy_nonoverlapping(text.as_ptr(), buf, n);
    *buf.add(n) = 0;
    if n < text.len() {
        STRSAFE_E_INSUFFICIENT_BUFFER
    } else {
        0
    }
}

/// VariantToStringAlloc : texte alloué par CoTaskMemAlloc.
pub unsafe extern "win64" fn VariantToStringAlloc(var: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    crate::wow32::write_ptr(out as usize, 0);
    let text = match variant_text(var) {
        Ok(t) => t,
        Err(hr) => return hr,
    };
    let p = super::ole32::co_task_mem_alloc((text.len() + 1) * 2) as *mut u16;
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len());
    *p.add(text.len()) = 0;
    crate::wow32::write_ptr(out as usize, p as usize);
    0
}
pub unsafe extern "win64" fn InitPropVariantFromStringVector(
    _strs: usize,
    _count: u32,
    _propvar: usize,
) -> i32 {
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PSGetPropertyKeyFromName" => fn PSGetPropertyKeyFromName,
        "VariantToString" => fn VariantToString,
        "VariantToStringAlloc" => fn VariantToStringAlloc,
        "InitPropVariantFromStringVector" => fn InitPropVariantFromStringVector,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_vers_texte() {
        unsafe {
            // VT_I4 = 42.
            let v = [3u64, 42, 0];
            let mut buf = [0u16; 8];
            assert_eq!(VariantToString(v.as_ptr() as usize, buf.as_mut_ptr(), 8), 0);
            assert_eq!(&buf[..3], &[b'4' as u16, b'2' as u16, 0]);
            let mut court = [0u16; 2];
            assert_eq!(
                VariantToString(v.as_ptr() as usize, court.as_mut_ptr(), 2),
                STRSAFE_E_INSUFFICIENT_BUFFER
            );
            assert_eq!(court, [b'4' as u16, 0]);
            let vide = [0u64; 3];
            assert_eq!(
                VariantToString(vide.as_ptr() as usize, buf.as_mut_ptr(), 8),
                0
            );
            assert_eq!(buf[0], 0);
            // VT_BSTR, allocation par CoTaskMemAlloc.
            let s: Vec<u16> = "Été".encode_utf16().collect();
            let b = crate::dlls::oleaut32::sys_alloc_string_len(s.as_ptr(), s.len() as u32);
            let v = [8u64, b as u64, 0];
            let mut p = 0usize;
            assert_eq!(VariantToStringAlloc(v.as_ptr() as usize, &mut p), 0);
            assert_eq!(crate::dlls::wstr(p as *const u16), "Été");
            crate::dlls::ole32::co_task_mem_free(p as *mut _);
            crate::dlls::oleaut32::sys_free_string(b);
        }
    }
}
