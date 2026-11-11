//! oleaut32, reste du lot C du relevé (#1511) : conversions vers CY,
//! hachage de nom, désinscription d'une bibliothèque de types et feuille
//! de propriétés.
//!
//! Sources : documentation Microsoft Learn de VarCyFromR8, VarCyFromDec,
//! CY, DECIMAL, LHashValOfNameSys (et des macros WHashValOfLHashVal,
//! IsHashValCompatible d'oleauto.h), UnRegisterTypeLib et
//! OleCreatePropertyFrame. Noms et ordinaux (102, 165, 186, 228, 417) :
//! bibliothèque d'importation OleAut32.Lib du SDK Windows public.

use super::comobj::Guid;

const S_OK: i32 = 0;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const DISP_E_OVERFLOW: i32 = 0x8002_000Au32 as i32;
const TYPE_E_LIBNOTREGISTERED: i32 = 0x8002_801Du32 as i32;

/// CY : entier sur 64 bits, valeur × 10 000, arrondi au pair le plus proche.
pub unsafe extern "win64" fn var_cy_from_r8(value: f64, out: *mut i64) -> i32 {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let scaled = (value * 10_000.0).round_ties_even();
    // i64::MAX n'est pas représentable exactement : borne stricte.
    if !scaled.is_finite()
        || scaled >= 9.223_372_036_854_776e18
        || scaled < -9.223_372_036_854_776e18
    {
        return DISP_E_OVERFLOW;
    }
    std::ptr::write_unaligned(out, scaled as i64);
    S_OK
}

/// DECIMAL : wReserved (0), scale (2), sign (3, 0x80 = négatif), Hi32 (4),
/// Lo64 (8) ; valeur = (Hi32:Lo64) / 10^scale.
pub unsafe extern "win64" fn var_cy_from_dec(dec: *const u8, out: *mut i64) -> i32 {
    if dec.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let scale = u32::from(*dec.add(2));
    let negative = *dec.add(3) & 0x80 != 0;
    if scale > 28 {
        return E_INVALIDARG;
    }
    let hi = u128::from(std::ptr::read_unaligned(dec.add(4) as *const u32));
    let lo = u128::from(std::ptr::read_unaligned(dec.add(8) as *const u64));
    let mantissa = (hi << 64) | lo;
    match dec_to_cy(mantissa, scale, negative) {
        Some(v) => {
            std::ptr::write_unaligned(out, v);
            S_OK
        }
        None => DISP_E_OVERFLOW,
    }
}

/// mantisse / 10^scale × 10 000, arrondi au pair.
fn dec_to_cy(mantissa: u128, scale: u32, negative: bool) -> Option<i64> {
    let magnitude = if scale <= 4 {
        mantissa.checked_mul(10u128.pow(4 - scale))?
    } else {
        let div = 10u128.pow(scale - 4);
        let (q, r) = (mantissa / div, mantissa % div);
        let twice = r * 2;
        if twice > div || (twice == div && q % 2 == 1) {
            q + 1
        } else {
            q
        }
    };
    if negative {
        if magnitude > 1u128 << 63 {
            return None;
        }
        Some((magnitude as i128).wrapping_neg() as i64)
    } else {
        i64::try_from(magnitude).ok()
    }
}

/// Hachage d'un nom pour ITypeComp::Bind. L'algorithme de Windows n'est pas
/// publié : la valeur ne sert qu'à être rendue aux fonctions de Winlux
/// (ITypeComp n'en tient pas compte). Mot bas : nom sans la casse ; octet
/// 16-23 : système et langue, comparé par IsHashValCompatible.
pub unsafe extern "win64" fn lhash_val_of_name_sys(
    syskind: u32,
    lcid: u32,
    name: *const u16,
) -> u32 {
    if name.is_null() {
        return 0;
    }
    let text = super::oleaut32_typelib::wide(name).to_uppercase();
    let mut h: u32 = 0x811C_9DC5;
    for b in text.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    let low = (h ^ (h >> 16)) & 0xFFFF;
    let compat = ((syskind & 0xF) << 4) | (lcid & 0xF);
    (compat << 16) | low
}

pub unsafe extern "win64" fn un_register_type_lib(
    lib: *const Guid,
    major: u16,
    minor: u16,
    _lcid: u32,
    _syskind: u32,
) -> i32 {
    if lib.is_null() {
        return E_INVALIDARG;
    }
    let g = std::ptr::read_unaligned(lib as *const super::ole32::Guid);
    if super::oleaut32_typelib::unregister(&g, major, minor) {
        S_OK
    } else {
        TYPE_E_LIBNOTREGISTERED
    }
}

/// Aucune page de propriétés n'est hébergée : la feuille ne s'ouvre pas.
#[allow(clippy::too_many_arguments)]
pub extern "win64" fn ole_create_property_frame(
    _owner: usize,
    _x: u32,
    _y: u32,
    _caption: usize,
    _objects: u32,
    _units: usize,
    _pages: u32,
    _page_clsids: usize,
    _lcid: u32,
    _reserved: u32,
    _reserved_ptr: usize,
) -> i32 {
    E_NOTIMPL
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "VarCyFromR8" => fn var_cy_from_r8,
        "VarCyFromDec" => fn var_cy_from_dec,
        "LHashValOfNameSys" => fn lhash_val_of_name_sys,
        "UnRegisterTypeLib" => fn un_register_type_lib,
        "OleCreatePropertyFrame" => fn ole_create_property_frame,
        "#102" => fn var_cy_from_r8,
        "#165" => fn lhash_val_of_name_sys,
        "#186" => fn un_register_type_lib,
        "#228" => fn var_cy_from_dec,
        "#417" => fn ole_create_property_frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_vers_cy_hachage_et_desinscription() {
        unsafe {
            let mut cy = 0i64;
            assert_eq!(var_cy_from_r8(1.5, &mut cy), S_OK);
            assert_eq!(cy, 15_000);
            // Arrondi au pair : 0,00005 → 0 ; 0,00015 → 0,0002.
            assert_eq!(var_cy_from_r8(0.000_05, &mut cy), S_OK);
            assert_eq!(cy, 0);
            assert_eq!(var_cy_from_r8(-2.25, &mut cy), S_OK);
            assert_eq!(cy, -22_500);
            assert_eq!(var_cy_from_r8(1e300, &mut cy), DISP_E_OVERFLOW);
            assert_eq!(var_cy_from_r8(f64::NAN, &mut cy), DISP_E_OVERFLOW);

            // 123,456789 : mantisse 123456789, échelle 6 → 1 234 568.
            let mut dec = [0u8; 16];
            dec[2] = 6;
            dec[8..16].copy_from_slice(&123_456_789u64.to_le_bytes());
            assert_eq!(var_cy_from_dec(dec.as_ptr(), &mut cy), S_OK);
            assert_eq!(cy, 1_234_568);
            dec[3] = 0x80;
            assert_eq!(var_cy_from_dec(dec.as_ptr(), &mut cy), S_OK);
            assert_eq!(cy, -1_234_568);
            dec = [0u8; 16];
            dec[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
            assert_eq!(var_cy_from_dec(dec.as_ptr(), &mut cy), DISP_E_OVERFLOW);
            assert_eq!(dec_to_cy(25, 5, false), Some(2)); // 0,00025 → 0,0002
            assert_eq!(dec_to_cy(35, 5, false), Some(4)); // 0,00035 → 0,0004

            let a: Vec<u16> = "Ajoute\0".encode_utf16().collect();
            let b: Vec<u16> = "AJOUTE\0".encode_utf16().collect();
            let ha = lhash_val_of_name_sys(3, 0x409, a.as_ptr());
            assert_eq!(ha, lhash_val_of_name_sys(3, 0x409, b.as_ptr()));
            assert_eq!(
                ha & 0x00FF_0000,
                lhash_val_of_name_sys(3, 0x409, a.as_ptr()) & 0x00FF_0000
            );

            let inconnue = [0x15u8; 16];
            assert_eq!(
                un_register_type_lib(inconnue.as_ptr().cast(), 1, 0, 0, 3),
                TYPE_E_LIBNOTREGISTERED
            );
            assert_eq!(
                ole_create_property_frame(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                E_NOTIMPL
            );
        }
    }
}
