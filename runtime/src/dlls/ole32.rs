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

/// Adresses de retour de l'appli (« module+rva »), pour les journaux
/// d'échec : « aucun » quand seul Winlux est sur la pile.
fn caller_text() -> String {
    let calls = super::seh::caller_stack(4);
    if calls.is_empty() {
        "aucun".into()
    } else {
        calls.join(" < ")
    }
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

pub(super) fn parse_guid(value: &str) -> Option<Guid> {
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

/// Appartement COM du thread : modèle (2 = STA, 0 = MTA) et nombre
/// d'initialisations en cours.
pub(crate) fn current_apartment() -> Option<(u32, u32)> {
    COM_APARTMENT.with(|apartment| apartment.get())
}

/// Jetons de CoIncrementMTAUsage encore pris (MTA implicite).
pub(crate) fn mta_usage() -> u32 {
    MTA_USAGE.load(std::sync::atomic::Ordering::SeqCst) as u32
}

fn apartment_refs() -> u32 {
    current_apartment().map(|(_, n)| n).unwrap_or(0)
}

pub extern "win64" fn co_initialize_ex(reserved: usize, coinit: u32) -> i32 {
    if reserved != 0 {
        return E_INVALIDARG;
    }
    // Espions d'IInitializeSpy (CoRegisterInitializeSpy) : avant et après.
    unsafe { super::ole32_lot_c::before_initialize(coinit, apartment_refs()) };
    let hr = co_initialize_apartment(coinit);
    unsafe { super::ole32_lot_c::after_initialize(hr, coinit, apartment_refs()) }
}

fn co_initialize_apartment(coinit: u32) -> i32 {
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
    let Some((model, count)) = current_apartment() else {
        return;
    };
    unsafe { super::ole32_lot_c::before_uninitialize(count) };
    if count == 1 {
        // Dernière sortie : RoRegisterForApartmentShutdown est prévenu
        // tant que l'appartement existe encore.
        unsafe { super::ole32_lot_c::apartment_shutdown() };
    }
    COM_APARTMENT.with(|apartment| {
        apartment.set(if count > 1 {
            Some((model, count - 1))
        } else {
            None
        });
    });
    unsafe { super::ole32_lot_c::after_uninitialize(count - 1) };
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
    // Échec : GUID_NULL. La documentation ne dit rien de la sortie en cas
    // d'échec ; règle à nous, comme CLSIDFromProgID (#2274).
    *out = Guid::default();
    let Some(text) = read_wide_string(value) else {
        return CO_E_CLASSSTRING;
    };
    if let Some(guid) = parse_guid(&text) {
        *out = guid;
        return S_OK;
    }
    // « Passing the ProgID results in saving associated CLSID in pclsid »
    // (CLSIDFromString, Remarks) : un ProgID se résout comme CLSIDFromProgID.
    if text.starts_with('{') || clsid_from_prog_id(value, out) != S_OK {
        *out = Guid::default();
        return CO_E_CLASSSTRING;
    }
    S_OK
}

/// IIDFromString : la forme `{…}` seule, un IID n'a pas de ProgID (#2274).
pub unsafe extern "win64" fn iid_from_string(value: *const u16, out: *mut Guid) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    if value.is_null() {
        *out = Guid::default();
        return S_OK;
    }
    let Some(guid) = read_wide_string(value).and_then(|v| parse_guid(&v)) else {
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
    // REGCLS_SINGLEUSE (0), MULTIPLEUSE (1), MULTI_SEPARATE (2), avec ou
    // sans REGCLS_SUSPENDED (4) : chaque création passe par la fabrique,
    // et une inscription suspendue est servie aussitôt.
    if _flags & !4 > 2 {
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
    // Serveur hors processus : canal ouvert aux autres processus (#16).
    if ctx & super::com_local::CLSCTX_LOCAL_SERVER != 0 {
        let hr = super::com_local::serve(c, &guid, punk);
        if hr < 0 {
            co_revoke_class_object(c);
            return hr;
        }
    }
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
    super::com_local::revoke(cookie);
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
        // Boîtes Ouvrir/Enregistrer de Vista (IFileOpenDialog/IFileSaveDialog).
        if let Some(hr) = super::filedialog_com::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Raccourcis Windows (ShellLink : Discord, Electron, #1732).
        if let Some(hr) = super::shell_link::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // DirectInput 8 et 7 par CoCreateInstance (SDL 2, #1516).
        if let Some(hr) = super::dinput8::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Graphe de lecture DirectShow (quartz.dll, #835).
        if let Some(hr) = super::quartz::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // DirectPlay et son lobby (dplayx.dll, #836).
        if let Some(hr) = super::dplayx::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // DirectPlay 8 (dpnet.dll, #863).
        if let Some(hr) = super::dpnet::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Listes de raccourcis de la barre des tâches (WinSCP, #905).
        if let Some(hr) = super::jumplist_com::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Bouton de la barre des tâches : progression, icône superposée (#1434).
        if let Some(hr) = super::taskbarlist_com::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Pare-feu, ancienne interface HNetCfg.FwMgr (SteamService, #1919).
        if let Some(hr) = super::netfw::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Profils de saisie du Text Services Framework (CEF de Steam, #1602).
        if let Some(hr) = super::msctf_profiles::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Annotations d'accessibilité (AccPropServices, VCL, #1114).
        if let Some(hr) = super::oleacc_props::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // WMI en lecture (WbemLocator, ROOT\\CIMV2, #1198).
        if let Some(hr) = super::wbem::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Documents XML MSXML (DOMDocument 2.6 à 6.0, #1084).
        if let Some(hr) = super::msxml::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Gestionnaire de listes de réseaux (module réseau de Qt 5, iCUE #1844).
        if let Some(hr) = super::netlistmgr::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Gestionnaire d'animations (UIAnimationManager, Paint.NET #561).
        if let Some(hr) = super::uianimation::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Options globales du runtime COM (IGlobalOptions, Teams #2208).
        if let Some(hr) = super::global_options::create_builtin(&guid, &iid, out) {
            return hr;
        }
        // Serveur en processus inscrit au registre (InprocServer32, #16).
        if ctx & super::com_registry::CLSCTX_INPROC_SERVER != 0 {
            if let Some(hr) = create_from_registry(&guid, &iid, out) {
                return hr;
            }
        }
        // Serveur hors processus (LocalServer32) : mandataire IDispatch.
        if ctx & super::com_local::CLSCTX_LOCAL_SERVER != 0 {
            if let Some(hr) = super::com_local::create(&guid, &iid, out) {
                return hr;
            }
        }
        // Interface, contexte et appelants : sans eux, un CLSID mal formé
        // ({00000000-0000-0000-0700-…} chez WinRAR, #1830) ne dit pas qui
        // l'a fourni, de l'appli ou d'une fonction de Winlux.
        crate::log_warn!(
            "com",
            "CoCreateInstance : classe {} inconnue (REGDB_E_CLASSNOTREG), interface {}, contexte {:#x}, appelants {}",
            guid_text(&guid),
            guid_text(&iid),
            ctx,
            caller_text()
        );
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

/// MULTI_QI de CoCreateInstanceEx.
#[repr(C)]
pub struct MultiQi {
    iid: *const Guid,
    itf: *mut c_void,
    hr: i32,
}

/// CoCreateInstanceEx : création locale (le nom de machine distante n'est
/// pas pris en charge) puis QueryInterface pour chaque MULTI_QI.
pub unsafe extern "win64" fn co_create_instance_ex(
    rclsid: *const Guid,
    outer: usize,
    ctx: u32,
    server: *const usize,
    count: u32,
    results: *mut MultiQi,
) -> i32 {
    if count == 0 || results.is_null() {
        return E_INVALIDARG;
    }
    let results = std::slice::from_raw_parts_mut(results, count as usize);
    for r in results.iter_mut() {
        r.itf = std::ptr::null_mut();
        r.hr = E_NOINTERFACE;
    }
    // COSERVERINFO : pwszName en deuxième champ ; seule la machine locale.
    if !server.is_null() && *server.add(1) != 0 {
        let name = read_wide_string(*server.add(1) as *const u16).unwrap_or_default();
        if !name.is_empty() && name != "." && !name.eq_ignore_ascii_case("localhost") {
            return 0x8007_06bau32 as i32; // RPC_S_SERVER_UNAVAILABLE
        }
    }
    let mut unknown: *mut c_void = std::ptr::null_mut();
    let hr = co_create_instance(rclsid, outer, ctx, &IID_IUNKNOWN, &mut unknown);
    if hr < 0 {
        for r in results.iter_mut() {
            r.hr = hr;
        }
        return hr;
    }
    let vtable = *(unknown as *const *const usize);
    let qi: unsafe extern "win64" fn(usize, *const Guid, *mut *mut c_void) -> i32 =
        std::mem::transmute(*vtable);
    let release: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vtable.add(2));
    let mut found = 0;
    for r in results.iter_mut() {
        if r.iid.is_null() {
            r.hr = E_INVALIDARG;
            continue;
        }
        r.hr = qi(unknown as usize, r.iid, &mut r.itf);
        if r.hr >= 0 {
            found += 1;
        }
    }
    release(unknown as usize);
    match found {
        0 => E_NOINTERFACE,
        n if n == results.len() => S_OK,
        _ => 0x0008_0012, // CO_S_NOTALLINTERFACES
    }
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
        if ctx & super::com_registry::CLSCTX_INPROC_SERVER != 0 {
            if let Some(hr) = super::com_registry::class_object(&guid, &iid, out) {
                return hr;
            }
        }
        crate::log_warn!(
            "com",
            "CoGetClassObject : classe {} inconnue (REGDB_E_CLASSNOTREG), interface {}, contexte {:#x}, appelants {}",
            guid_text(&guid),
            guid_text(&iid),
            ctx,
            caller_text()
        );
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
const IID_ICLASSFACTORY: Guid = Guid {
    data1: 0x0000_0001,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};

/// CoCreateInstance par la fabrique d'un serveur inscrit au registre :
/// IClassFactory::CreateInstance puis Release de la fabrique.
unsafe fn create_from_registry(guid: &Guid, iid: &Guid, out: *mut *mut c_void) -> Option<i32> {
    let mut factory: *mut c_void = std::ptr::null_mut();
    let hr = super::com_registry::class_object(guid, &IID_ICLASSFACTORY, &mut factory)?;
    if hr < 0 || factory.is_null() {
        return Some(if hr < 0 { hr } else { E_NOINTERFACE });
    }
    let vtable = *(factory as *const *const usize);
    let create: unsafe extern "win64" fn(usize, usize, *const Guid, *mut *mut c_void) -> i32 =
        std::mem::transmute(*vtable.add(3));
    let release: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vtable.add(2));
    let result = create(factory as usize, 0, iid, out);
    release(factory as usize);
    Some(result)
}

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
        None => {
            // Classes intégrées sans clé HKCR : MSXML (#1758).
            if let Some(g) = super::msxml::clsid_for_prog_id(&name) {
                *clsid = g;
                return S_OK;
            }
            if std::env::var_os("WINLUX_TRACE").is_some() {
                eprintln!(
                    "winlux : CLSIDFromProgID(\"{name}\") = 0x800401F3 (CO_E_CLASSSTRING) : ProgID inconnu"
                );
            }
            return CO_E_CLASSSTRING;
        }
    };
    let text = text.as_str();
    match parse_guid(text) {
        Some(g) => {
            *clsid = g;
            S_OK
        }
        None => {
            if std::env::var_os("WINLUX_TRACE").is_some() {
                eprintln!(
                    "winlux : CLSIDFromProgID(\"{name}\") = 0x800401F3 (CO_E_CLASSSTRING) : CLSID illisible « {text} »"
                );
            }
            CO_E_CLASSSTRING
        }
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

// ---------- CoIncrementMTAUsage / CoDecrementMTAUsage ----------

/// Utilisations du MTA posées par CoIncrementMTAUsage et pas encore rendues.
static MTA_USAGE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Prochain jeton (CO_MTA_USAGE_COOKIE), jamais nul.
static MTA_COOKIE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// `CoIncrementMTAUsage` (combaseapi.h, Windows 8) : garde le MTA du
/// processus en vie sans CoInitializeEx sur le thread. Le runtime .NET
/// (CsWinRT) l'appelle avant d'activer une classe WinRT et rend le jeton
/// dans un finaliseur (#1147 : Flow Launcher). Ici le MTA implicite existe
/// toujours : le compteur suit seulement les appels.
/// CoInitializeSecurity : sécurité par défaut du processus. Les objets
/// COM de Winlux vivent dans le processus : rien à régler (WMI, #1198).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn co_initialize_security(
    _desc: usize,
    _count: i32,
    _services: usize,
    _reserved: usize,
    _authn_level: u32,
    _imp_level: u32,
    _auth_list: usize,
    _capabilities: u32,
    _reserved3: usize,
) -> i32 {
    0
}

/// CoSetProxyBlanket : réglage d'authentification d'un mandataire ; les
/// objets en processus l'acceptent tel quel.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn co_set_proxy_blanket(
    proxy: usize,
    _authn: u32,
    _authz: u32,
    _principal: usize,
    _authn_level: u32,
    _imp_level: u32,
    _auth_info: usize,
    _capabilities: u32,
) -> i32 {
    if proxy == 0 {
        return E_INVALIDARG;
    }
    0
}

/// CoQueryProxyBlanket : valeurs par défaut (NTLM, appel, emprunt d'identité).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "win64" fn co_query_proxy_blanket(
    proxy: usize,
    authn: *mut u32,
    authz: *mut u32,
    principal: *mut usize,
    authn_level: *mut u32,
    imp_level: *mut u32,
    auth_info: *mut usize,
    capabilities: *mut u32,
) -> i32 {
    if proxy == 0 {
        return E_INVALIDARG;
    }
    for (p, v) in [
        (authn, 10u32),
        (authz, 0),
        (authn_level, 3),
        (imp_level, 3),
        (capabilities, 0),
    ] {
        if !p.is_null() {
            p.write_unaligned(v);
        }
    }
    for p in [principal, auth_info] {
        if !p.is_null() {
            p.write_unaligned(0);
        }
    }
    0
}

/// CoCopyProxy : un objet en processus est son propre mandataire.
pub unsafe extern "win64" fn co_copy_proxy(proxy: usize, out: *mut usize) -> i32 {
    if proxy == 0 || out.is_null() {
        return E_INVALIDARG;
    }
    let add_ref: unsafe extern "win64" fn(usize) -> u32 =
        std::mem::transmute(*(*(proxy as *const *const usize)).add(1));
    add_ref(proxy);
    *out = proxy;
    0
}

pub unsafe extern "win64" fn co_increment_mta_usage(cookie: *mut usize) -> i32 {
    if cookie.is_null() {
        return E_INVALIDARG;
    }
    MTA_USAGE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    // CO_MTA_USAGE_COOKIE est un pointeur : 4 octets en x86 (#1630).
    crate::wow32::write_ptr(
        cookie as usize,
        MTA_COOKIE.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
    );
    S_OK
}

/// `CoDecrementMTAUsage` : rend un jeton de CoIncrementMTAUsage.
pub extern "win64" fn co_decrement_mta_usage(cookie: usize) -> i32 {
    if cookie == 0 {
        return E_INVALIDARG;
    }
    use std::sync::atomic::Ordering::SeqCst;
    let mut n = MTA_USAGE.load(SeqCst);
    while n > 0 {
        match MTA_USAGE.compare_exchange(n, n - 1, SeqCst, SeqCst) {
            Ok(_) => break,
            Err(now) => n = now,
        }
    }
    S_OK
}

// ---------- CoWaitForMultipleHandles ----------

/// `CoWaitForMultipleHandles` : attente des handles (COWAIT_WAITALL = 1) ;
/// RPC_S_CALLPENDING si le délai expire, index du handle signalé sinon.
pub unsafe extern "win64" fn co_wait_for_multiple_handles(
    flags: u32,
    timeout: u32,
    count: u32,
    handles: *const usize,
    index: *mut u32,
) -> i32 {
    const RPC_S_CALLPENDING: i32 = 0x8001_0115u32 as i32;
    if handles.is_null() || index.is_null() || count == 0 {
        return E_INVALIDARG;
    }
    let r = crate::dlls::kernel32::wait_for_multiple_objects(
        count,
        handles,
        i32::from(flags & 1 != 0),
        timeout,
    );
    match r {
        0x102 => RPC_S_CALLPENDING, // WAIT_TIMEOUT
        r if r < count || (0x80..0x80 + count).contains(&r) => {
            *index = r & 0x7F;
            S_OK
        }
        _ => E_INVALIDARG,
    }
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

/// `CoGetObjectContext` : interface `riid` du contexte du thread (même objet
/// que `CoGetContextToken`). Le runtime .NET la demande dès qu'un objet COM
/// traverse le code managé (#268 : clic sur un bouton WPF).
pub unsafe extern "win64" fn co_get_object_context(riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut token = 0usize;
    co_get_context_token(&mut token);
    context_qi(token as *mut c_void, riid, out)
}

// ---------- PropVariantClear ----------

pub unsafe extern "win64" fn prop_variant_clear(v: *mut c_void) -> i32 {
    crate::dlls::oleaut32::variant_clear(v)
}

// --- Verrous d'objets et marshalling dans le même processus ----------------------------

unsafe extern "win64" fn ole_s_ok(_a: usize, _b: usize, _c: usize) -> u32 {
    0
}

/// Flux de marshalling factice : il porte l'interface (AddRef faite) d'un
/// thread à l'autre du même processus ; c'est un vrai objet COM minimal.
#[repr(C)]
struct MarshalStream {
    vtbl: &'static MarshalVtbl,
    refs: std::sync::atomic::AtomicU32,
    punk: usize,
}

#[repr(C)]
struct MarshalVtbl {
    qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> u32,
    add_ref: unsafe extern "win64" fn(usize) -> u32,
    release: unsafe extern "win64" fn(usize) -> u32,
}

unsafe extern "win64" fn ms_qi(_this: usize, _iid: *const u8, out: *mut usize) -> u32 {
    if !out.is_null() {
        *out = 0;
    }
    0x8000_4002 // E_NOINTERFACE
}

unsafe extern "win64" fn ms_add_ref(this: usize) -> u32 {
    (*(this as *const MarshalStream))
        .refs
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
        + 1
}

unsafe extern "win64" fn ms_release(this: usize) -> u32 {
    let s = this as *mut MarshalStream;
    let n = (*s).refs.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) - 1;
    if n == 0 {
        let b = Box::from_raw(s);
        if b.punk != 0 {
            com_release(b.punk);
        }
    }
    n
}

static MARSHAL_VTBL: MarshalVtbl = MarshalVtbl {
    qi: ms_qi,
    add_ref: ms_add_ref,
    release: ms_release,
};

unsafe fn com_call(obj: usize, slot: usize) -> usize {
    *((*(obj as *const usize)) as *const usize).add(slot)
}

unsafe fn com_release(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(com_call(obj, 2));
    f(obj);
}

unsafe extern "win64" fn co_marshal_inter_thread_interface_in_stream(
    _iid: *const u8,
    punk: usize,
    stream: *mut usize,
) -> u32 {
    if punk == 0 || stream.is_null() {
        return 0x8007_0057; // E_INVALIDARG
    }
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(com_call(punk, 1));
    f(punk);
    let s = Box::new(MarshalStream {
        vtbl: &MARSHAL_VTBL,
        refs: std::sync::atomic::AtomicU32::new(1),
        punk,
    });
    *stream = Box::into_raw(s) as usize;
    0
}

unsafe extern "win64" fn co_get_interface_and_release_stream(
    stream: usize,
    iid: *const u8,
    out: *mut usize,
) -> u32 {
    if stream == 0 || out.is_null() {
        return 0x8007_0057;
    }
    let s = stream as *const MarshalStream;
    if !std::ptr::eq((*s).vtbl, &MARSHAL_VTBL) {
        *out = 0;
        return 0x8000_4002;
    }
    let punk = (*s).punk;
    let qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> u32 =
        std::mem::transmute(com_call(punk, 0));
    let hr = qi(punk, iid, out);
    ms_release(stream);
    hr
}

// ---------- Fonctions OLE sans objet derrière ----------

/// IsEqualGUID (exportée par ole32, en plus de la macro de objbase.h).
pub unsafe extern "win64" fn is_equal_guid(a: *const Guid, b: *const Guid) -> i32 {
    match (
        super::comobj::read_guid(a as *const u8),
        super::comobj::read_guid(b as *const u8),
    ) {
        (Some(a), Some(b)) => (a == b) as i32,
        _ => 0,
    }
}

/// CoDisconnectObject : pas de mandataire hors processus, rien à couper.
pub extern "win64" fn co_disconnect_object(_unk: usize, _reserved: u32) -> i32 {
    S_OK
}

/// CoFreeUnusedLibraries : décharge les serveurs en processus chargés
/// depuis le registre dont `DllCanUnloadNow` rend S_OK.
pub extern "win64" fn co_free_unused_libraries() {
    unsafe {
        super::com_registry::free_unused();
    }
}

static SERVER_PROCESS_REFS: AtomicU32 = AtomicU32::new(0);

/// CoAddRefServerProcess : compteur global du serveur hors processus.
pub extern "win64" fn co_add_ref_server_process() -> u32 {
    SERVER_PROCESS_REFS.fetch_add(1, Ordering::SeqCst) + 1
}

/// CoReleaseServerProcess : à zéro, le serveur va s'arrêter.
pub extern "win64" fn co_release_server_process() -> u32 {
    let mut n = SERVER_PROCESS_REFS.load(Ordering::SeqCst);
    loop {
        let next = n.saturating_sub(1);
        match SERVER_PROCESS_REFS.compare_exchange(n, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return next,
            Err(actual) => n = actual,
        }
    }
}

/// CoResumeClassObjects / CoSuspendClassObjects : les inscriptions sont
/// servies dès CoRegisterClassObject.
pub extern "win64" fn co_resume_class_objects() -> i32 {
    S_OK
}

/// CoFreeUnusedLibrariesEx : le délai demandé n'est pas observé.
pub extern "win64" fn co_free_unused_libraries_ex(_delay: u32, _reserved: u32) {
    co_free_unused_libraries();
}

// CreateBindCtx : ole32_moniker.rs.

/// OleRegEnumVerbs : aucun verbe OLE enregistré (OLEOBJ_E_NOVERBS).
pub unsafe extern "win64" fn ole_reg_enum_verbs(_clsid: *const Guid, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    0x8004_0180u32 as i32
}

thread_local! {
    /// Filtre de messages COM du thread (CoRegisterMessageFilter).
    static MESSAGE_FILTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `CoRegisterMessageFilter(nouveau, &ancien)` : un filtre par thread. Le
/// nouveau est gardé (AddRef) et l'ancien rendu à l'appelant, qui en devient
/// propriétaire ; sans pointeur de retour, l'ancien est relâché. WinForms
/// l'appelle avant sa boucle de messages pour trouver un gestionnaire de
/// composants Office (#666).
pub unsafe extern "win64" fn co_register_message_filter(
    filter: usize,
    previous: *mut usize,
) -> i32 {
    unsafe fn call_slot(obj: usize, slot: usize) {
        let vtbl = *(obj as *const *const usize);
        let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(*vtbl.add(slot));
        f(obj);
    }
    if filter != 0 {
        call_slot(filter, 1); // AddRef
    }
    let old = MESSAGE_FILTER.with(|m| m.replace(filter));
    if previous.is_null() {
        if old != 0 {
            call_slot(old, 2); // Release
        }
    } else {
        *previous = old;
    }
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "CoRegisterMessageFilter"=>fn co_register_message_filter,
        "CoInitialize"=>fn co_initialize,"CoInitializeEx"=>fn co_initialize_ex,"CoUninitialize"=>fn co_uninitialize,
        "OleInitialize"=>fn ole_initialize,"OleUninitialize"=>fn ole_uninitialize,
        "CoTaskMemAlloc"=>fn co_task_mem_alloc,"CoTaskMemRealloc"=>fn co_task_mem_realloc,"CoTaskMemFree"=>fn co_task_mem_free,
        "CoCreateGuid"=>fn co_create_guid,"CLSIDFromString"=>fn clsid_from_string,"IIDFromString"=>fn iid_from_string,
        "StringFromGUID2"=>fn string_from_guid2,"StringFromCLSID"=>fn string_from_clsid,"StringFromIID"=>fn string_from_clsid,
        "CreateStreamOnHGlobal"=>fn create_stream_on_hglobal,"GetHGlobalFromStream"=>fn get_hglobal_from_stream,
        "CoCreateInstance"=>fn co_create_instance,"CoCreateInstanceEx"=>fn co_create_instance_ex,
        "CoRegisterClassObject"=>fn co_register_class_object,"CoRevokeClassObject"=>fn co_revoke_class_object,
        "CoGetClassObject"=>fn co_get_class_object,
        "CoCreateFreeThreadedMarshaler"=>fn co_create_free_threaded_marshaler,
        "ProgIDFromCLSID"=>fn prog_id_from_clsid,"CLSIDFromProgID"=>fn clsid_from_prog_id,
        "CoGetMalloc"=>fn co_get_malloc,
        "CoLockObjectExternal"=>fn ole_s_ok,
        // OleRun, OleLockRunning, OleSetContainedObject, OleDraw : ole32_embed.rs.
        "CoMarshalInterThreadInterfaceInStream"=>fn co_marshal_inter_thread_interface_in_stream,
        "CoGetInterfaceAndReleaseStream"=>fn co_get_interface_and_release_stream,
        "OleSetClipboard"=>fn ole_set_clipboard,"OleGetClipboard"=>fn ole_get_clipboard,"OleFlushClipboard"=>fn ole_flush_clipboard,
        "RegisterDragDrop"=>fn register_drag_drop,"RevokeDragDrop"=>fn revoke_drag_drop,"DoDragDrop"=>fn do_drag_drop,
        "PropVariantClear"=>fn prop_variant_clear,
        "CoGetContextToken"=>fn co_get_context_token,
        "CoInitializeSecurity"=>fn co_initialize_security,"CoSetProxyBlanket"=>fn co_set_proxy_blanket,
        "CoQueryProxyBlanket"=>fn co_query_proxy_blanket,"CoCopyProxy"=>fn co_copy_proxy,
        "CoIncrementMTAUsage"=>fn co_increment_mta_usage,"CoDecrementMTAUsage"=>fn co_decrement_mta_usage,
        "CoGetObjectContext"=>fn co_get_object_context,
        "CoWaitForMultipleHandles"=>fn co_wait_for_multiple_handles,
        // ole32 exporte aussi les fonctions d'IErrorInfo d'oleaut32
        // (MinGW les importe d'ole32).
    "IsEqualGUID"=>fn is_equal_guid,"CoDisconnectObject"=>fn co_disconnect_object,
        "CoFreeUnusedLibraries"=>fn co_free_unused_libraries,"CoFreeUnusedLibrariesEx"=>fn co_free_unused_libraries_ex,
        "CoAddRefServerProcess"=>fn co_add_ref_server_process,"CoReleaseServerProcess"=>fn co_release_server_process,
        "CoResumeClassObjects"=>fn co_resume_class_objects,"CoSuspendClassObjects"=>fn co_resume_class_objects,
        "OleRegEnumVerbs"=>fn ole_reg_enum_verbs,
        // IsAccelerator, OleSetMenuDescriptor : ole32_inplace.rs.
        "CreateErrorInfo"=>fn super::errorinfo::create_error_info,
        "GetErrorInfo"=>fn super::errorinfo::get_error_info,
        "SetErrorInfo"=>fn super::errorinfo::set_error_info,
        "RoInitialize"=>fn ro_initialize,"RoUninitialize"=>fn ro_uninitialize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utilisation_du_mta_par_jetons() {
        // CsWinRT (.NET) : jeton pris à l'activation, rendu par un finaliseur.
        unsafe {
            let mut a = 0usize;
            let mut b = 0usize;
            assert_eq!(co_increment_mta_usage(&mut a), S_OK);
            assert_eq!(co_increment_mta_usage(&mut b), S_OK);
            assert!(a != 0 && b != 0 && a != b);
            assert_eq!(co_decrement_mta_usage(b), S_OK);
            assert_eq!(co_decrement_mta_usage(a), S_OK);
            assert_eq!(co_increment_mta_usage(std::ptr::null_mut()), E_INVALIDARG);
            assert_eq!(co_decrement_mta_usage(0), E_INVALIDARG);
        }
    }

    #[test]
    fn clsid_from_string_resout_un_progid() {
        // #2274 : « Passing the ProgID results in saving associated CLSID in
        // pclsid » (CLSIDFromString, Remarks).
        let w = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let rempli = || parse_guid("{11111111-1111-1111-1111-111111111111}").unwrap();
        unsafe {
            let prog = w("Msxml2.DOMDocument");
            let mut attendu = Guid::default();
            assert_eq!(clsid_from_prog_id(prog.as_ptr(), &mut attendu), S_OK);
            let mut g = rempli();
            assert_eq!(clsid_from_string(prog.as_ptr(), &mut g), S_OK);
            assert_eq!(guid_text(&g), guid_text(&attendu));
            // La forme {…} reste servie.
            let texte = w("{00112233-4455-6677-8899-AABBCCDDEEFF}");
            assert_eq!(clsid_from_string(texte.as_ptr(), &mut g), S_OK);
            assert_eq!(guid_text(&g), "{00112233-4455-6677-8899-AABBCCDDEEFF}");
            // Échec : GUID_NULL (règle à nous, la documentation est muette).
            for faux in ["Winlux.ProgIdInexistant", "{pas-un-guid}"] {
                let mut g = rempli();
                assert_eq!(
                    clsid_from_string(w(faux).as_ptr(), &mut g),
                    CO_E_CLASSSTRING
                );
                assert_eq!(guid_text(&g), guid_text(&Guid::default()));
            }
            // IIDFromString : pas de ProgID.
            let mut g = rempli();
            assert_eq!(iid_from_string(prog.as_ptr(), &mut g), CO_E_CLASSSTRING);
            assert_eq!(iid_from_string(texte.as_ptr(), &mut g), S_OK);
        }
    }

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
    fn attente_de_handles_com() {
        unsafe {
            let e = crate::dlls::kernel32::create_event_w(0, 1, 0, std::ptr::null());
            let mut index = 9;
            assert_eq!(
                co_wait_for_multiple_handles(0, 0, 1, &e, &mut index),
                0x8001_0115u32 as i32
            );
            crate::dlls::kernel32::set_event(e);
            assert_eq!(co_wait_for_multiple_handles(0, 0, 1, &e, &mut index), S_OK);
            assert_eq!(index, 0);
            assert_eq!(
                co_wait_for_multiple_handles(0, 0, 0, &e, &mut index),
                E_INVALIDARG
            );
            crate::dlls::kernel32::close_handle(e);
        }
    }

    #[test]
    fn contexte_d_objet_du_thread() {
        unsafe {
            const IID_IUNKNOWN: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
            let mut token = 0usize;
            co_get_context_token(&mut token);
            let mut ctx = std::ptr::null_mut();
            assert_eq!(co_get_object_context(IID_IUNKNOWN.as_ptr(), &mut ctx), S_OK);
            assert_eq!(ctx as usize, token);
            let other = [1u8; 16];
            assert_eq!(
                co_get_object_context(other.as_ptr(), &mut ctx),
                E_NOINTERFACE
            );
            assert!(ctx.is_null());
            assert_eq!(
                co_get_object_context(IID_IUNKNOWN.as_ptr(), std::ptr::null_mut()),
                E_POINTER
            );
        }
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
