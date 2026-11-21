//! DispCallFunc appelé par un programme x86 (#2800).
//!
//! `DispCallFunc(pvInstance, oVft, cc, vtReturn, cActuals, prgvt,
//! prgpvarg, pvargResult)` appelle une méthode (ou une fonction) avec des
//! arguments décrits par des VARIANT (documentation Microsoft Learn de
//! DispCallFunc). Un puits d'événements ATL d'un programme x86 s'en sert
//! pour appeler le gestionnaire qu'il a trouvé dans la bibliothèque de
//! types (#2796) ; la version du runtime ne sait appeler que du code win64.
//!
//! Ici tout est en mémoire x86 : `pvInstance` est un objet x86 dont la
//! table a des cases de 4 octets (`oVft` est un décalage en octets dans
//! cette table ; `pvInstance` nul : `oVft` est l'adresse de la fonction),
//! `prgpvarg` est un tableau de pointeurs de 4 octets vers des VARIANT de
//! 16 octets (vt à +0, valeur à +8, déclarations publiques d'oaidl.h du SDK
//! Windows, mêmes dans mingw-w64). Les arguments sont empilés comme le fait
//! un compilateur x86 :
//!
//! - une case de 4 octets pour les entiers de 4 octets au plus, les
//!   pointeurs, VT_BYREF et VT_ARRAY ;
//! - deux cases pour VT_I8, VT_UI8, VT_R8, VT_DATE et VT_CY (8 octets) ;
//! - quatre cases pour VT_VARIANT et VT_DECIMAL (16 octets passés par
//!   valeur).
//!
//! `this` est le premier argument empilé (CC_STDCALL, CC_CDECL). `call32`
//! rend edx:eax : les valeurs rendues entières sont servies ; une valeur
//! rendue dans ST0 (VT_R4, VT_R8, VT_DATE) ou un VARIANT/DECIMAL rendu par
//! adresse cachée ne le sont pas (DISP_E_BADVARTYPE, sans appeler).

use super::{call32, rd, wr};

const S_OK: i32 = 0;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const DISP_E_BADVARTYPE: i32 = 0x8002_0008u32 as i32;
const DISP_E_BADCALLEE: i32 = 0x8002_0010u32 as i32;

const CC_CDECL: u32 = 1;
const CC_STDCALL: u32 = 4;

const VT_EMPTY: u16 = 0;
const VT_R4: u16 = 4;
const VT_R8: u16 = 5;
const VT_CY: u16 = 6;
const VT_DATE: u16 = 7;
const VT_VARIANT: u16 = 12;
const VT_DECIMAL: u16 = 14;
const VT_I8: u16 = 20;
const VT_UI8: u16 = 21;
const VT_VOID: u16 = 24;
const VT_ARRAY: u16 = 0x2000;
const VT_BYREF: u16 = 0x4000;

/// Nombre de cases de 4 octets qu'occupe sur la pile x86 un argument de
/// type `vt`.
fn words(vt: u16) -> usize {
    if vt & (VT_BYREF | VT_ARRAY) != 0 {
        return 1;
    }
    match vt {
        VT_I8 | VT_UI8 | VT_R8 | VT_DATE | VT_CY => 2,
        VT_VARIANT | VT_DECIMAL => 4,
        _ => 1,
    }
}

/// Cases de pile x86 des arguments (après `this`), lues dans les VARIANT
/// x86 désignés par `args` (tableau de `count` pointeurs de 4 octets) et
/// typées par `types` (tableau de `count` VARTYPE).
fn stack(count: u32, types: u32, args: u32) -> Result<Vec<u32>, i32> {
    let mut out = Vec::new();
    for i in 0..count {
        // SAFETY : tableau x86 de `count` VARTYPE (mémoire du processus).
        let vt = unsafe { ((types + 2 * i) as usize as *const u16).read_unaligned() };
        let var = rd(args + 4 * i);
        if var == 0 {
            return Err(E_INVALIDARG);
        }
        match words(vt) {
            1 => out.push(rd(var + 8)),
            2 => out.extend([rd(var + 8), rd(var + 12)]),
            // Les 16 octets du VARIANT (VT_VARIANT) ou du DECIMAL, qui
            // occupe tout le VARIANT.
            _ => out.extend([rd(var), rd(var + 4), rd(var + 8), rd(var + 12)]),
        }
    }
    Ok(out)
}

/// Écrit dans le VARIANT x86 `result` la valeur rendue `raw` (edx:eax).
fn put_result(result: u32, vt: u16, raw: u64) {
    if result == 0 {
        return;
    }
    for k in 0..4 {
        wr(result + 4 * k, 0);
    }
    if matches!(vt, VT_EMPTY | VT_VOID) {
        return;
    }
    wr(result, vt as u32);
    wr(result + 8, raw as u32);
    if words(vt) == 2 {
        wr(result + 12, (raw >> 32) as u32);
    }
}

/// DispCallFunc d'un programme x86 ; arguments tels que reçus de la pile
/// x86 (adresses de 32 bits).
#[allow(clippy::too_many_arguments)]
pub(crate) fn disp_call_func32(
    instance: u32,
    ovft: u32,
    cc: u32,
    vt_return: u16,
    count: u32,
    types: u32,
    args: u32,
    result: u32,
) -> i32 {
    if count > 0 && (types == 0 || args == 0) {
        return E_INVALIDARG;
    }
    if !matches!(cc, CC_CDECL | CC_STDCALL) {
        crate::log_warn!("wow32", "DispCallFunc x86 : convention {cc} non servie");
        return DISP_E_BADCALLEE;
    }
    if vt_return & VT_BYREF == 0
        && matches!(vt_return, VT_R4 | VT_R8 | VT_DATE | VT_VARIANT | VT_DECIMAL)
    {
        crate::log_warn!(
            "wow32",
            "DispCallFunc x86 : valeur rendue de type {vt_return} non servie (ST0 ou adresse cachée)"
        );
        return DISP_E_BADVARTYPE;
    }
    let target = if instance != 0 {
        if !ovft.is_multiple_of(4) {
            return E_INVALIDARG;
        }
        rd(rd(instance) + ovft)
    } else {
        ovft
    };
    if target == 0 || target as usize >= super::limit() {
        return E_INVALIDARG;
    }
    let mut a = Vec::with_capacity(count as usize + 1);
    if instance != 0 {
        a.push(instance);
    }
    match stack(count, types, args) {
        Ok(s) => a.extend(s),
        Err(e) => return e,
    }
    let raw = call32(target as usize, &a);
    put_result(result, vt_return, raw);
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Page sous 4 Go pour des données x86 de test.
    fn page() -> u32 {
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_32BIT,
                -1,
                0,
            )
        };
        assert_ne!(p, libc::MAP_FAILED);
        p as usize as u32
    }

    #[test]
    fn pile_x86_des_arguments_de_dispcallfunc_2800() {
        let m = page();
        // Types : VT_DISPATCH, VT_VARIANT|VT_BYREF, VT_I8, VT_VARIANT.
        let types = m;
        wr(types, 9 | ((VT_VARIANT | VT_BYREF) as u32) << 16);
        wr(types + 4, VT_I8 as u32 | (VT_VARIANT as u32) << 16);
        // Quatre VARIANT x86 de 16 octets à m + 0x100…
        let v = |i: u32| m + 0x100 + 16 * i;
        wr(v(0), 9);
        wr(v(0) + 8, 0x1234_5678);
        wr(v(1), (VT_VARIANT | VT_BYREF) as u32);
        wr(v(1) + 8, 0x0bad_cafe);
        wr(v(2), VT_I8 as u32);
        wr(v(2) + 8, 0x1111_1111);
        wr(v(2) + 12, 0x2222_2222);
        wr(v(3), 8);
        wr(v(3) + 4, 0xaaaa);
        wr(v(3) + 8, 0xbbbb);
        wr(v(3) + 12, 0xcccc);
        let args = m + 0x80;
        for i in 0..4 {
            wr(args + 4 * i, v(i));
        }
        let s = stack(4, types, args).unwrap();
        assert_eq!(
            s,
            vec![
                0x1234_5678,
                0x0bad_cafe,
                0x1111_1111,
                0x2222_2222,
                8,
                0xaaaa,
                0xbbbb,
                0xcccc
            ]
        );
        wr(args + 4, 0);
        assert_eq!(stack(4, types, args), Err(E_INVALIDARG));

        // Valeur rendue : HRESULT (VT_ERROR, 10) puis VT_I8 sur edx:eax.
        let r = m + 0x200;
        put_result(r, 10, 0x8000_4005);
        assert_eq!((rd(r), rd(r + 8)), (10, 0x8000_4005));
        put_result(r, VT_I8, 0x0000_0007_0000_0003);
        assert_eq!((rd(r), rd(r + 8), rd(r + 12)), (VT_I8 as u32, 3, 7));
        put_result(r, VT_VOID, 99);
        assert_eq!((rd(r), rd(r + 8)), (0, 0));

        // Refus avant tout appel.
        assert_eq!(
            disp_call_func32(0, 0x1000, 2, 0, 0, 0, 0, 0),
            DISP_E_BADCALLEE
        );
        assert_eq!(
            disp_call_func32(0, 0x1000, CC_STDCALL, VT_R8, 0, 0, 0, 0),
            DISP_E_BADVARTYPE
        );
        unsafe { libc::munmap(m as usize as *mut libc::c_void, 4096) };
    }
}
