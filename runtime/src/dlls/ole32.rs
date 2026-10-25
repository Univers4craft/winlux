use crate::dlls::kernel32::{
    create_global_memory, free_global_memory, lookup_global_memory, GlobalMemory,
};
use std::cell::Cell;
use std::ffi::c_void;
use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000eu32 as i32;
const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
const RPC_E_CHANGED_MODE: i32 = 0x8001_0106u32 as i32;
const CO_E_CLASSSTRING: i32 = 0x8004_01f3u32 as i32;
const STG_E_INVALIDFUNCTION: i32 = 0x8003_0001u32 as i32;
const STG_E_INVALIDPOINTER: i32 = 0x8003_0009u32 as i32;

thread_local! {
    static COM_APARTMENT: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

const IID_IUNKNOWN: Guid = Guid {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
const IID_ISEQUENTIAL_STREAM: Guid = Guid {
    data1: 0x0c73_3a30,
    data2: 0x2a1c,
    data3: 0x11ce,
    data4: [0xad, 0xe5, 0, 0xaa, 0, 0x44, 0x77, 0x3d],
};
const IID_ISTREAM: Guid = Guid {
    data1: 0x0000_000c,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StatStg {
    name: *mut u16,
    kind: u32,
    size: u64,
    modified: FileTime,
    created: FileTime,
    accessed: FileTime,
    mode: u32,
    locks_supported: u32,
    clsid: Guid,
    state_bits: u32,
    reserved: u32,
}

#[repr(C)]
struct IStreamVTable {
    query_interface: unsafe extern "win64" fn(*mut ComStream, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut ComStream) -> u32,
    release: unsafe extern "win64" fn(*mut ComStream) -> u32,
    read: unsafe extern "win64" fn(*mut ComStream, *mut c_void, u32, *mut u32) -> i32,
    write: unsafe extern "win64" fn(*mut ComStream, *const c_void, u32, *mut u32) -> i32,
    seek: unsafe extern "win64" fn(*mut ComStream, i64, u32, *mut u64) -> i32,
    set_size: unsafe extern "win64" fn(*mut ComStream, u64) -> i32,
    copy_to: unsafe extern "win64" fn(*mut ComStream, *mut c_void, u64, *mut u64, *mut u64) -> i32,
    commit: unsafe extern "win64" fn(*mut ComStream, u32) -> i32,
    revert: unsafe extern "win64" fn(*mut ComStream) -> i32,
    lock_region: unsafe extern "win64" fn(*mut ComStream, u64, u64, u32) -> i32,
    unlock_region: unsafe extern "win64" fn(*mut ComStream, u64, u64, u32) -> i32,
    stat: unsafe extern "win64" fn(*mut ComStream, *mut StatStg, u32) -> i32,
    clone_stream: unsafe extern "win64" fn(*mut ComStream, *mut *mut c_void) -> i32,
}

struct SharedStream {
    global: usize,
    memory: Arc<Mutex<GlobalMemory>>,
    delete_on_release: bool,
}

impl Drop for SharedStream {
    fn drop(&mut self) {
        if self.delete_on_release {
            free_global_memory(self.global);
        }
    }
}

#[repr(C)]
struct ComStream {
    vtable: *const IStreamVTable,
    refs: AtomicU32,
    shared: Arc<SharedStream>,
    cursor: Mutex<u64>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn guid_text(guid: &Guid) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    )
}

fn parse_hex_u32(value: &str) -> Option<u32> {
    u32::from_str_radix(value, 16).ok()
}
fn parse_hex_u16(value: &str) -> Option<u16> {
    u16::from_str_radix(value, 16).ok()
}
fn parse_hex_u8(value: &str) -> Option<u8> {
    u8::from_str_radix(value, 16).ok()
}

fn parse_guid(value: &str) -> Option<Guid> {
    let value = value
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .unwrap_or(value);
    let parts: Vec<_> = value.split('-').collect();
    if parts.len() != 5
        || parts[0].len() != 8
        || parts[1].len() != 4
        || parts[2].len() != 4
        || parts[3].len() != 4
        || parts[4].len() != 12
    {
        return None;
    }
    let mut data4 = [0u8; 8];
    data4[0] = parse_hex_u8(&parts[3][0..2])?;
    data4[1] = parse_hex_u8(&parts[3][2..4])?;
    for (index, byte) in data4[2..].iter_mut().enumerate() {
        *byte = parse_hex_u8(&parts[4][index * 2..index * 2 + 2])?;
    }
    Some(Guid {
        data1: parse_hex_u32(parts[0])?,
        data2: parse_hex_u16(parts[1])?,
        data3: parse_hex_u16(parts[2])?,
        data4,
    })
}

unsafe fn read_wide_string(value: *const u16) -> Option<String> {
    if value.is_null() {
        return Some(String::new());
    }
    let mut len = 0usize;
    while len <= 64 && *value.add(len) != 0 {
        len += 1;
    }
    if len > 64 {
        return None;
    }
    String::from_utf16(std::slice::from_raw_parts(value, len)).ok()
}

pub extern "win64" fn co_initialize_ex(reserved: usize, coinit: u32) -> i32 {
    if reserved != 0 {
        return E_INVALIDARG;
    }
    let model = coinit & 2;
    COM_APARTMENT.with(|apartment| match apartment.get() {
        None => {
            apartment.set(Some((model, 1)));
            S_OK
        }
        Some((current, count)) if current == model => {
            apartment.set(Some((current, count.saturating_add(1))));
            S_FALSE
        }
        Some(_) => RPC_E_CHANGED_MODE,
    })
}

pub extern "win64" fn co_initialize(reserved: usize) -> i32 {
    co_initialize_ex(reserved, 2)
}
pub extern "win64" fn ole_initialize(reserved: usize) -> i32 {
    co_initialize_ex(reserved, 2)
}

pub extern "win64" fn co_uninitialize() {
    COM_APARTMENT.with(|apartment| {
        if let Some((model, count)) = apartment.get() {
            apartment.set(if count > 1 {
                Some((model, count - 1))
            } else {
                None
            });
        }
    });
}

pub extern "win64" fn ole_uninitialize() {
    co_uninitialize();
}

pub unsafe extern "win64" fn co_task_mem_alloc(size: usize) -> *mut c_void {
    libc::malloc(size.max(1))
}
pub unsafe extern "win64" fn co_task_mem_realloc(memory: *mut c_void, size: usize) -> *mut c_void {
    if memory.is_null() {
        co_task_mem_alloc(size)
    } else {
        libc::realloc(memory, size.max(1))
    }
}
pub unsafe extern "win64" fn co_task_mem_free(memory: *mut c_void) {
    libc::free(memory)
}

pub unsafe extern "win64" fn co_create_guid(out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut bytes = [0u8; 16];
    let Ok(mut random) = std::fs::File::open("/dev/urandom") else {
        return E_OUTOFMEMORY;
    };
    if random.read_exact(&mut bytes).is_err() {
        return E_OUTOFMEMORY;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    *out = Guid {
        data1: u32::from_be_bytes(bytes[0..4].try_into().unwrap()),
        data2: u16::from_be_bytes(bytes[4..6].try_into().unwrap()),
        data3: u16::from_be_bytes(bytes[6..8].try_into().unwrap()),
        data4: bytes[8..16].try_into().unwrap(),
    };
    S_OK
}

pub unsafe extern "win64" fn clsid_from_string(value: *const u16, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if value.is_null() {
        *out = Guid::default();
        return S_OK;
    }
    let Some(value) = read_wide_string(value) else {
        return CO_E_CLASSSTRING;
    };
    let Some(guid) = parse_guid(&value) else {
        return CO_E_CLASSSTRING;
    };
    *out = guid;
    S_OK
}

pub unsafe extern "win64" fn string_from_guid2(
    guid: *const Guid,
    out: *mut u16,
    capacity: i32,
) -> i32 {
    if guid.is_null() || out.is_null() || capacity <= 0 {
        return 0;
    }
    let text: Vec<u16> = guid_text(&*guid)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    if text.len() > capacity as usize {
        return 0;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len());
    text.len() as i32
}

pub unsafe extern "win64" fn string_from_clsid(guid: *const Guid, out: *mut *mut u16) -> i32 {
    if guid.is_null() || out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let text: Vec<u16> = guid_text(&*guid)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let memory = co_task_mem_alloc(text.len() * std::mem::size_of::<u16>()) as *mut u16;
    if memory.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(text.as_ptr(), memory, text.len());
    *out = memory;
    S_OK
}

unsafe extern "win64" fn stream_query_interface(
    this: *mut ComStream,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if this.is_null() || iid.is_null() {
        return E_NOINTERFACE;
    }
    let iid = std::ptr::read_unaligned(iid);
    if iid != IID_IUNKNOWN && iid != IID_ISEQUENTIAL_STREAM && iid != IID_ISTREAM {
        return E_NOINTERFACE;
    }
    stream_add_ref(this);
    *out = this.cast();
    S_OK
}

unsafe extern "win64" fn stream_add_ref(this: *mut ComStream) -> u32 {
    (*this)
        .refs
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1)
}

unsafe extern "win64" fn stream_release(this: *mut ComStream) -> u32 {
    let previous = (*this).refs.fetch_sub(1, Ordering::Release);
    let remaining = previous.saturating_sub(1);
    if remaining == 0 {
        std::sync::atomic::fence(Ordering::Acquire);
        drop(Box::from_raw(this));
    }
    remaining
}

unsafe extern "win64" fn stream_read(
    this: *mut ComStream,
    out: *mut c_void,
    count: u32,
    read: *mut u32,
) -> i32 {
    if !read.is_null() {
        *read = 0;
    }
    if this.is_null() || (out.is_null() && count != 0) {
        return STG_E_INVALIDPOINTER;
    }
    let this = &*this;
    let mut cursor = lock(&this.cursor);
    let memory = lock(&this.shared.memory);
    let start = (*cursor).min(usize::MAX as u64) as usize;
    let available = memory.bytes.len().saturating_sub(start);
    let done = available.min(count as usize);
    if done != 0 {
        std::ptr::copy_nonoverlapping(memory.bytes.as_ptr().add(start), out.cast(), done);
    }
    *cursor = cursor.saturating_add(done as u64);
    if !read.is_null() {
        *read = done as u32;
    }
    if done == count as usize {
        S_OK
    } else {
        S_FALSE
    }
}

unsafe extern "win64" fn stream_write(
    this: *mut ComStream,
    input: *const c_void,
    count: u32,
    written: *mut u32,
) -> i32 {
    if !written.is_null() {
        *written = 0;
    }
    if this.is_null() || (input.is_null() && count != 0) {
        return STG_E_INVALIDPOINTER;
    }
    let this = &*this;
    let mut cursor = lock(&this.cursor);
    let Ok(start) = usize::try_from(*cursor) else {
        return E_OUTOFMEMORY;
    };
    let Some(end) = start.checked_add(count as usize) else {
        return E_OUTOFMEMORY;
    };
    let mut memory = lock(&this.shared.memory);
    if end > memory.bytes.len() {
        let extra = end - memory.bytes.len();
        if memory.bytes.try_reserve_exact(extra).is_err() {
            return E_OUTOFMEMORY;
        }
        memory.bytes.resize(end, 0);
    }
    if count != 0 {
        std::ptr::copy_nonoverlapping(
            input.cast(),
            memory.bytes.as_mut_ptr().add(start),
            count as usize,
        );
    }
    *cursor = end as u64;
    if !written.is_null() {
        *written = count;
    }
    S_OK
}

unsafe extern "win64" fn stream_seek(
    this: *mut ComStream,
    movement: i64,
    origin: u32,
    position: *mut u64,
) -> i32 {
    if this.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let this = &*this;
    let mut cursor = lock(&this.cursor);
    let base = match origin {
        0 => 0i128,
        1 => *cursor as i128,
        2 => lock(&this.shared.memory).bytes.len() as i128,
        _ => return STG_E_INVALIDFUNCTION,
    };
    let next = base + movement as i128;
    if next < 0 || next > u64::MAX as i128 {
        return STG_E_INVALIDFUNCTION;
    }
    *cursor = next as u64;
    if !position.is_null() {
        *position = *cursor;
    }
    S_OK
}

unsafe extern "win64" fn stream_set_size(this: *mut ComStream, size: u64) -> i32 {
    if this.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let this = &*this;
    let Ok(size) = usize::try_from(size) else {
        return E_OUTOFMEMORY;
    };
    let mut memory = lock(&this.shared.memory);
    let old_size = memory.bytes.len();
    if size > old_size && memory.bytes.try_reserve_exact(size - old_size).is_err() {
        return E_OUTOFMEMORY;
    }
    memory.bytes.resize(size, 0);
    S_OK
}

unsafe extern "win64" fn stream_copy_to(
    this: *mut ComStream,
    destination: *mut c_void,
    count: u64,
    read: *mut u64,
    written: *mut u64,
) -> i32 {
    if !read.is_null() {
        *read = 0;
    }
    if !written.is_null() {
        *written = 0;
    }
    if this.is_null() || destination.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let destination_vtable = *(destination as *mut *const IStreamVTable);
    if destination_vtable.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let mut total_read = 0u64;
    let mut total_written = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    while total_read < count {
        let wanted = (count - total_read).min(buffer.len() as u64) as u32;
        let mut got = 0u32;
        let result = stream_read(this, buffer.as_mut_ptr().cast(), wanted, &mut got);
        total_read += got as u64;
        if got == 0 {
            break;
        }
        let mut done = 0u32;
        let result_write = ((*destination_vtable).write)(
            destination.cast(),
            buffer.as_ptr().cast(),
            got,
            &mut done,
        );
        total_written += done as u64;
        if result_write < 0 {
            if !read.is_null() {
                *read = total_read;
            }
            if !written.is_null() {
                *written = total_written;
            }
            return result_write;
        }
        if done != got || result == S_FALSE {
            break;
        }
    }
    if !read.is_null() {
        *read = total_read;
    }
    if !written.is_null() {
        *written = total_written;
    }
    S_OK
}

unsafe extern "win64" fn stream_commit(_this: *mut ComStream, _flags: u32) -> i32 {
    S_OK
}
unsafe extern "win64" fn stream_revert(_this: *mut ComStream) -> i32 {
    E_NOTIMPL
}
unsafe extern "win64" fn stream_lock_region(
    _this: *mut ComStream,
    _offset: u64,
    _count: u64,
    _kind: u32,
) -> i32 {
    STG_E_INVALIDFUNCTION
}
unsafe extern "win64" fn stream_unlock_region(
    _this: *mut ComStream,
    _offset: u64,
    _count: u64,
    _kind: u32,
) -> i32 {
    STG_E_INVALIDFUNCTION
}

unsafe extern "win64" fn stream_stat(this: *mut ComStream, stat: *mut StatStg, _flags: u32) -> i32 {
    if this.is_null() || stat.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let this = &*this;
    *stat = StatStg {
        kind: 2,
        size: lock(&this.shared.memory).bytes.len() as u64,
        mode: 2,
        ..StatStg::default()
    };
    S_OK
}

fn new_stream(shared: Arc<SharedStream>, cursor: u64) -> *mut ComStream {
    Box::into_raw(Box::new(ComStream {
        vtable: &STREAM_VTABLE,
        refs: AtomicU32::new(1),
        shared,
        cursor: Mutex::new(cursor),
    }))
}

unsafe extern "win64" fn stream_clone(this: *mut ComStream, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if this.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let this = &*this;
    let cursor = *lock(&this.cursor);
    *out = new_stream(Arc::clone(&this.shared), cursor).cast();
    S_OK
}

static STREAM_VTABLE: IStreamVTable = IStreamVTable {
    query_interface: stream_query_interface,
    add_ref: stream_add_ref,
    release: stream_release,
    read: stream_read,
    write: stream_write,
    seek: stream_seek,
    set_size: stream_set_size,
    copy_to: stream_copy_to,
    commit: stream_commit,
    revert: stream_revert,
    lock_region: stream_lock_region,
    unlock_region: stream_unlock_region,
    stat: stream_stat,
    clone_stream: stream_clone,
};

pub unsafe extern "win64" fn create_stream_on_hglobal(
    global: usize,
    delete_on_release: i32,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let (global, memory) = if global == 0 {
        let Some(created) = create_global_memory(0, 2) else {
            return E_OUTOFMEMORY;
        };
        created
    } else {
        let Some(memory) = lookup_global_memory(global) else {
            return E_INVALIDARG;
        };
        (global, memory)
    };
    let shared = Arc::new(SharedStream {
        global,
        memory,
        delete_on_release: delete_on_release != 0,
    });
    *out = new_stream(shared, 0).cast();
    S_OK
}

#[allow(dangerous_implicit_autorefs)]
pub unsafe extern "win64" fn get_hglobal_from_stream(stream: *mut c_void, out: *mut usize) -> i32 {
    if stream.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let stream = stream.cast::<ComStream>();
    if (*stream).vtable != &STREAM_VTABLE {
        return E_INVALIDARG;
    }
    *out = (*stream).shared.global;
    S_OK
}

pub unsafe extern "win64" fn co_create_instance(
    _rclsid: *const c_void,
    _outer: usize,
    _ctx: u32,
    _riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CoInitialize"=>fn co_initialize,"CoInitializeEx"=>fn co_initialize_ex,"CoUninitialize"=>fn co_uninitialize,
        "OleInitialize"=>fn ole_initialize,"OleUninitialize"=>fn ole_uninitialize,
        "CoTaskMemAlloc"=>fn co_task_mem_alloc,"CoTaskMemRealloc"=>fn co_task_mem_realloc,"CoTaskMemFree"=>fn co_task_mem_free,
        "CoCreateGuid"=>fn co_create_guid,"CLSIDFromString"=>fn clsid_from_string,"IIDFromString"=>fn clsid_from_string,
        "StringFromGUID2"=>fn string_from_guid2,"StringFromCLSID"=>fn string_from_clsid,"StringFromIID"=>fn string_from_clsid,
        "CreateStreamOnHGlobal"=>fn create_stream_on_hglobal,"GetHGlobalFromStream"=>fn get_hglobal_from_stream,
        "CoCreateInstance"=>fn co_create_instance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_round_trip_and_uuid_version() {
        let original = parse_guid("{00112233-4455-6677-8899-AABBCCDDEEFF}").unwrap();
        assert_eq!(
            guid_text(&original),
            "{00112233-4455-6677-8899-AABBCCDDEEFF}"
        );
        let mut generated = Guid::default();
        assert_eq!(unsafe { co_create_guid(&mut generated) }, S_OK);
        assert_eq!(generated.data3 >> 12, 4);
        assert_eq!(generated.data4[0] >> 6, 2);
    }

    #[test]
    fn apartment_rejects_a_model_change_on_the_same_thread() {
        assert_eq!(co_initialize_ex(0, 0), S_OK);
        assert_eq!(co_initialize_ex(0, 0), S_FALSE);
        assert_eq!(co_initialize_ex(0, 2), RPC_E_CHANGED_MODE);
        co_uninitialize();
        co_uninitialize();
        assert_eq!(co_initialize_ex(0, 2), S_OK);
        co_uninitialize();
    }

    #[test]
    fn memory_stream_shares_its_hglobal_and_resizes() {
        let mut raw = std::ptr::null_mut();
        assert_eq!(unsafe { create_stream_on_hglobal(0, 1, &mut raw) }, S_OK);
        let stream = raw.cast::<ComStream>();
        let input = *b"Winlux";
        let mut written = 0;
        assert_eq!(
            unsafe {
                stream_write(
                    stream,
                    input.as_ptr().cast(),
                    input.len() as u32,
                    &mut written,
                )
            },
            S_OK
        );
        assert_eq!(written, input.len() as u32);
        assert_eq!(
            unsafe { stream_seek(stream, 0, 0, std::ptr::null_mut()) },
            S_OK
        );
        let mut output = [0u8; 6];
        let mut read = 0;
        assert_eq!(
            unsafe {
                stream_read(
                    stream,
                    output.as_mut_ptr().cast(),
                    output.len() as u32,
                    &mut read,
                )
            },
            S_OK
        );
        assert_eq!(output, input);
        let mut global = 0;
        assert_eq!(unsafe { get_hglobal_from_stream(raw, &mut global) }, S_OK);
        assert_eq!(crate::dlls::kernel32::global_size(global), input.len());
        assert_eq!(unsafe { stream_release(stream) }, 0);
        assert!(lookup_global_memory(global).is_none());
    }
}
