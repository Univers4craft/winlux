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
pub unsafe extern "win64" fn query_act_ctx(
    _flags: u32,
    _ctx: usize,
    _sub: usize,
    _class: u32,
    _buf: usize,
    _len: usize,
    needed: *mut usize,
) -> i32 {
    if !needed.is_null() {
        *needed = 0;
    }
    crate::process::set_last_error(50);
    0
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
}
