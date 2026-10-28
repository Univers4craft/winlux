//! UUID generation used by native Windows libraries. RPC transport and service
//! management are not implemented by this module.
use super::ole32::{co_create_guid, Guid};

pub unsafe extern "win64" fn uuid_create(out: *mut Guid) -> u32 {
    if out.is_null() {
        return 87; // RPC_S_INVALID_ARG / ERROR_INVALID_PARAMETER
    }
    // CoCreateGuid already supplies cryptographically random RFC 4122 v4 GUIDs
    // in the Windows GUID memory layout. RPC returns status codes, not HRESULT.
    if co_create_guid(out) == 0 {
        0
    } else {
        14
    }
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! { "UuidCreate" => fn uuid_create }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_is_random_v4_in_windows_layout() {
        let mut first = Guid::default();
        let mut second = Guid::default();
        unsafe {
            assert_eq!(uuid_create(&mut first), 0);
            assert_eq!(uuid_create(&mut second), 0);
            assert_eq!(uuid_create(std::ptr::null_mut()), 87);
        }
        assert_eq!(first.data3 >> 12, 4);
        assert_eq!(first.data4[0] >> 6, 2);
        assert_ne!(first, second);
    }
}
