//! Stubs pour userenv.dll — profils utilisateur et blocs d'environnement.
#![allow(non_snake_case)]

pub unsafe extern "win64" fn CreateEnvironmentBlock(
    env: *mut usize,
    _token: usize,
    _inherit: i32,
) -> i32 {
    if !env.is_null() {
        *env = 0;
    }
    1
}
pub extern "win64" fn DestroyEnvironmentBlock(_env: usize) -> i32 {
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CreateEnvironmentBlock" => fn CreateEnvironmentBlock,
        "DestroyEnvironmentBlock" => fn DestroyEnvironmentBlock,
    }
}
