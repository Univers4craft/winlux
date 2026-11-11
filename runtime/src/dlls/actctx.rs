//! Contextes d'activation (side-by-side) : WinForms appelle
//! `CreateActCtxW` puis `ActivateActCtx` pour activer les styles visuels de
//! comctl32 v6. Winlux n'a qu'une seule version de chaque DLL : un
//! contexte est un simple jeton, toujours activable.

use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0x00AC_0001);

/// ACTCTXW/A : cbSize en tête. Le contexte renvoyé n'a pas d'effet.
pub unsafe extern "win64" fn create_act_ctx(ctx: *const u32) -> usize {
    if ctx.is_null() || *ctx < 32 {
        crate::process::set_last_error(87);
        return usize::MAX; // INVALID_HANDLE_VALUE
    }
    NEXT.fetch_add(4, Ordering::Relaxed)
}

pub unsafe extern "win64" fn activate_act_ctx(_ctx: usize, cookie: *mut usize) -> i32 {
    if !cookie.is_null() {
        *cookie = NEXT.fetch_add(4, Ordering::Relaxed);
    }
    1
}

pub extern "win64" fn deactivate_act_ctx(_flags: u32, _cookie: usize) -> i32 {
    1
}

pub extern "win64" fn release_act_ctx(_ctx: usize) {}

pub extern "win64" fn add_ref_act_ctx(_ctx: usize) {}

pub extern "win64" fn zombify_act_ctx(_ctx: usize) -> i32 {
    1
}

/// Aucun contexte actif : le contexte par défaut du processus (NULL).
pub unsafe extern "win64" fn get_current_act_ctx(out: *mut usize) -> i32 {
    if out.is_null() {
        crate::process::set_last_error(87);
        return 0;
    }
    *out = 0;
    1
}

/// Pas de section side-by-side : ERROR_SXS_KEY_NOT_FOUND.
pub extern "win64" fn find_act_ctx_section_string(
    _flags: u32,
    _guid: usize,
    _section: u32,
    _name: usize,
    _data: usize,
) -> i32 {
    crate::process::set_last_error(14007);
    0
}

pub extern "win64" fn find_act_ctx_section_guid(
    _flags: u32,
    _ext: usize,
    _section: u32,
    _guid: usize,
    _data: usize,
) -> i32 {
    crate::process::set_last_error(14007);
    0
}

/// QueryActCtxW : aucune information détaillée (ERROR_NOT_SUPPORTED) ;
/// la taille requise vaut 0.
/// QueryActCtxW (#1685) : Winlux n'a pas de manifestes d'assemblages
/// côte à côte, le contexte est vide mais l'interrogation réussit, comme
/// pour un programme sans manifeste sous Windows. MFC interroge le contexte
/// actif avant de créer une boîte de dialogue et l'abandonnait sur l'échec.
/// Classes (winnt.h) : 1 informations de base, 2 détails, 5 niveau
/// d'exécution, 6 compatibilité ; 3 et 4 visent un assemblage du contexte,
/// qui n'en a aucun.
pub unsafe extern "win64" fn query_act_ctx(
    flags: u32,
    ctx: usize,
    _sub: usize,
    class: u32,
    buf: usize,
    len: usize,
    needed: *mut usize,
) -> i32 {
    const QUERY_ACTCTX_FLAG_USE_ACTIVE_ACTCTX: u32 = 0x4;
    let ptr = if crate::wow32::active() { 4 } else { 8 };
    // Contenu à écrire, champ par champ (taille en octets, valeur).
    let handle = if flags & QUERY_ACTCTX_FLAG_USE_ACTIVE_ACTCTX != 0 {
        0
    } else {
        ctx as u64
    };
    let fields: Vec<(usize, u64)> = match class {
        // ACTIVATION_CONTEXT_BASIC_INFORMATION { HANDLE hActCtx; DWORD dwFlags; }
        1 => vec![(ptr, handle), (4, 0)],
        // ACTIVATION_CONTEXT_DETAILED_INFORMATION : neuf DWORD (format 1,
        // aucun assemblage, aucun chemin) puis trois pointeurs nuls.
        2 => {
            let mut f = vec![(4, 0), (4, 1)];
            f.extend(std::iter::repeat_n((4, 0), 7));
            f.extend(std::iter::repeat_n((ptr, 0), 3));
            f
        }
        // ACTIVATION_CONTEXT_RUN_LEVEL_INFORMATION : niveau non précisé.
        5 => vec![(4, 0), (4, 0), (4, 0)],
        // ACTIVATION_CONTEXT_COMPATIBILITY_INFORMATION : aucun élément.
        6 => vec![(4, 0)],
        3 | 4 => {
            crate::process::set_last_error(87); // ERROR_INVALID_PARAMETER
            return 0;
        }
        _ => {
            crate::process::set_last_error(87);
            return 0;
        }
    };
    // Alignement des pointeurs (structure C, champs naturels).
    let mut layout = Vec::with_capacity(fields.len());
    let mut off = 0usize;
    for &(size, value) in &fields {
        off = off.next_multiple_of(size);
        layout.push((off, size, value));
        off += size;
    }
    let total = off.next_multiple_of(if fields.iter().any(|f| f.0 == 8) {
        8
    } else {
        4
    });
    if !needed.is_null() {
        if ptr == 4 {
            (needed as *mut u32).write_unaligned(total as u32);
        } else {
            needed.write_unaligned(total);
        }
    }
    if buf == 0 || len < total {
        crate::process::set_last_error(122); // ERROR_INSUFFICIENT_BUFFER
        return 0;
    }
    std::ptr::write_bytes(buf as *mut u8, 0, total);
    for (o, size, value) in layout {
        let at = (buf + o) as *mut u8;
        if size == 8 {
            (at as *mut u64).write_unaligned(value);
        } else {
            (at as *mut u32).write_unaligned(value as u32);
        }
    }
    crate::process::set_last_error(0);
    1
}

pub fn kernel32_exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateActCtxW" => fn create_act_ctx, "CreateActCtxA" => fn create_act_ctx,
        "ActivateActCtx" => fn activate_act_ctx, "DeactivateActCtx" => fn deactivate_act_ctx,
        "ReleaseActCtx" => fn release_act_ctx, "AddRefActCtx" => fn add_ref_act_ctx,
        "ZombifyActCtx" => fn zombify_act_ctx, "GetCurrentActCtx" => fn get_current_act_ctx,
        "FindActCtxSectionStringW" => fn find_act_ctx_section_string,
        "FindActCtxSectionStringA" => fn find_act_ctx_section_string,
        "FindActCtxSectionGuid" => fn find_act_ctx_section_guid,
        "QueryActCtxW" => fn query_act_ctx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contexte_d_activation_cree_et_active() {
        let ctx = [32u32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let h = unsafe { create_act_ctx(ctx.as_ptr()) };
        assert_ne!(h, usize::MAX);
        let mut cookie = 0;
        assert_eq!(unsafe { activate_act_ctx(h, &mut cookie) }, 1);
        assert_ne!(cookie, 0);
        assert_eq!(deactivate_act_ctx(0, cookie), 1);
        assert_eq!(unsafe { create_act_ctx(std::ptr::null()) }, usize::MAX);
    }

    #[test]
    fn interrogation_du_contexte_actif() {
        // Informations de base du contexte actif : 16 octets en 64 bits.
        let mut buf = [0xAAu8; 32];
        let mut needed = 0usize;
        let ok = unsafe { query_act_ctx(0x4, 0, 0, 1, buf.as_mut_ptr() as usize, 32, &mut needed) };
        assert_eq!(ok, 1);
        assert_eq!(needed, 16);
        assert!(buf[..16].iter().all(|&b| b == 0));
        // Tampon trop petit : taille demandée et ERROR_INSUFFICIENT_BUFFER.
        let ok = unsafe { query_act_ctx(0x4, 0, 0, 2, buf.as_mut_ptr() as usize, 8, &mut needed) };
        assert_eq!(ok, 0);
        assert_eq!(needed, 64);
        // Détails : format 1, aucun assemblage.
        let mut big = [0xAAu8; 64];
        let ok = unsafe { query_act_ctx(0x4, 0, 0, 2, big.as_mut_ptr() as usize, 64, &mut needed) };
        assert_eq!(ok, 1);
        assert_eq!(u32::from_le_bytes(big[4..8].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(big[8..12].try_into().unwrap()), 0);
        // Classe inconnue refusée.
        assert_eq!(
            unsafe { query_act_ctx(0x4, 0, 0, 99, 0, 0, &mut needed) },
            0
        );
    }
}
