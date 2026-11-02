//! secur32.dll (et sspicli.dll) : nom complet de l'utilisateur et interface
//! SSPI (Kerberos, NTLM, Schannel).
//!
//! GetUserNameEx répond d'après le compte Linux. Côté SSPI, Schannel (TLS)
//! est fourni ([`super::schannel`]) ; Kerberos, NTLM et Negotiate répondent
//! SEC_E_SECPKG_NOT_FOUND, ce que les logiciels traitent comme
//! « authentification intégrée indisponible » (PuTTY retombe alors sur le
//! mot de passe ou la clé).

use crate::process::set_last_error;

use super::schannel::{SEC_E_INVALID_HANDLE, SEC_E_OK};
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

// ---------- SSPI ----------
//
// Seul Schannel (TLS) est fourni, par [`super::schannel`]. Kerberos, NTLM et
// Negotiate restent « paquet introuvable », ce que les logiciels traitent
// comme « authentification intégrée indisponible ».

use super::schannel::{self, SecBufferDesc, SecHandle};

const PKG_NAME: &str = "Microsoft Unified Security Protocol Provider";
const PKG_COMMENT: &str = "Schannel Security Package";

/// SecPkgInfoW / SecPkgInfoA alloués d'un bloc (libéré par FreeContextBuffer).
unsafe fn pkg_info(count: usize, wide: bool) -> *mut u8 {
    let enc = |s: &str| -> Vec<u8> {
        if wide {
            s.encode_utf16()
                .chain([0])
                .flat_map(|c| c.to_le_bytes())
                .collect()
        } else {
            s.bytes().chain([0]).collect()
        }
    };
    let names: Vec<(Vec<u8>, Vec<u8>)> = [PKG_NAME, "Schannel"]
        .iter()
        .take(count)
        .map(|n| (enc(n), enc(PKG_COMMENT)))
        .collect();
    let strings: usize = names.iter().map(|(a, b)| a.len() + b.len()).sum();
    let p = libc::calloc(1, 32 * count + strings) as *mut u8;
    if p.is_null() {
        return p;
    }
    let mut at = p.add(32 * count);
    for (i, (n, c)) in names.iter().enumerate() {
        let e = p.add(32 * i);
        // fCapabilities, wVersion, wRPCID (14 : GSS_SCHANNEL), cbMaxToken.
        (e as *mut u32).write_unaligned(0x0001_07b3);
        (e.add(4) as *mut u16).write_unaligned(1);
        (e.add(6) as *mut u16).write_unaligned(14);
        (e.add(8) as *mut u32).write_unaligned(0x6000);
        std::ptr::copy_nonoverlapping(n.as_ptr(), at, n.len());
        (e.add(16) as *mut usize).write_unaligned(at as usize);
        at = at.add(n.len());
        std::ptr::copy_nonoverlapping(c.as_ptr(), at, c.len());
        (e.add(24) as *mut usize).write_unaligned(at as usize);
        at = at.add(c.len());
    }
    p
}

unsafe fn enumerate(count: *mut u32, info: *mut usize, wide: bool) -> i32 {
    let p = pkg_info(2, wide);
    if !count.is_null() {
        *count = if p.is_null() { 0 } else { 2 };
    }
    if !info.is_null() {
        *info = p as usize;
    }
    SEC_E_OK
}

pub unsafe extern "win64" fn enumerate_security_packages_w(
    count: *mut u32,
    info: *mut usize,
) -> i32 {
    enumerate(count, info, true)
}

pub unsafe extern "win64" fn enumerate_security_packages_a(
    count: *mut u32,
    info: *mut usize,
) -> i32 {
    enumerate(count, info, false)
}

pub unsafe extern "win64" fn query_security_package_info_w(
    name: *const u16,
    info: *mut usize,
) -> i32 {
    if !schannel::is_package(&crate::dlls::wstr(name)) || info.is_null() {
        return SEC_E_SECPKG_NOT_FOUND;
    }
    *info = pkg_info(1, true) as usize;
    SEC_E_OK
}

pub unsafe extern "win64" fn query_security_package_info_a(
    name: *const u8,
    info: *mut usize,
) -> i32 {
    if !schannel::is_package(&crate::dlls::cstr(name)) || info.is_null() {
        return SEC_E_SECPKG_NOT_FOUND;
    }
    *info = pkg_info(1, false) as usize;
    SEC_E_OK
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn acquire_credentials_handle_w(
    _principal: *const u16,
    package: *const u16,
    use_: u32,
    _logon: usize,
    auth: *const u8,
    _get_key: usize,
    _arg: usize,
    cred: *mut SecHandle,
    expiry: *mut i64,
) -> i32 {
    if !schannel::is_package(&crate::dlls::wstr(package)) {
        return SEC_E_SECPKG_NOT_FOUND;
    }
    schannel::acquire_credentials(use_, auth, cred, expiry)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn acquire_credentials_handle_a(
    _principal: *const u8,
    package: *const u8,
    use_: u32,
    _logon: usize,
    auth: *const u8,
    _get_key: usize,
    _arg: usize,
    cred: *mut SecHandle,
    expiry: *mut i64,
) -> i32 {
    if !schannel::is_package(&crate::dlls::cstr(package)) {
        return SEC_E_SECPKG_NOT_FOUND;
    }
    schannel::acquire_credentials(use_, auth, cred, expiry)
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn initialize_security_context_w(
    cred: *const SecHandle,
    ctx: *const SecHandle,
    target: *const u16,
    req: u32,
    _reserved1: u32,
    _rep: u32,
    input: *mut SecBufferDesc,
    _reserved2: u32,
    new_ctx: *mut SecHandle,
    output: *mut SecBufferDesc,
    attrs: *mut u32,
    expiry: *mut i64,
) -> i32 {
    if !schannel::is_cred(cred) && !schannel::is_ctx(ctx) {
        return SEC_E_INVALID_HANDLE;
    }
    if !expiry.is_null() {
        *expiry = i64::MAX;
    }
    schannel::initialize(
        cred,
        ctx,
        crate::dlls::wstr(target),
        req,
        input,
        new_ctx,
        output,
        attrs,
    )
}

#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn initialize_security_context_a(
    cred: *const SecHandle,
    ctx: *const SecHandle,
    target: *const u8,
    req: u32,
    _reserved1: u32,
    _rep: u32,
    input: *mut SecBufferDesc,
    _reserved2: u32,
    new_ctx: *mut SecHandle,
    output: *mut SecBufferDesc,
    attrs: *mut u32,
    expiry: *mut i64,
) -> i32 {
    if !schannel::is_cred(cred) && !schannel::is_ctx(ctx) {
        return SEC_E_INVALID_HANDLE;
    }
    if !expiry.is_null() {
        *expiry = i64::MAX;
    }
    schannel::initialize(
        cred,
        ctx,
        crate::dlls::cstr(target),
        req,
        input,
        new_ctx,
        output,
        attrs,
    )
}

pub unsafe extern "win64" fn free_credentials_handle(cred: *const SecHandle) -> i32 {
    schannel::free_credentials(cred)
}

pub unsafe extern "win64" fn delete_security_context(ctx: *const SecHandle) -> i32 {
    schannel::delete(ctx)
}

pub unsafe extern "win64" fn apply_control_token(
    ctx: *const SecHandle,
    input: *mut SecBufferDesc,
) -> i32 {
    schannel::apply_control_token(ctx, input)
}

pub unsafe extern "win64" fn query_context_attributes(
    ctx: *const SecHandle,
    attr: u32,
    out: *mut u8,
) -> i32 {
    schannel::query_attributes(ctx, attr, out)
}

pub unsafe extern "win64" fn encrypt_message(
    ctx: *const SecHandle,
    _qop: u32,
    msg: *mut SecBufferDesc,
    _seq: u32,
) -> i32 {
    schannel::encrypt(ctx, msg)
}

pub unsafe extern "win64" fn decrypt_message(
    ctx: *const SecHandle,
    msg: *mut SecBufferDesc,
    _seq: u32,
    qop: *mut u32,
) -> i32 {
    if !qop.is_null() {
        *qop = 0;
    }
    schannel::decrypt(ctx, msg)
}

pub extern "win64" fn package_not_found() -> i32 {
    SEC_E_SECPKG_NOT_FOUND
}

pub extern "win64" fn invalid_handle() -> i32 {
    SEC_E_INVALID_HANDLE
}

/// FreeContextBuffer : libère ce que Winlux a alloué (malloc) ; nul accepté.
pub unsafe extern "win64" fn free_context_buffer(buffer: *mut u8) -> i32 {
    if !buffer.is_null() {
        libc::free(buffer.cast());
    }
    SEC_E_OK
}

/// Table SecurityFunctionTable (version 4), A ou W.
fn function_table(wide: bool) -> usize {
    use std::sync::OnceLock;
    static W: OnceLock<Box<[usize; 32]>> = OnceLock::new();
    static A: OnceLock<Box<[usize; 32]>> = OnceLock::new();
    let build = |wide: bool| -> Box<[usize; 32]> {
        let mut t = [0usize; 32];
        let f = |x: *const ()| x as usize;
        t[0] = 4;
        t[1] = if wide {
            f(enumerate_security_packages_w as *const ())
        } else {
            f(enumerate_security_packages_a as *const ())
        };
        t[2] = f(invalid_handle as *const ());
        t[3] = if wide {
            f(acquire_credentials_handle_w as *const ())
        } else {
            f(acquire_credentials_handle_a as *const ())
        };
        t[4] = f(free_credentials_handle as *const ());
        t[6] = if wide {
            f(initialize_security_context_w as *const ())
        } else {
            f(initialize_security_context_a as *const ())
        };
        t[7] = f(package_not_found as *const ());
        t[8] = f(invalid_handle as *const ());
        t[9] = f(delete_security_context as *const ());
        t[10] = f(apply_control_token as *const ());
        t[11] = f(query_context_attributes as *const ());
        t[12] = f(invalid_handle as *const ());
        t[13] = f(invalid_handle as *const ());
        t[14] = f(invalid_handle as *const ());
        t[15] = f(invalid_handle as *const ());
        t[16] = f(free_context_buffer as *const ());
        t[17] = if wide {
            f(query_security_package_info_w as *const ())
        } else {
            f(query_security_package_info_a as *const ())
        };
        t[20] = f(invalid_handle as *const ());
        t[21] = f(invalid_handle as *const ());
        t[22] = f(package_not_found as *const ());
        t[24] = f(invalid_handle as *const ());
        t[25] = f(encrypt_message as *const ());
        t[26] = f(decrypt_message as *const ());
        t[27] = f(invalid_handle as *const ());
        t[28] = f(invalid_handle as *const ());
        t[29] = f(package_not_found as *const ());
        t[30] = f(query_context_attributes as *const ());
        t[31] = f(invalid_handle as *const ());
        Box::new(t)
    };
    let t = if wide {
        W.get_or_init(|| build(true))
    } else {
        A.get_or_init(|| build(false))
    };
    t.as_ptr() as usize
}

pub extern "win64" fn init_security_interface_w() -> usize {
    function_table(true)
}

pub extern "win64" fn init_security_interface_a() -> usize {
    function_table(false)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetUserNameExA" => fn get_user_name_ex_a,
        "GetUserNameExW" => fn get_user_name_ex_w,
        "GetComputerObjectNameA" => fn get_computer_object_name,
        "GetComputerObjectNameW" => fn get_computer_object_name,
        "EnumerateSecurityPackagesA" => fn enumerate_security_packages_a,
        "EnumerateSecurityPackagesW" => fn enumerate_security_packages_w,
        "QuerySecurityPackageInfoA" => fn query_security_package_info_a,
        "QuerySecurityPackageInfoW" => fn query_security_package_info_w,
        "AcquireCredentialsHandleA" => fn acquire_credentials_handle_a,
        "AcquireCredentialsHandleW" => fn acquire_credentials_handle_w,
        "InitializeSecurityContextA" => fn initialize_security_context_a,
        "InitializeSecurityContextW" => fn initialize_security_context_w,
        "AcceptSecurityContext" => fn package_not_found,
        "FreeCredentialsHandle" => fn free_credentials_handle,
        "DeleteSecurityContext" => fn delete_security_context,
        "ApplyControlToken" => fn apply_control_token,
        "QueryContextAttributesA" => fn query_context_attributes,
        "QueryContextAttributesW" => fn query_context_attributes,
        "QueryContextAttributesExA" => fn query_context_attributes,
        "QueryContextAttributesExW" => fn query_context_attributes,
        "QueryCredentialsAttributesA" => fn invalid_handle,
        "QueryCredentialsAttributesW" => fn invalid_handle,
        "MakeSignature" => fn invalid_handle,
        "VerifySignature" => fn invalid_handle,
        "EncryptMessage" => fn encrypt_message,
        "DecryptMessage" => fn decrypt_message,
        "CompleteAuthToken" => fn invalid_handle,
        "ImpersonateSecurityContext" => fn invalid_handle,
        "RevertSecurityContext" => fn invalid_handle,
        "FreeContextBuffer" => fn free_context_buffer,
        "InitSecurityInterfaceA" => fn init_security_interface_a,
        "InitSecurityInterfaceW" => fn init_security_interface_w,
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
