use crate::dlls::kernel32::{
    create_global_memory, free_global_memory, lookup_global_memory, GlobalMemory,
};
use std::cell::Cell;
use std::ffi::c_void;
use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

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

use super::lock;

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

// ---------- Class factory registry ----------
struct ClassFactoryEntry {
    cookie: u32,
    clsid: Guid,
    punk: usize,
    context: u32,
}
static CLASS_FACTORIES: std::sync::OnceLock<Mutex<Vec<Arc<ClassFactoryEntry>>>> =
    std::sync::OnceLock::new();
impl Drop for ClassFactoryEntry {
    fn drop(&mut self) {
        unsafe {
            let vtable = *(self.punk as *const *const usize);
            let release: unsafe extern "win64" fn(usize) -> u32 =
                std::mem::transmute(*vtable.add(2));
            release(self.punk);
        }
    }
}
static NEXT_CF_COOKIE: AtomicU32 = AtomicU32::new(1);

pub unsafe extern "win64" fn co_register_class_object(
    clsid: *const Guid,
    punk: usize,
    ctx: u32,
    _flags: u32,
    cookie: *mut u32,
) -> i32 {
    if cookie.is_null() {
        return E_POINTER;
    }
    *cookie = 0;
    if clsid.is_null() || punk == 0 || ctx == 0 {
        return E_INVALIDARG;
    }
    // Seules les fabriques multi-usages sont actuellement prises en charge.
    if _flags != 1 {
        return E_NOTIMPL;
    }
    let guid = *clsid;
    let c = NEXT_CF_COOKIE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let vtable = *(punk as *const *const usize);
    let addref: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vtable.add(1));
    addref(punk);
    CLASS_FACTORIES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(Arc::new(ClassFactoryEntry {
            cookie: c,
            clsid: guid,
            punk,
            context: ctx,
        }));
    *cookie = c;
    S_OK
}

pub unsafe extern "win64" fn co_revoke_class_object(cookie: u32) -> i32 {
    let mut factories = CLASS_FACTORIES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap();
    let Some(index) = factories.iter().position(|entry| entry.cookie == cookie) else {
        return E_INVALIDARG;
    };
    let entry = factories.remove(index);
    // Release peut réentrer dans COM : ne pas garder le verrou du registre.
    drop(factories);
    drop(entry);
    S_OK
}

#[cfg(test)]
mod reviewed_factory_tests {
    use super::*;
    #[repr(C)]
    struct Factory {
        vtable: *const usize,
        refs: AtomicU32,
        cookie: AtomicU32,
    }
    unsafe extern "win64" fn qi(_: usize, _: *const Guid, _: *mut *mut c_void) -> i32 {
        E_NOINTERFACE
    }
    unsafe extern "win64" fn addref(p: usize) -> u32 {
        (*(p as *const Factory)).refs.fetch_add(1, Ordering::SeqCst) + 1
    }
    unsafe extern "win64" fn release(p: usize) -> u32 {
        (*(p as *const Factory)).refs.fetch_sub(1, Ordering::SeqCst) - 1
    }
    unsafe extern "win64" fn create(
        p: usize,
        _: usize,
        _: *const Guid,
        out: *mut *mut c_void,
    ) -> i32 {
        // La fabrique peut se révoquer pendant CreateInstance, sans deadlock.
        let cookie = (*(p as *const Factory)).cookie.load(Ordering::SeqCst);
        assert_eq!(co_revoke_class_object(cookie), S_OK);
        assert_eq!((*(p as *const Factory)).refs.load(Ordering::SeqCst), 2);
        *out = std::ptr::null_mut();
        E_NOINTERFACE
    }
    #[test]
    fn cookies_stay_stable_and_callbacks_can_reenter() {
        unsafe {
            let vt = [
                qi as *const () as usize,
                addref as *const () as usize,
                release as *const () as usize,
                create as *const () as usize,
            ];
            let first = Factory {
                vtable: vt.as_ptr(),
                refs: AtomicU32::new(1),
                cookie: AtomicU32::new(0),
            };
            let second = Factory {
                vtable: vt.as_ptr(),
                refs: AtomicU32::new(1),
                cookie: AtomicU32::new(0),
            };
            let cls1 = Guid {
                data1: 0x776c0001,
                ..Guid::default()
            };
            let cls2 = Guid {
                data1: 0x776c0002,
                ..Guid::default()
            };
            let mut a = 0;
            let mut b = 0;
            assert_eq!(
                co_register_class_object(&cls1, &first as *const _ as usize, 1, 1, &mut a),
                S_OK
            );
            assert_eq!(
                co_register_class_object(&cls2, &second as *const _ as usize, 1, 1, &mut b),
                S_OK
            );
            second.cookie.store(b, Ordering::SeqCst);
            assert_eq!(co_revoke_class_object(a), S_OK);
            assert_eq!(first.refs.load(Ordering::SeqCst), 1);
            let mut out = std::ptr::null_mut();
            assert_eq!(
                co_create_instance(&cls2, 0, 1, &IID_IUNKNOWN, &mut out),
                E_NOINTERFACE
            );
            assert_eq!(second.refs.load(Ordering::SeqCst), 1);
            assert_eq!(co_revoke_class_object(b), E_INVALIDARG);
        }
    }
}

pub unsafe extern "win64" fn co_create_instance(
    rclsid: *const Guid,
    outer: usize,
    ctx: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if rclsid.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    if outer != 0 {
        return 0x8004_0110u32 as i32;
    } // CLASS_E_NOAGGREGATION
    let guid = *rclsid;
    let iid = *riid;
    let factories = CLASS_FACTORIES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap();
    let Some(entry) = factories
        .iter()
        .find(|entry| entry.clsid == guid && (entry.context & ctx) != 0)
    else {
        drop(factories);
        // Classes intégrées au runtime (planificateur de tâches…).
        if let Some(hr) = crate::dlls::taskschd::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Classes audio intégrées (MMDeviceEnumerator, DirectSound, XAudio2 2.7).
        if let Some(hr) = super::audio_com::create_builtin(&guid, riid, out) {
            return hr;
        }
        return 0x8004_0154u32 as i32; // REGDB_E_CLASSNOTREG
    };
    let entry = Arc::clone(entry);
    drop(factories);
    let punk = entry.punk;
    let vtable = *(punk as *const *const usize);
    let create: unsafe extern "win64" fn(usize, usize, *const Guid, *mut *mut c_void) -> i32 =
        std::mem::transmute(*vtable.add(3));
    let result = create(punk, outer, &iid, out);
    drop(entry);
    result
}

// ---------- CoGetClassObject ----------

pub unsafe extern "win64" fn co_get_class_object(
    rclsid: *const Guid,
    ctx: u32,
    _reserved: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    if rclsid.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    let guid = *rclsid;
    let iid = *riid;
    let factories = CLASS_FACTORIES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap();
    let Some(entry) = factories
        .iter()
        .find(|entry| entry.clsid == guid && (entry.context & ctx) != 0)
    else {
        drop(factories);
        if let Some(hr) = super::audio_com::class_object(&guid, riid, out) {
            return hr;
        }
        return REGDB_E_CLASSNOTREG;
    };
    let entry = Arc::clone(entry);
    drop(factories);
    let punk = entry.punk;
    let vtable = *(punk as *const *const usize);
    let qi: unsafe extern "win64" fn(usize, *const Guid, *mut *mut c_void) -> i32 =
        std::mem::transmute(*vtable);
    let result = qi(punk, &iid, out);
    drop(entry);
    result
}

const REGDB_E_CLASSNOTREG: i32 = 0x8004_0154u32 as i32;

// ---------- CoCreateFreeThreadedMarshaler ----------

pub unsafe extern "win64" fn co_create_free_threaded_marshaler(
    _outer: usize,
    ppunk: *mut *mut c_void,
) -> i32 {
    if !ppunk.is_null() {
        *ppunk = std::ptr::null_mut();
    }
    S_OK
}

// ---------- ProgIDFromCLSID / CLSIDFromProgID ----------

pub unsafe extern "win64" fn prog_id_from_clsid(clsid: *const Guid, prog_id: *mut *mut u16) -> i32 {
    if prog_id.is_null() {
        return E_POINTER;
    }
    *prog_id = std::ptr::null_mut();
    if clsid.is_null() {
        return E_INVALIDARG;
    }
    let guid_str = guid_text(&*clsid);
    let reg_path = format!("hkcr\\clsid\\{}\\progid", guid_str.to_lowercase());
    let text = match crate::registry::get_string(&reg_path, "") {
        Some(v) => v,
        None => return REGDB_E_CLASSNOTREG,
    };
    let text = text.as_str();
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let mem = co_task_mem_alloc(wide.len() * 2) as *mut u16;
    if mem.is_null() {
        return E_OUTOFMEMORY;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), mem, wide.len());
    *prog_id = mem;
    S_OK
}

pub unsafe extern "win64" fn clsid_from_prog_id(prog_id: *const u16, clsid: *mut Guid) -> i32 {
    if clsid.is_null() {
        return E_POINTER;
    }
    *clsid = Guid::default();
    if prog_id.is_null() {
        return CO_E_CLASSSTRING;
    }
    let name = match read_wide_string(prog_id) {
        Some(s) => s,
        None => return CO_E_CLASSSTRING,
    };
    let reg_path = format!("hkcr\\{}\\clsid", name.to_lowercase());
    let text = match crate::registry::get_string(&reg_path, "") {
        Some(v) => v,
        None => return CO_E_CLASSSTRING,
    };
    let text = text.as_str();
    match parse_guid(text) {
        Some(g) => {
            *clsid = g;
            S_OK
        }
        None => CO_E_CLASSSTRING,
    }
}

// ---------- CoGetMalloc (IMalloc singleton) ----------

#[repr(C)]
struct IMallocVTable {
    query_interface: unsafe extern "win64" fn(usize, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(usize) -> u32,
    release: unsafe extern "win64" fn(usize) -> u32,
    alloc: unsafe extern "win64" fn(usize, usize) -> *mut c_void,
    realloc: unsafe extern "win64" fn(usize, *mut c_void, usize) -> *mut c_void,
    free: unsafe extern "win64" fn(usize, *mut c_void),
    get_size: unsafe extern "win64" fn(usize, *mut c_void) -> usize,
    did_alloc: unsafe extern "win64" fn(usize, *mut c_void) -> i32,
    heap_minimize: unsafe extern "win64" fn(usize),
}

const IID_IMALLOC: Guid = Guid {
    data1: 0x0000_0002,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};

unsafe extern "win64" fn malloc_qi(this: usize, iid: *const Guid, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::null_mut();
    let iid = std::ptr::read_unaligned(iid);
    if iid == IID_IUNKNOWN || iid == IID_IMALLOC {
        *out = this as *mut c_void;
        S_OK
    } else {
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn malloc_addref(_: usize) -> u32 {
    2
}
unsafe extern "win64" fn malloc_release(_: usize) -> u32 {
    1
}
unsafe extern "win64" fn malloc_alloc(_: usize, size: usize) -> *mut c_void {
    co_task_mem_alloc(size)
}
unsafe extern "win64" fn malloc_realloc(_: usize, p: *mut c_void, size: usize) -> *mut c_void {
    co_task_mem_realloc(p, size)
}
unsafe extern "win64" fn malloc_free(_: usize, p: *mut c_void) {
    co_task_mem_free(p)
}
unsafe extern "win64" fn malloc_get_size(_: usize, _p: *mut c_void) -> usize {
    usize::MAX
}
unsafe extern "win64" fn malloc_did_alloc(_: usize, _p: *mut c_void) -> i32 {
    -1
}
unsafe extern "win64" fn malloc_heap_minimize(_: usize) {}

static MALLOC_VTABLE: IMallocVTable = IMallocVTable {
    query_interface: malloc_qi,
    add_ref: malloc_addref,
    release: malloc_release,
    alloc: malloc_alloc,
    realloc: malloc_realloc,
    free: malloc_free,
    get_size: malloc_get_size,
    did_alloc: malloc_did_alloc,
    heap_minimize: malloc_heap_minimize,
};

static MALLOC_INSTANCE: &IMallocVTable = &MALLOC_VTABLE;

pub unsafe extern "win64" fn co_get_malloc(_context: u32, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = std::ptr::addr_of!(MALLOC_INSTANCE) as *mut c_void;
    S_OK
}

// ---------- OLE Clipboard stubs ----------

pub extern "win64" fn ole_set_clipboard(_data_object: usize) -> i32 {
    S_OK
}
pub unsafe extern "win64" fn ole_get_clipboard(out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    S_OK
}
pub extern "win64" fn ole_flush_clipboard() -> i32 {
    S_OK
}

// ---------- Drag & Drop stubs ----------

const DRAGDROP_S_DROP: i32 = 0x0004_0100;

pub extern "win64" fn register_drag_drop(_hwnd: usize, _drop_target: usize) -> i32 {
    S_OK
}
pub extern "win64" fn revoke_drag_drop(_hwnd: usize) -> i32 {
    S_OK
}
pub extern "win64" fn do_drag_drop(
    _data_object: usize,
    _drop_source: usize,
    _ok_effects: u32,
    _effect: *mut u32,
) -> i32 {
    DRAGDROP_S_DROP
}

// ---------- RoInitialize ----------

/// `RoInitialize` : RO_INIT_SINGLETHREADED (0) donne un STA, RO_INIT_MULTITHREADED (1) le MTA.
pub extern "win64" fn ro_initialize(init_type: u32) -> i32 {
    co_initialize_ex(0, if init_type == 1 { 0 } else { 2 })
}
pub extern "win64" fn ro_uninitialize() {
    co_uninitialize()
}

// ---------- CoGetContextToken ----------

/// Objet de contexte COM : un par appartement STA (thread), un seul pour le
/// MTA. Le jeton est son pointeur IUnknown, sans référence ajoutée, comme
/// sous Windows ; seul IUnknown est exposé.
#[repr(C)]
struct ContextObject {
    vtbl: &'static ContextVtbl,
}
#[repr(C)]
struct ContextVtbl {
    query_interface: unsafe extern "win64" fn(*mut c_void, *const u8, *mut *mut c_void) -> i32,
    add_ref: extern "win64" fn(*mut c_void) -> u32,
    release: extern "win64" fn(*mut c_void) -> u32,
}
unsafe impl Sync for ContextObject {}

unsafe extern "win64" fn context_qi(
    this: *mut c_void,
    iid: *const u8,
    out: *mut *mut c_void,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    const IID_IUNKNOWN: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
    if !iid.is_null() && std::slice::from_raw_parts(iid, 16) == IID_IUNKNOWN {
        *out = this;
        return S_OK;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
extern "win64" fn context_ref(_this: *mut c_void) -> u32 {
    1
}
static CONTEXT_VTBL: ContextVtbl = ContextVtbl {
    query_interface: context_qi,
    add_ref: context_ref,
    release: context_ref,
};
static MTA_CONTEXT: ContextObject = ContextObject {
    vtbl: &CONTEXT_VTBL,
};
thread_local! {
    static STA_CONTEXT: ContextObject = const { ContextObject { vtbl: &CONTEXT_VTBL } };
}

/// `CoGetContextToken` : jeton du contexte COM du thread. Un thread sans
/// CoInitialize est dans le MTA implicite, comme sous Windows 8 et suivants.
pub unsafe extern "win64" fn co_get_context_token(token: *mut usize) -> i32 {
    if token.is_null() {
        return E_POINTER;
    }
    let sta = COM_APARTMENT.with(|a| matches!(a.get(), Some((2, _))));
    *token = if sta {
        STA_CONTEXT.with(|c| c as *const ContextObject as usize)
    } else {
        &MTA_CONTEXT as *const ContextObject as usize
    };
    S_OK
}

// ---------- PropVariantClear ----------

pub unsafe extern "win64" fn prop_variant_clear(v: *mut c_void) -> i32 {
    crate::dlls::oleaut32::variant_clear(v)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CoInitialize"=>fn co_initialize,"CoInitializeEx"=>fn co_initialize_ex,"CoUninitialize"=>fn co_uninitialize,
        "OleInitialize"=>fn ole_initialize,"OleUninitialize"=>fn ole_uninitialize,
        "CoTaskMemAlloc"=>fn co_task_mem_alloc,"CoTaskMemRealloc"=>fn co_task_mem_realloc,"CoTaskMemFree"=>fn co_task_mem_free,
        "CoCreateGuid"=>fn co_create_guid,"CLSIDFromString"=>fn clsid_from_string,"IIDFromString"=>fn clsid_from_string,
        "StringFromGUID2"=>fn string_from_guid2,"StringFromCLSID"=>fn string_from_clsid,"StringFromIID"=>fn string_from_clsid,
        "CreateStreamOnHGlobal"=>fn create_stream_on_hglobal,"GetHGlobalFromStream"=>fn get_hglobal_from_stream,
        "CoCreateInstance"=>fn co_create_instance,
        "CoRegisterClassObject"=>fn co_register_class_object,"CoRevokeClassObject"=>fn co_revoke_class_object,
        "CoGetClassObject"=>fn co_get_class_object,
        "CoCreateFreeThreadedMarshaler"=>fn co_create_free_threaded_marshaler,
        "ProgIDFromCLSID"=>fn prog_id_from_clsid,"CLSIDFromProgID"=>fn clsid_from_prog_id,
        "CoGetMalloc"=>fn co_get_malloc,
        "OleSetClipboard"=>fn ole_set_clipboard,"OleGetClipboard"=>fn ole_get_clipboard,"OleFlushClipboard"=>fn ole_flush_clipboard,
        "RegisterDragDrop"=>fn register_drag_drop,"RevokeDragDrop"=>fn revoke_drag_drop,"DoDragDrop"=>fn do_drag_drop,
        "PropVariantClear"=>fn prop_variant_clear,
        "CoGetContextToken"=>fn co_get_context_token,
        "RoInitialize"=>fn ro_initialize,"RoUninitialize"=>fn ro_uninitialize
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
    fn jeton_de_contexte_par_appartement() {
        std::thread::spawn(|| unsafe {
            let (mut mta, mut sta, mut other) = (0usize, 0usize, 0usize);
            // Sans CoInitialize : MTA implicite, jeton commun au processus.
            assert_eq!(co_get_context_token(&mut mta), S_OK);
            assert_ne!(mta, 0);
            assert_eq!(co_get_context_token(std::ptr::null_mut()), E_POINTER);
            assert_eq!(ro_initialize(0), S_OK); // RO_INIT_SINGLETHREADED → STA
            assert_eq!(co_get_context_token(&mut sta), S_OK);
            assert_ne!(sta, mta);
            assert_eq!(ro_initialize(1), RPC_E_CHANGED_MODE);
            ro_uninitialize();
            // Le jeton est un IUnknown : QueryInterface(IID_IUnknown) le renvoie.
            let vtbl = *(sta as *const *const ContextVtbl);
            let iid_unknown: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
            let mut out = std::ptr::null_mut();
            assert_eq!(
                ((*vtbl).query_interface)(sta as *mut _, iid_unknown.as_ptr(), &mut out),
                S_OK
            );
            assert_eq!(out as usize, sta);
            assert_eq!(co_get_context_token(&mut other), S_OK);
            assert_eq!(other, mta);
        })
        .join()
        .unwrap();
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
