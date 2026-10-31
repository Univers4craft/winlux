//! Stubs pour propsys.dll — système de propriétés Windows.
#![allow(non_snake_case)]

const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;

pub unsafe extern "win64" fn PSGetPropertyKeyFromName(
    _name: *const u16, _key: usize,
) -> i32 { E_NOTIMPL }
pub unsafe extern "win64" fn VariantToStringAlloc(_var: usize, _buf: *mut usize) -> i32 { E_NOTIMPL }
pub unsafe extern "win64" fn InitPropVariantFromStringVector(
    _strs: usize, _count: u32, _propvar: usize,
) -> i32 { E_NOTIMPL }

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "PSGetPropertyKeyFromName" => fn PSGetPropertyKeyFromName,
        "VariantToStringAlloc" => fn VariantToStringAlloc,
        "InitPropVariantFromStringVector" => fn InitPropVariantFromStringVector,
    }
}
