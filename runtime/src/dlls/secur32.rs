//! secur32.dll (et sspicli.dll) : nom complet de l'utilisateur et interface
//! SSPI (Kerberos, NTLM, Schannel).
//!
//! GetUserNameEx répond d'après le compte Linux. Les paquets de sécurité
//! SSPI ne sont pas encore fournis : EnumerateSecurityPackages en annonce
//! zéro et les autres appels répondent SEC_E_SECPKG_NOT_FOUND, ce que les
//! logiciels traitent comme « authentification intégrée indisponible »
//! (PuTTY retombe alors sur le mot de passe ou la clé).

use crate::process::set_last_error;

const SEC_E_OK: i32 = 0;
const SEC_E_INVALID_HANDLE: i32 = 0x8009_0301u32 as i32;
const SEC_E_SECPKG_NOT_FOUND: i32 = 0x8009_0305u32 as i32;
const ERROR_NONE_MAPPED: u32 = 1332;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_CANT_ACCESS_DOMAIN_INFO: u32 = 1351;

/// Nom affiché du compte (champ GECOS de /etc/passwd, sinon l'identifiant).
fn display_name() -> String {
    let user = crate::paths::user_name();
    let c = std::ffi::CString::new(user.clone()).unwrap_or_default();
    // SAFETY: getpwnam renvoie une entrée statique ou NULL.
    let gecos = unsafe {
        let pw = libc::getpwnam(c.as_ptr());
        if pw.is_null() || (*pw).pw_gecos.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr((*pw).pw_gecos)
                .to_string_lossy()
                .into_owned()
        }
    };
    let full = gecos.split(',').next().unwrap_or("").trim().to_string();
    if full.is_empty() {
        user
    } else {
        full
    }
}

fn computer() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: tampon local de taille connue.
    let host = unsafe {
        if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) == 0 {
            std::ffi::CStr::from_ptr(buf.as_ptr() as *const libc::c_char)
                .to_string_lossy()
                .into_owned()
        } else {
            "WINLUX".into()
        }
    };
    host.split('.')
        .next()
        .unwrap_or("WINLUX")
        .to_ascii_uppercase()
}

/// EXTENDED_NAME_FORMAT : NameSamCompatible (2), NameDisplay (3) ; les noms
/// d'annuaire (domaine Active Directory) n'existent pas sur un poste local.
fn name_for(format: u32) -> Result<String, u32> {
    match format {
        2 => Ok(format!("{}\\{}", computer(), crate::paths::user_name())),
        3 => Ok(display_name()),
        1 | 4 | 6 | 7 | 8 | 9 | 10 | 11 | 12 => Err(ERROR_NONE_MAPPED),
        _ => Err(87),
    }
}

pub unsafe extern "win64" fn get_user_name_ex_w(format: u32, buf: *mut u16, size: *mut u32) -> i32 {
    if size.is_null() {
        set_last_error(87);
        return 0;
    }
    let name = match name_for(format) {
        Ok(n) => n,
        Err(e) => {
            set_last_error(e);
            return 0;
        }
    };
    let w: Vec<u16> = name.encode_utf16().collect();
    if buf.is_null() || (*size as usize) <= w.len() {
        *size = w.len() as u32 + 1;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
    *buf.add(w.len()) = 0;
    *size = w.len() as u32;
    1
}

pub unsafe extern "win64" fn get_user_name_ex_a(format: u32, buf: *mut u8, size: *mut u32) -> i32 {
    if size.is_null() {
        set_last_error(87);
        return 0;
    }
    let name = match name_for(format) {
        Ok(n) => n,
        Err(e) => {
            set_last_error(e);
            return 0;
        }
    };
    let b = crate::dlls::encode_ansi(&name);
    if buf.is_null() || (*size as usize) <= b.len() {
        *size = b.len() as u32 + 1;
        set_last_error(ERROR_MORE_DATA);
        return 0;
    }
    std::ptr::copy_nonoverlapping(b.as_ptr(), buf, b.len());
    *buf.add(b.len()) = 0;
    *size = b.len() as u32;
    1
}

pub extern "win64" fn get_computer_object_name(
    _format: u32,
    _buf: *mut u8,
    _size: *mut u32,
) -> i32 {
    set_last_error(ERROR_CANT_ACCESS_DOMAIN_INFO);
    0
}

/// EnumerateSecurityPackages : aucun paquet (tableau nul, à ne pas libérer).
pub unsafe extern "win64" fn enumerate_security_packages(count: *mut u32, info: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !info.is_null() {
        *info = 0;
    }
    SEC_E_OK
}

pub extern "win64" fn package_not_found() -> i32 {
    SEC_E_SECPKG_NOT_FOUND
}

pub extern "win64" fn invalid_handle() -> i32 {
    SEC_E_INVALID_HANDLE
}

/// FreeContextBuffer : rien n'a été alloué par Winlux ; un pointeur nul est accepté.
pub extern "win64" fn free_context_buffer(_buffer: *mut u8) -> i32 {
    SEC_E_OK
}

/// InitSecurityInterface : pas de table de fonctions SSPI.
pub extern "win64" fn init_security_interface() -> usize {
    0
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetUserNameExA" => fn get_user_name_ex_a,
        "GetUserNameExW" => fn get_user_name_ex_w,
        "GetComputerObjectNameA" => fn get_computer_object_name,
        "GetComputerObjectNameW" => fn get_computer_object_name,
        "EnumerateSecurityPackagesA" => fn enumerate_security_packages,
        "EnumerateSecurityPackagesW" => fn enumerate_security_packages,
        "QuerySecurityPackageInfoA" => fn package_not_found,
        "QuerySecurityPackageInfoW" => fn package_not_found,
        "AcquireCredentialsHandleA" => fn package_not_found,
        "AcquireCredentialsHandleW" => fn package_not_found,
        "InitializeSecurityContextA" => fn package_not_found,
        "InitializeSecurityContextW" => fn package_not_found,
        "AcceptSecurityContext" => fn package_not_found,
        "FreeCredentialsHandle" => fn invalid_handle,
        "DeleteSecurityContext" => fn invalid_handle,
        "QueryContextAttributesA" => fn invalid_handle,
        "QueryContextAttributesW" => fn invalid_handle,
        "QueryCredentialsAttributesA" => fn invalid_handle,
        "QueryCredentialsAttributesW" => fn invalid_handle,
        "MakeSignature" => fn invalid_handle,
        "VerifySignature" => fn invalid_handle,
        "EncryptMessage" => fn invalid_handle,
        "DecryptMessage" => fn invalid_handle,
        "CompleteAuthToken" => fn invalid_handle,
        "ImpersonateSecurityContext" => fn invalid_handle,
        "RevertSecurityContext" => fn invalid_handle,
        "FreeContextBuffer" => fn free_context_buffer,
        "InitSecurityInterfaceA" => fn init_security_interface,
        "InitSecurityInterfaceW" => fn init_security_interface,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sam_compatible_name_has_machine_and_user() {
        let mut buf = [0u16; 128];
        let mut size = buf.len() as u32;
        let ok = unsafe { get_user_name_ex_w(2, buf.as_mut_ptr(), &mut size) };
        assert_eq!(ok, 1);
        let name = String::from_utf16_lossy(&buf[..size as usize]);
        assert!(name.contains('\\'), "{name}");
        assert!(name.ends_with(&crate::paths::user_name()), "{name}");
    }

    #[test]
    fn principal_name_is_not_mapped() {
        let mut size = 0u32;
        assert_eq!(
            unsafe { get_user_name_ex_w(8, std::ptr::null_mut(), &mut size) },
            0
        );
    }
}
