//! DirectShow (quartz.dll) stubs – permet aux apps qui créent un
//! FilterGraph / IGraphBuilder de démarrer sans crash.

use std::ffi::c_void;

const E_NOINTERFACE: i32 = 0x80004002u32 as i32;
const VFW_E_CANNOT_RENDER: i32 = 0x80040218u32 as i32;

// ---------- Fake COM vtables -----------------------------------------------

#[repr(C)]
struct FakeUnknownVtbl {
    query_interface: unsafe extern "win64" fn(*mut c_void, *const u8, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut c_void) -> u32,
    release: unsafe extern "win64" fn(*mut c_void) -> u32,
}

#[repr(C)]
struct FakeGraphVtbl {
    // IUnknown
    query_interface: unsafe extern "win64" fn(*mut c_void, *const u8, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "win64" fn(*mut c_void) -> u32,
    release: unsafe extern "win64" fn(*mut c_void) -> u32,
    // IFilterGraph – AddFilter, RemoveFilter, EnumFilters, FindFilterByName, ConnectDirect, Reconnect, Disconnect, SetDefaultSyncSource
    add_filter: unsafe extern "win64" fn(*mut c_void, *mut c_void, *const u16) -> i32,
    remove_filter: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32,
    enum_filters: unsafe extern "win64" fn(*mut c_void, *mut *mut c_void) -> i32,
    find_filter_by_name: unsafe extern "win64" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32,
    connect_direct: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void, *const c_void) -> i32,
    reconnect: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32,
    disconnect: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32,
    set_default_sync_source: unsafe extern "win64" fn(*mut c_void) -> i32,
    // IGraphBuilder – Connect, Render, RenderFile, AddSourceFilter, SetLogFile, Abort, ShouldOperationContinue
    connect: unsafe extern "win64" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
    render: unsafe extern "win64" fn(*mut c_void, *mut c_void) -> i32,
    render_file: unsafe extern "win64" fn(*mut c_void, *const u16, *const u16) -> i32,
    add_source_filter: unsafe extern "win64" fn(*mut c_void, *const u16, *const u16, *mut *mut c_void) -> i32,
    set_log_file: unsafe extern "win64" fn(*mut c_void, usize) -> i32,
    abort: unsafe extern "win64" fn(*mut c_void) -> i32,
    should_operation_continue: unsafe extern "win64" fn(*mut c_void) -> i32,
    // IMediaControl – Run, Pause, Stop, GetState, RenderFile (MC), AddSourceFilter (MC), get_FilterCollection, get_RegFilterCollection, StopWhenReady
    mc_run: unsafe extern "win64" fn(*mut c_void) -> i32,
    mc_pause: unsafe extern "win64" fn(*mut c_void) -> i32,
    mc_stop: unsafe extern "win64" fn(*mut c_void) -> i32,
    mc_get_state: unsafe extern "win64" fn(*mut c_void, i32, *mut i32) -> i32,
}

// IGraphBuilder {56a868a9-0ad4-11ce-b03a-0020af0ba770}
const IID_IGRAPH_BUILDER: [u8; 16] = [
    0xa9, 0x68, 0xa8, 0x56, 0xd4, 0x0a, 0xce, 0x11,
    0xb0, 0x3a, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70,
];

// IMediaControl {56a868b1-0ad4-11ce-b03a-0020af0ba770}
const IID_IMEDIA_CONTROL: [u8; 16] = [
    0xb1, 0x68, 0xa8, 0x56, 0xd4, 0x0a, 0xce, 0x11,
    0xb0, 0x3a, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70,
];

// IMediaEvent {56a868b6-0ad4-11ce-b03a-0020af0ba770}
const IID_IMEDIA_EVENT: [u8; 16] = [
    0xb6, 0x68, 0xa8, 0x56, 0xd4, 0x0a, 0xce, 0x11,
    0xb0, 0x3a, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70,
];

#[repr(C)]
struct FakeGraph {
    vtbl: *const FakeGraphVtbl,
    ref_count: u32,
}

unsafe extern "win64" fn graph_qi(this: *mut c_void, riid: *const u8, out: *mut *mut c_void) -> i32 {
    if out.is_null() { return E_NOINTERFACE; }
    if riid.is_null() { *out = std::ptr::null_mut(); return E_NOINTERFACE; }
    let iid = std::slice::from_raw_parts(riid, 16);
    if iid == IID_IGRAPH_BUILDER || iid == IID_IMEDIA_CONTROL || iid == IID_IMEDIA_EVENT {
        graph_addref(this);
        *out = this;
        return 0;
    }
    *out = std::ptr::null_mut();
    E_NOINTERFACE
}
unsafe extern "win64" fn graph_addref(this: *mut c_void) -> u32 {
    let g = this as *mut FakeGraph;
    (*g).ref_count += 1;
    (*g).ref_count
}
unsafe extern "win64" fn graph_release(this: *mut c_void) -> u32 {
    let g = this as *mut FakeGraph;
    (*g).ref_count = (*g).ref_count.saturating_sub(1);
    let rc = (*g).ref_count;
    if rc == 0 { let _ = Box::from_raw(g); }
    rc
}
unsafe extern "win64" fn stub_hr1(_: *mut c_void) -> i32 { 0 }
unsafe extern "win64" fn stub_hr2(_: *mut c_void, _: *mut c_void) -> i32 { 0 }
unsafe extern "win64" fn stub_hr3pp(_: *mut c_void, _: *mut c_void, _: *mut c_void) -> i32 { 0 }
unsafe extern "win64" fn stub_hr2w(_: *mut c_void, _: *const u16) -> i32 { 0 }
unsafe extern "win64" fn stub_hr3(_: *mut c_void, _: *mut c_void, _: *const u16) -> i32 { 0 }
unsafe extern "win64" fn stub_hr4v(_: *mut c_void, _: *mut c_void, _: *mut c_void, _: *const c_void) -> i32 { 0 }
unsafe extern "win64" fn stub_enum(_: *mut c_void, _: *mut *mut c_void) -> i32 { E_NOINTERFACE }
unsafe extern "win64" fn stub_find(_: *mut c_void, _: *const u16, _: *mut *mut c_void) -> i32 { E_NOINTERFACE }
unsafe extern "win64" fn stub_render_file(_: *mut c_void, _: *const u16, _: *const u16) -> i32 { VFW_E_CANNOT_RENDER }
unsafe extern "win64" fn stub_add_src(_: *mut c_void, _: *const u16, _: *const u16, _: *mut *mut c_void) -> i32 { E_NOINTERFACE }
unsafe extern "win64" fn stub_set_log(_: *mut c_void, _: usize) -> i32 { 0 }
unsafe extern "win64" fn stub_get_state(_: *mut c_void, _: i32, out: *mut i32) -> i32 { if !out.is_null() { *out = 0; } 0 }

static GRAPH_VTBL: FakeGraphVtbl = FakeGraphVtbl {
    query_interface: graph_qi,
    add_ref: graph_addref,
    release: graph_release,
    add_filter: stub_hr3,
    remove_filter: stub_hr2,
    enum_filters: stub_enum,
    find_filter_by_name: stub_find,
    connect_direct: stub_hr4v,
    reconnect: stub_hr2,
    disconnect: stub_hr2,
    set_default_sync_source: stub_hr1,
    connect: stub_hr3pp,
    render: stub_hr2,
    render_file: stub_render_file,
    add_source_filter: stub_add_src,
    set_log_file: stub_set_log,
    abort: stub_hr1,
    should_operation_continue: stub_hr1,
    mc_run: stub_hr1,
    mc_pause: stub_hr1,
    mc_stop: stub_hr1,
    mc_get_state: stub_get_state,
};

// CLSID_FilterGraph {e436ebb3-524f-11ce-9f53-0020af0ba770}
const CLSID_FILTER_GRAPH: [u8; 16] = [
    0xb3, 0xeb, 0x36, 0xe4, 0x4f, 0x52, 0xce, 0x11,
    0x9f, 0x53, 0x00, 0x20, 0xaf, 0x0b, 0xa7, 0x70,
];

pub unsafe fn try_create_dshow(clsid: *const u8, _iid: *const u8, out: *mut *mut c_void) -> Option<i32> {
    if clsid.is_null() || out.is_null() { return None; }
    let cls = std::slice::from_raw_parts(clsid, 16);
    if cls != CLSID_FILTER_GRAPH { return None; }
    let g = Box::new(FakeGraph { vtbl: &GRAPH_VTBL, ref_count: 1 });
    *out = Box::into_raw(g) as *mut c_void;
    Some(0)
}

// ---------- AMGetErrorTextA/W ----------
pub unsafe extern "win64" fn am_get_error_text_a(hr: i32, buf: *mut u8, max_len: u32) -> u32 {
    let msg = format!("DirectShow error 0x{:08X}", hr as u32);
    crate::dlls::put_a(&msg, buf, max_len)
}

pub unsafe extern "win64" fn am_get_error_text_w(hr: i32, buf: *mut u16, max_len: u32) -> u32 {
    let msg = format!("DirectShow error 0x{:08X}", hr as u32);
    crate::dlls::put_w(&msg, buf, max_len)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "AMGetErrorTextA" => fn am_get_error_text_a,
        "AMGetErrorTextW" => fn am_get_error_text_w,
    }
}
