use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::thread::ThreadId;

pub static mut PROCESS_HEAP: usize = 0;
struct VmRegion { size: usize, committed: Vec<bool>, protection: Vec<u32>, allocation_protection: u32 }
static ALLOCS: OnceLock<Mutex<HashMap<usize, VmRegion>>> = OnceLock::new();
static HEAP_ALLOCS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
enum FileHandle { File(File) }
static HANDLES: OnceLock<Mutex<HashMap<usize, FileHandle>>> = OnceLock::new();
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(0x1_0000);

struct EventObject { manual_reset: bool, signaled: Mutex<bool>, changed: Condvar }
struct MutexState { owner: Option<ThreadId>, recursion: usize }
struct MutexObject { state: Mutex<MutexState>, changed: Condvar }
struct ThreadState { finished: bool, exit_code: u32, suspend_count: usize }
struct ThreadObject { state: Mutex<ThreadState>, changed: Condvar }
struct ProcessState { child: Option<Child>, exit_code: u32 }
struct ProcessObject { state: Mutex<ProcessState>, changed: Condvar }
#[derive(Clone)]
enum SyncHandle { Event(Arc<EventObject>), Mutex(Arc<MutexObject>), Thread(Arc<ThreadObject>), Process(Arc<ProcessObject>) }
static SYNCS: OnceLock<Mutex<HashMap<usize, SyncHandle>>> = OnceLock::new();
static SYNC_NAMES: OnceLock<Mutex<HashMap<String, (SyncHandle, usize)>>> = OnceLock::new();
static SYNC_GATE: OnceLock<Mutex<()>> = OnceLock::new();
static CRITICALS: OnceLock<Mutex<HashMap<usize, Arc<MutexObject>>>> = OnceLock::new();
const TLS_LIMIT: usize = crate::process::TLS_SLOTS + crate::process::TLS_EXPANSION_SLOTS;
static TLS_GENERATION: AtomicUsize = AtomicUsize::new(1);
static TLS_SLOTS: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();
static CLOCK_START: OnceLock<std::time::Instant> = OnceLock::new();

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SystemInfo {
    pub processor_architecture: u16,
    pub reserved: u16,
    pub page_size: u32,
    pub minimum_application_address: *mut c_void,
    pub maximum_application_address: *mut c_void,
    pub active_processor_mask: usize,
    pub number_of_processors: u32,
    pub processor_type: u32,
    pub allocation_granularity: u32,
    pub processor_level: u16,
    pub processor_revision: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryBasicInformation {
    pub base_address:*mut c_void,
    pub allocation_base:*mut c_void,
    pub allocation_protect:u32,
    pub partition_id:u16,
    pub region_size:usize,
    pub state:u32,
    pub protect:u32,
    pub kind:u32,
}

#[repr(C)]
pub struct StartupInfoA {
    pub cb:u32, pub reserved:*mut u8, pub desktop:*mut u8, pub title:*mut u8,
    pub x:u32,pub y:u32,pub x_size:u32,pub y_size:u32,pub x_chars:u32,pub y_chars:u32,
    pub fill_attribute:u32,pub flags:u32,pub show_window:u16,pub reserved2_size:u16,
    pub reserved2:*mut u8,pub std_input:usize,pub std_output:usize,pub std_error:usize,
}
#[repr(C)]
pub struct ProcessInformation { pub process:usize,pub thread:usize,pub process_id:u32,pub thread_id:u32 }

#[repr(C)]
struct CriticalSection { debug_info:*mut c_void, lock_count:i32, recursion_count:i32, owning_thread:usize, lock_semaphore:usize, spin_count:usize }

pub fn init() {
    unsafe {
        if PROCESS_HEAP == 0 { PROCESS_HEAP = 1; }
    }
    let _ = ALLOCS.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let _ = HEAP_ALLOCS.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = HANDLES.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = SYNCS.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = SYNC_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = CRITICALS.get_or_init(|| Mutex::new(HashMap::new()));
    let _ = TLS_SLOTS.get_or_init(|| Mutex::new(vec![0; TLS_LIMIT]));
    let _ = CLOCK_START.get_or_init(std::time::Instant::now);
}

pub extern "win64" fn get_std_handle(which: u32) -> usize {
    match which { x if x == (-10i32 as u32) => (-10isize) as usize, x if x == (-11i32 as u32) => (-11isize) as usize, x if x == (-12i32 as u32) => (-12isize) as usize, _ => 0 }
}

fn next_handle() -> usize { NEXT_HANDLE.fetch_add(1, Ordering::Relaxed) }
fn io_error_code(e: &std::io::Error) -> u32 {
    match e.kind() { std::io::ErrorKind::NotFound => 2, std::io::ErrorKind::PermissionDenied => 5,
        std::io::ErrorKind::AlreadyExists => 183, std::io::ErrorKind::InvalidInput => 87,
        std::io::ErrorKind::NotADirectory => 267, _ => 31 }
}
unsafe fn bytes<'a>(p: *const u8, len: usize) -> Option<&'a [u8]> {
    if len != 0 && p.is_null() { None } else if len == 0 { Some(&[]) } else { Some(std::slice::from_raw_parts(p, len)) }
}
unsafe fn bytes_mut<'a>(p: *mut u8, len: usize) -> Option<&'a mut [u8]> {
    if len != 0 && p.is_null() { None } else if len == 0 { Some(std::slice::from_raw_parts_mut(std::ptr::NonNull::<u8>::dangling().as_ptr(), 0)) } else { Some(std::slice::from_raw_parts_mut(p, len)) }
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> { m.lock().unwrap_or_else(std::sync::PoisonError::into_inner) }
fn sync_gate()->std::sync::MutexGuard<'static,()>{lock(SYNC_GATE.get_or_init(||Mutex::new(())))}

pub unsafe extern "win64" fn write_file(h: usize, buf: *const u8, len: u32, written: *mut u32, _ov: usize) -> i32 {
    if !written.is_null() { *written = 0; }
    let Some(s) = bytes(buf, len as usize) else { crate::process::set_last_error(87); return 0; };
    if h == (-11isize) as usize {
        return match std::io::stdout().write_all(s) { Ok(()) => { if !written.is_null(){*written=len;} 1 }, Err(e) => { crate::process::set_last_error(io_error_code(&e)); 0 } };
    }
    if h == (-12isize) as usize {
        return match std::io::stderr().write_all(s) { Ok(()) => { if !written.is_null(){*written=len;} 1 }, Err(e) => { crate::process::set_last_error(io_error_code(&e)); 0 } };
    }
    if h == (-10isize) as usize { crate::process::set_last_error(6); return 0; }
    let mut handles = lock(HANDLES.get_or_init(|| Mutex::new(HashMap::new())));
    let Some(FileHandle::File(f)) = handles.get_mut(&h) else { crate::process::set_last_error(6); return 0; };
    match f.write(s) { Ok(n) => { if !written.is_null(){*written=n as u32;} 1 }, Err(e) => { crate::process::set_last_error(io_error_code(&e)); 0 } }
}
pub unsafe extern "win64" fn read_file(h:usize,buf:*mut u8,len:u32,read:*mut u32,_ov:usize)->i32 {
    if !read.is_null(){*read=0;}
    let Some(b)=bytes_mut(buf,len as usize) else {crate::process::set_last_error(87);return 0;};
    if h==(-10isize) as usize {return match std::io::stdin().read(b){Ok(n)=>{if !read.is_null(){*read=n as u32;}1},Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}};}
    let mut handles=lock(HANDLES.get_or_init(||Mutex::new(HashMap::new())));
    let Some(FileHandle::File(f))=handles.get_mut(&h) else {crate::process::set_last_error(6);return 0;};
    match f.read(b){Ok(n)=>{if !read.is_null(){*read=n as u32;}1},Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}
}
pub unsafe extern "win64" fn create_file_a(path:*const u8,access:u32,_share:u32,_sec:usize,creation:u32,_flags:u32,_template:usize)->usize {
    if path.is_null(){crate::process::set_last_error(87);return usize::MAX;}
    let p=crate::paths::to_linux(&crate::dlls::cstr(path));
    let existed=p.exists();
    let want_read=access&0x8000_0000!=0 || access&1!=0;
    let want_write=access&0x4000_0000!=0 || access&2!=0;
    let mut o=OpenOptions::new();o.read(want_read||!want_write).write(want_write);
    match creation {1=>{o.create_new(true);},2=>{o.create(true).truncate(true);},3=>{},4=>{o.create(true);},5=>{o.truncate(true);},_=>{crate::process::set_last_error(87);return usize::MAX;}}
    let f=match o.open(p){Ok(f)=>f,Err(e)=>{crate::process::set_last_error(io_error_code(&e));return usize::MAX;}};
    if (creation==2||creation==4)&&existed{crate::process::set_last_error(183);}
    let h=next_handle();lock(HANDLES.get_or_init(||Mutex::new(HashMap::new()))).insert(h,FileHandle::File(f));h
}
pub unsafe extern "win64" fn get_file_size(h:usize,high:*mut u32)->u32 {
    let handles=lock(HANDLES.get_or_init(||Mutex::new(HashMap::new())));
    let Some(FileHandle::File(f))=handles.get(&h) else {crate::process::set_last_error(6);return u32::MAX;};
    match f.metadata(){Ok(m)=>{let n=m.len();if !high.is_null(){*high=(n>>32) as u32;}n as u32},Err(e)=>{crate::process::set_last_error(io_error_code(&e));u32::MAX}}
}
pub unsafe extern "win64" fn set_file_pointer(h:usize,distance:i32,high:*mut i32,method:u32)->u32 {
    let mut handles=lock(HANDLES.get_or_init(||Mutex::new(HashMap::new())));
    let Some(FileHandle::File(f))=handles.get_mut(&h) else {crate::process::set_last_error(6);return u32::MAX;};
    let d=if high.is_null(){distance as i64}else{(((*high as u32 as u64)<<32)|(distance as u32 as u64)) as i64};
    let from=match method{0 if d>=0=>SeekFrom::Start(d as u64),0=>{crate::process::set_last_error(87);return u32::MAX;},1=>SeekFrom::Current(d),2=>SeekFrom::End(d),_=>{crate::process::set_last_error(87);return u32::MAX;}};
    match f.seek(from){Ok(n)=>{if !high.is_null(){*high=(n>>32) as i32;}n as u32},Err(e)=>{crate::process::set_last_error(io_error_code(&e));u32::MAX}}
}
pub extern "win64" fn flush_file_buffers(h:usize)->i32 {
    let mut handles=lock(HANDLES.get_or_init(||Mutex::new(HashMap::new())));
    let Some(FileHandle::File(f))=handles.get_mut(&h) else {crate::process::set_last_error(6);return 0;};
    match f.sync_all(){Ok(())=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}
}
pub extern "win64" fn exit_process(code: u32) -> ! { flush_and_exit(code as i32) }
pub extern "win64" fn get_command_line_a() -> usize { crate::process::PROCESS.get().map(|p| p.command_line_a.as_ptr() as usize).unwrap_or(0) }
pub extern "win64" fn get_command_line_w() -> usize { crate::process::PROCESS.get().map(|p| p.command_line_w.as_ptr() as usize).unwrap_or(0) }
pub unsafe extern "win64" fn get_module_handle_a(name: *const u8) -> usize {
    if name.is_null() { return crate::loader::main_module().map(|m| m.base).unwrap_or(0); }
    let n = crate::dlls::cstr(name);
    crate::dlls::module_handle(&n).unwrap_or(0)
}
pub unsafe extern "win64" fn get_module_handle_w(name:*const u16)->usize{if name.is_null(){return get_module_handle_a(std::ptr::null());}let s=match cstring_from_wide(name){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};get_module_handle_a(s.as_ptr()as *const u8)}
pub unsafe extern "win64" fn get_proc_address(module: usize, name: *const u8) -> usize {
    if name.is_null(){crate::process::set_last_error(87);return 0;}
    let by_ordinal=(name as usize)<=0xffff;
    if let Some(dll)=crate::dlls::builtin_by_handle(module){if by_ordinal{crate::process::set_last_error(127);return 0;}return crate::dlls::lookup(dll,&crate::dlls::cstr(name)).unwrap_or_else(||{crate::process::set_last_error(127);0});}
    let Some(m)=crate::loader::find_module_by_addr(module).filter(|m|m.base==module)else{crate::process::set_last_error(6);return 0;};
    let found=if by_ordinal{crate::loader::export_by_ordinal(&m,name as usize as u32)}else{crate::loader::export_by_name(&m,&crate::dlls::cstr(name))};
    found.unwrap_or_else(||{crate::process::set_last_error(127);0})
}
pub unsafe extern "win64" fn load_library_a(name:*const u8)->usize{
    if name.is_null(){crate::process::set_last_error(87);return 0;}let n=crate::dlls::cstr(name);if n.is_empty(){crate::process::set_last_error(87);return 0;}
    if let Some(h)=crate::dlls::module_handle(&n){return h;}
    let Some(importer)=crate::loader::main_module()else{crate::process::set_last_error(126);return 0;};
    let Some(path)=crate::loader::find_dll_file(&n,&importer.path)else{crate::process::set_last_error(126);return 0;};
    match crate::loader::load(&path,false){Ok(m)=>if crate::process::initialize_dynamic_module(&m){m.base}else{crate::process::set_last_error(1114);0},Err(_)=>{crate::process::set_last_error(126);0}}
}
pub unsafe extern "win64" fn load_library_w(name:*const u16)->usize{let s=match cstring_from_wide(name){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};load_library_a(s.as_ptr()as *const u8)}
pub extern "win64" fn free_library(module:usize)->i32{if crate::dlls::builtin_by_handle(module).is_some(){return 1;}if crate::loader::find_module_by_addr(module).is_some_and(|m|m.base==module){1}else{crate::process::set_last_error(6);0}}
pub extern "win64" fn get_current_process()->usize{(-1isize)as usize}
pub extern "win64" fn get_current_thread()->usize{(-2isize)as usize}
pub extern "win64" fn get_current_thread_id()->u32{let t=crate::process::teb();if t!=0{unsafe{*((t+crate::process::TEB_TID)as *const usize)as u32}}else{unsafe{libc::syscall(libc::SYS_gettid)as u32}}}
pub unsafe extern "win64" fn get_system_info(out:*mut SystemInfo){if out.is_null(){crate::process::set_last_error(87);return;}let n=std::thread::available_parallelism().map(|x|x.get()).unwrap_or(1).max(1);let mask=if n>=usize::BITS as usize{usize::MAX}else{(1usize<<n)-1};std::ptr::write(out,SystemInfo{processor_architecture:9,reserved:0,page_size:4096,minimum_application_address:0x10000usize as *mut c_void,maximum_application_address:0x0000_7fff_ffff_efffu64 as usize as *mut c_void,active_processor_mask:mask,number_of_processors:n as u32,processor_type:586,allocation_granularity:65536,processor_level:6,processor_revision:0x3a09});}
pub unsafe extern "win64" fn get_native_system_info(out:*mut SystemInfo){get_system_info(out)}
fn filetime_now()->u64{const EPOCH:u64=116_444_736_000_000_000;let d=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();EPOCH.saturating_add(d.as_secs().saturating_mul(10_000_000)).saturating_add((d.subsec_nanos()/100)as u64)}
pub unsafe extern "win64" fn get_system_time_as_file_time(out:*mut u64){if out.is_null(){crate::process::set_last_error(87);return;}*out=filetime_now()}
pub extern "win64" fn get_version()->u32{10u32|(19045u32<<16)}
pub unsafe extern "win64" fn get_version_ex_a(info:*mut u8)->i32{if info.is_null(){crate::process::set_last_error(87);return 0;}let size=std::ptr::read_unaligned(info as *const u32);if size<148{crate::process::set_last_error(87);return 0;}std::ptr::write_unaligned(info.add(4)as *mut u32,10);std::ptr::write_unaligned(info.add(8)as *mut u32,0);std::ptr::write_unaligned(info.add(12)as *mut u32,19045);std::ptr::write_unaligned(info.add(16)as *mut u32,2);std::ptr::write_bytes(info.add(20),0,128);1}
pub unsafe extern "win64" fn get_version_ex_w(info:*mut u16)->i32{if info.is_null(){crate::process::set_last_error(87);return 0;}let size=std::ptr::read_unaligned(info as *const u32);if size<276{crate::process::set_last_error(87);return 0;}std::ptr::write_unaligned(info.add(2)as *mut u32,10);std::ptr::write_unaligned(info.add(4)as *mut u32,0);std::ptr::write_unaligned(info.add(6)as *mut u32,19045);std::ptr::write_unaligned(info.add(8)as *mut u32,2);std::ptr::write_bytes(info.add(10),0,128);1}
pub extern "win64" fn get_last_error() -> u32 { crate::process::last_error() }
pub extern "win64" fn set_last_error(code: u32) { crate::process::set_last_error(code) }
pub extern "win64" fn get_process_heap() -> usize { unsafe { PROCESS_HEAP } }
pub unsafe extern "win64" fn heap_alloc(_heap: usize, flags: u32, size: usize) -> *mut c_void {
    let p = libc::malloc(size.max(1));
    if !p.is_null() {if flags & 8 != 0 { std::ptr::write_bytes(p, 0, size); }lock(HEAP_ALLOCS.get_or_init(||Mutex::new(HashMap::new()))).insert(p as usize,size.max(1));}
    p
}
pub unsafe extern "win64" fn heap_realloc(_heap:usize,flags:u32,p:*mut c_void,size:usize)->*mut c_void{
    if p.is_null(){return heap_alloc(0,flags,size);}
    if flags&0x10!=0{crate::process::set_last_error(87);return std::ptr::null_mut();}
    let old=lock(HEAP_ALLOCS.get_or_init(||Mutex::new(HashMap::new()))).get(&(p as usize)).copied();let Some(old)=old else{crate::process::set_last_error(87);return std::ptr::null_mut();};
    let q=libc::realloc(p,size.max(1));if q.is_null(){return q;}
    let mut allocations=lock(HEAP_ALLOCS.get_or_init(||Mutex::new(HashMap::new())));allocations.remove(&(p as usize));allocations.insert(q as usize,size.max(1));
    if flags&8!=0&&size>old{std::ptr::write_bytes((q as *mut u8).add(old),0,size-old);}q
}
pub unsafe extern "win64" fn heap_free(_heap: usize, _flags: u32, p: *mut c_void) -> i32 {if p.is_null(){crate::process::set_last_error(87);return 0;}if lock(HEAP_ALLOCS.get_or_init(||Mutex::new(HashMap::new()))).remove(&(p as usize)).is_none(){crate::process::set_last_error(87);return 0;}libc::free(p);1 }
pub unsafe extern "win64" fn virtual_alloc(addr: *mut c_void, size: usize, _kind: u32, prot: u32) -> *mut c_void {
    const MEM_COMMIT:u32=0x1000;const MEM_RESERVE:u32=0x2000;const MEM_RESET:u32=0x80000;
    const PAGE:usize=4096;const GRANULARITY:usize=65536;
    if size==0||!valid_win_prot(prot)||_kind&!(MEM_COMMIT|MEM_RESERVE|MEM_RESET)!=0||_kind&(MEM_COMMIT|MEM_RESERVE|MEM_RESET)==0||(_kind&MEM_RESET!=0&&_kind!=MEM_RESET) {crate::process::set_last_error(87);return std::ptr::null_mut();}
    let Some(request_end)=(addr as usize).checked_add(size) else {crate::process::set_last_error(87);return std::ptr::null_mut();};
    if _kind==MEM_RESET {
        let Some(end0)=request_end.checked_add(PAGE-1)else{crate::process::set_last_error(87);return std::ptr::null_mut();};let base=addr as usize&!(PAGE-1);let end=end0&!(PAGE-1);let mut maps=lock(ALLOCS.get_or_init(||Mutex::new(HashMap::new())));
        let Some((&region_base,_r))=maps.iter_mut().find(|(b,r)|base>=**b&&end<=**b+r.size)else{crate::process::set_last_error(487);return std::ptr::null_mut();};
        if libc::madvise(base as *mut _,end-base,libc::MADV_DONTNEED)!=0{crate::process::set_last_error(8);return std::ptr::null_mut();}
        let _=region_base;return base as *mut c_void;
    }
    if _kind&MEM_RESERVE==0 {
        if addr.is_null(){return virtual_alloc(std::ptr::null_mut(),size,MEM_RESERVE|MEM_COMMIT,prot);}
        let Some(page_end0)=request_end.checked_add(PAGE-1)else{crate::process::set_last_error(87);return std::ptr::null_mut();};let page_base=addr as usize&!(PAGE-1);let page_end=page_end0&!(PAGE-1);
        let mut maps=lock(ALLOCS.get_or_init(||Mutex::new(HashMap::new())));
        let Some((&region_base,r))=maps.iter_mut().find(|(b,r)|page_base>=**b&&page_end<=**b+r.size)else{crate::process::set_last_error(487);return std::ptr::null_mut();};
        let start=(page_base-region_base)/PAGE;let count=(page_end-page_base)/PAGE;
        if libc::mprotect(page_base as *mut _,page_end-page_base,win_prot(prot))!=0{crate::process::set_last_error(8);return std::ptr::null_mut();}
        r.committed[start..start+count].fill(true);r.protection[start..start+count].fill(prot);return page_base as *mut c_void;
    }
    let page_base=if addr.is_null(){0}else{(addr as usize)&!(GRANULARITY-1)};
    let offset=if addr.is_null(){0}else{(addr as usize)-page_base};
    let Some(size)=size.checked_add(offset).and_then(|n|n.checked_add(PAGE-1)).map(|n|n&!(PAGE-1))else{crate::process::set_last_error(87);return std::ptr::null_mut();};
    let initial_prot=if _kind&MEM_COMMIT!=0{win_prot(prot)}else{libc::PROT_NONE};
    let (p,mapped_size)=if addr.is_null(){
        let Some(reserve)=size.checked_add(GRANULARITY)else{crate::process::set_last_error(8);return std::ptr::null_mut();};
        let raw=libc::mmap(std::ptr::null_mut(),reserve,libc::PROT_NONE,libc::MAP_PRIVATE|libc::MAP_ANONYMOUS,-1,0);
        if raw==libc::MAP_FAILED{crate::process::set_last_error(8);return std::ptr::null_mut();}
        let aligned=((raw as usize)+GRANULARITY-1)&!(GRANULARITY-1);let prefix=aligned-raw as usize;let suffix=reserve-prefix-size;
        if prefix>0{libc::munmap(raw,prefix);}if suffix>0{libc::munmap((aligned+size)as *mut c_void,suffix);}
        (aligned,size)
    }else{
        let p=libc::mmap(page_base as *mut c_void,size,libc::PROT_NONE,libc::MAP_PRIVATE|libc::MAP_ANONYMOUS|libc::MAP_FIXED_NOREPLACE,-1,0);
        if p==libc::MAP_FAILED{crate::process::set_last_error(487);return std::ptr::null_mut();}(p as usize,size)
    };
    if _kind&MEM_COMMIT!=0&&libc::mprotect(p as *mut c_void,mapped_size,initial_prot)!=0{libc::munmap(p as *mut c_void,mapped_size);crate::process::set_last_error(8);return std::ptr::null_mut();}
    let pages=mapped_size/PAGE;lock(ALLOCS.get_or_init(||Mutex::new(HashMap::new()))).insert(p,VmRegion{size:mapped_size,committed:vec![_kind&MEM_COMMIT!=0;pages],protection:vec![if _kind&MEM_COMMIT!=0{prot}else{1};pages],allocation_protection:prot});p as *mut c_void
}
pub unsafe extern "win64" fn virtual_free(addr: *mut c_void, size: usize, kind: u32) -> i32 {
    const MEM_DECOMMIT:u32=0x4000;const MEM_RELEASE:u32=0x8000;const PAGE:usize=4096;
    if addr.is_null(){crate::process::set_last_error(487);return 0;}
    let mut maps=lock(ALLOCS.get_or_init(||Mutex::new(HashMap::new())));let p=addr as usize;
    if kind==MEM_RELEASE {
        if size!=0{crate::process::set_last_error(87);return 0;}
        let Some(region)=maps.get(&p)else{crate::process::set_last_error(487);return 0;};
        if libc::munmap(p as *mut c_void,region.size)!=0{crate::process::set_last_error(8);return 0;}
        maps.remove(&p);return 1;
    }
    if kind==MEM_DECOMMIT {
        if size==0{crate::process::set_last_error(87);return 0;}
        let start=p&!(PAGE-1);let Some(end_raw)=p.checked_add(size)else{crate::process::set_last_error(87);return 0;};let Some(end0)=end_raw.checked_add(PAGE-1)else{crate::process::set_last_error(87);return 0;};let end=end0&!(PAGE-1);
        let Some((&base,region))=maps.iter_mut().find(|(b,r)|start>=**b&&end<=**b+r.size)else{crate::process::set_last_error(487);return 0;};
        if libc::mprotect(start as *mut c_void,end-start,libc::PROT_NONE)!=0||libc::madvise(start as *mut c_void,end-start,libc::MADV_DONTNEED)!=0{crate::process::set_last_error(8);return 0;}
        let first=(start-base)/PAGE;let count=(end-start)/PAGE;region.committed[first..first+count].fill(false);region.protection[first..first+count].fill(1);return 1;
    }
    crate::process::set_last_error(87);0
}
fn valid_win_prot(p:u32)->bool{matches!(p,0x01|0x02|0x04|0x08|0x10|0x20|0x40|0x80)}
fn win_prot(p: u32) -> i32 { match p { 0x01=>libc::PROT_NONE,0x02=>libc::PROT_READ,0x04|0x08=>libc::PROT_READ|libc::PROT_WRITE,0x10=>libc::PROT_EXEC,0x20=>libc::PROT_EXEC|libc::PROT_READ,0x40|0x80=>libc::PROT_EXEC|libc::PROT_READ|libc::PROT_WRITE,_=>libc::PROT_NONE } }
pub unsafe extern "win64" fn virtual_protect(addr:*mut c_void,size:usize,prot:u32,old:*mut u32)->i32{
    const PAGE:usize=4096;if addr.is_null()||size==0||old.is_null()||!valid_win_prot(prot){crate::process::set_last_error(87);return 0;}
    let start=addr as usize&!(PAGE-1);let Some(end_raw)=(addr as usize).checked_add(size)else{crate::process::set_last_error(87);return 0;};let Some(end0)=end_raw.checked_add(PAGE-1)else{crate::process::set_last_error(87);return 0;};let end=end0&!(PAGE-1);
    let mut maps=lock(ALLOCS.get_or_init(||Mutex::new(HashMap::new())));let Some((&base,r))=maps.iter_mut().find(|(b,r)|start>=**b&&end<=**b+r.size)else{crate::process::set_last_error(487);return 0;};
    let first=(start-base)/PAGE;let count=(end-start)/PAGE;if r.committed[first..first+count].iter().any(|c|!*c){crate::process::set_last_error(487);return 0;}
    let previous=r.protection[first];if libc::mprotect(start as *mut c_void,end-start,win_prot(prot))!=0{crate::process::set_last_error(8);return 0;}*old=previous;r.protection[first..first+count].fill(prot);1
}
pub unsafe extern "win64" fn virtual_query(addr:*const c_void,info:*mut MemoryBasicInformation,length:usize)->usize{
    const PAGE:usize=4096;const MEM_COMMIT:u32=0x1000;const MEM_RESERVE:u32=0x2000;const MEM_FREE:u32=0x10000;const MEM_PRIVATE:u32=0x20000;
    let required=std::mem::size_of::<MemoryBasicInformation>();if info.is_null(){crate::process::set_last_error(87);return 0;}if length<required{crate::process::set_last_error(122);return 0;}
    let address=addr as usize;let page=address&!(PAGE-1);let maps=lock(ALLOCS.get_or_init(||Mutex::new(HashMap::new())));
    let region=maps.iter().find(|(base,r)|address>=**base&&address<**base+r.size);
    let result=if let Some((&base,r))=region{
        let idx=(page-base)/PAGE;let committed=r.committed[idx];let protect=if committed{r.protection[idx]}else{1};let mut first=idx;let mut last=idx+1;
        while first>0&&r.committed[first-1]==committed&&(!committed||r.protection[first-1]==protect){first-=1;}
        while last<r.committed.len()&&r.committed[last]==committed&&(!committed||r.protection[last]==protect){last+=1;}
        MemoryBasicInformation{base_address:(base+first*PAGE)as *mut c_void,allocation_base:base as *mut c_void,allocation_protect:r.allocation_protection,partition_id:0,region_size:(last-first)*PAGE,state:if committed{MEM_COMMIT}else{MEM_RESERVE},protect,kind:MEM_PRIVATE}
    }else{
        let next=maps.keys().copied().filter(|b|*b>page).min().unwrap_or(0x0000_7fff_ffff_ffffusize);let len=next.saturating_sub(page).max(PAGE);
        MemoryBasicInformation{base_address:page as *mut c_void,allocation_base:std::ptr::null_mut(),allocation_protect:0,partition_id:0,region_size:len,state:MEM_FREE,protect:1,kind:0}
    };
    std::ptr::write_unaligned(info,result);required
}
pub unsafe extern "win64" fn virtual_query_ex(process:usize,addr:*const c_void,info:*mut MemoryBasicInformation,length:usize)->usize{if process!=get_current_process(){crate::process::set_last_error(6);return 0;}virtual_query(addr,info,length)}
pub extern "win64" fn flush_instruction_cache(_process:usize,_addr:*const c_void,_size:usize)->i32{1}
pub extern "win64" fn get_tick_count() -> u32 { CLOCK_START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u32 }
pub extern "win64" fn get_tick_count64() -> u64 { CLOCK_START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 }
pub unsafe extern "win64" fn get_current_process_id() -> u32 { libc::getpid() as u32 }
fn same_sync(a:&SyncHandle,b:&SyncHandle)->bool { match(a,b){(SyncHandle::Event(x),SyncHandle::Event(y))=>Arc::ptr_eq(x,y),(SyncHandle::Mutex(x),SyncHandle::Mutex(y))=>Arc::ptr_eq(x,y),(SyncHandle::Thread(x),SyncHandle::Thread(y))=>Arc::ptr_eq(x,y),(SyncHandle::Process(x),SyncHandle::Process(y))=>Arc::ptr_eq(x,y),_=>false} }
fn add_sync(obj:SyncHandle,name:Option<String>)->usize {
    let obj=if let Some(name)=name { let key=name.to_lowercase();let mut names=lock(SYNC_NAMES.get_or_init(||Mutex::new(HashMap::new())));if let Some((old,count))=names.get_mut(&key){
        let compatible=matches!((&obj,&*old),(SyncHandle::Event(_),SyncHandle::Event(_))|(SyncHandle::Mutex(_),SyncHandle::Mutex(_)));
        if !compatible{crate::process::set_last_error(6);return 0;}
        *count+=1;crate::process::set_last_error(183);old.clone()
    }else{names.insert(key,(obj.clone(),1));obj} }else{obj};
    let h=next_handle();lock(SYNCS.get_or_init(||Mutex::new(HashMap::new()))).insert(h,obj);h
}
fn lookup_sync(h:usize)->Option<SyncHandle>{lock(SYNCS.get_or_init(||Mutex::new(HashMap::new()))).get(&h).cloned()}
fn remove_sync(h:usize)->bool {
    let Some(obj)=lock(SYNCS.get_or_init(||Mutex::new(HashMap::new()))).remove(&h) else{return false;};
    let mut names=lock(SYNC_NAMES.get_or_init(||Mutex::new(HashMap::new())));
    let mut dead=None;for (name,(named,count)) in names.iter_mut(){if same_sync(&obj,named){*count=count.saturating_sub(1);if *count==0{dead=Some(name.clone());}break;}}
    if let Some(name)=dead{names.remove(&name);}true
}
fn named_event(name:*const u8)->Option<String>{if name.is_null(){None}else{let s=unsafe{crate::dlls::cstr(name)};if s.is_empty(){None}else{Some(s)}}}
fn wait_condvar<'a,T>(cv:&Condvar,guard:std::sync::MutexGuard<'a,T>,timeout:u32)->(std::sync::MutexGuard<'a,T>,bool){
    if timeout==u32::MAX { (cv.wait(guard).unwrap_or_else(std::sync::PoisonError::into_inner),false) }
    else { let (g,r)=cv.wait_timeout(guard,std::time::Duration::from_millis(timeout as u64)).unwrap_or_else(std::sync::PoisonError::into_inner);(g,r.timed_out()) }
}
fn wait_event(e:&EventObject,timeout:u32)->u32 {
    let start=std::time::Instant::now();
    loop {let gate=sync_gate();let mut state=lock(&e.signaled);if *state{if !e.manual_reset{*state=false;}return 0;}if timeout==0{return 258;}
        let left=if timeout==u32::MAX{u32::MAX}else{let elapsed=start.elapsed().as_millis().min(u32::MAX as u128)as u32;if elapsed>=timeout{return 258;}timeout-elapsed};drop(gate);
        let(next,_)=wait_condvar(&e.changed,state,left);drop(next);
    }
}
fn wait_mutex(m:&MutexObject,timeout:u32)->u32 {
    let me=std::thread::current().id();let start=std::time::Instant::now();
    loop {let gate=sync_gate();let mut s=lock(&m.state);if s.owner.as_ref()==Some(&me){s.recursion=s.recursion.saturating_add(1);return 0;}if s.owner.is_none(){s.owner=Some(me);s.recursion=1;return 0;}if timeout==0{return 258;}
        let left=if timeout==u32::MAX{u32::MAX}else{let elapsed=start.elapsed().as_millis().min(u32::MAX as u128)as u32;if elapsed>=timeout{return 258;}timeout-elapsed};drop(gate);
        let(next,_)=wait_condvar(&m.changed,s,left);drop(next);
    }
}
fn wait_thread(t:&ThreadObject,timeout:u32)->u32 {
    let start=std::time::Instant::now();loop{let gate=sync_gate();let s=lock(&t.state);if s.finished{return 0;}if timeout==0{return 258;}let left=if timeout==u32::MAX{u32::MAX}else{let elapsed=start.elapsed().as_millis().min(u32::MAX as u128)as u32;if elapsed>=timeout{return 258;}timeout-elapsed};drop(gate);let(next,_)=wait_condvar(&t.changed,s,left);drop(next);}
}
fn poll_process(p:&ProcessObject)->bool{let mut s=lock(&p.state);if let Some(child)=s.child.as_mut(){match child.try_wait(){Ok(Some(status))=>{s.exit_code=status.code().unwrap_or(1)as u32;s.child=None;p.changed.notify_all();},Ok(None)=>{},Err(_)=>{s.exit_code=1;s.child=None;p.changed.notify_all();}}}s.child.is_none()}
fn wait_process(p:&ProcessObject,timeout:u32)->u32{let start=std::time::Instant::now();loop{let gate=sync_gate();if poll_process(p){return 0;}if timeout==0{return 258;}if timeout!=u32::MAX&&start.elapsed().as_millis()>=timeout as u128{return 258;}drop(gate);std::thread::sleep(std::time::Duration::from_millis(2));}}
fn try_mutex(m:&MutexObject)->bool{let _gate=sync_gate();let me=std::thread::current().id();let mut s=lock(&m.state);if s.owner.as_ref()==Some(&me){s.recursion=s.recursion.saturating_add(1);true}else if s.owner.is_none(){s.owner=Some(me);s.recursion=1;true}else{false}}
fn leave_mutex(m:&MutexObject)->bool{let _gate=sync_gate();let me=std::thread::current().id();let mut s=lock(&m.state);if s.owner.as_ref()!=Some(&me){crate::process::set_last_error(288);return false;}if s.recursion>1{s.recursion-=1;}else{s.recursion=0;s.owner=None;m.changed.notify_one();}true}
fn critical_object(cs:usize)->Option<Arc<MutexObject>>{lock(CRITICALS.get_or_init(||Mutex::new(HashMap::new()))).get(&cs).cloned()}
unsafe fn update_critical_fields(cs:*mut c_void,m:&MutexObject){let s=lock(&m.state);let p=cs as *mut CriticalSection;(*p).lock_count=if s.owner.is_some(){0}else{-1};(*p).recursion_count=s.recursion.min(i32::MAX as usize)as i32;(*p).owning_thread=s.owner.as_ref().map_or(0,|_|get_current_thread_id()as usize);}
pub unsafe extern "win64" fn initialize_critical_section(cs:*mut c_void){if cs.is_null(){crate::process::set_last_error(87);return;}let p=cs as *mut CriticalSection;std::ptr::write(p,CriticalSection{debug_info:std::ptr::null_mut(),lock_count:-1,recursion_count:0,owning_thread:0,lock_semaphore:0,spin_count:0});lock(CRITICALS.get_or_init(||Mutex::new(HashMap::new()))).insert(cs as usize,Arc::new(MutexObject{state:Mutex::new(MutexState{owner:None,recursion:0}),changed:Condvar::new()}));}
pub unsafe extern "win64" fn initialize_critical_section_and_spin_count(cs:*mut c_void,spin:u32)->i32{initialize_critical_section(cs);if cs.is_null(){return 0;}(*(cs as *mut CriticalSection)).spin_count=spin as usize;1}
pub unsafe extern "win64" fn initialize_critical_section_ex(cs:*mut c_void,spin:u32,flags:u32)->i32{if flags&!0x0100_0000!=0{crate::process::set_last_error(87);return 0;}initialize_critical_section_and_spin_count(cs,spin)}
pub unsafe extern "win64" fn enter_critical_section(cs:*mut c_void){let Some(m)=critical_object(cs as usize)else{crate::process::set_last_error(87);return;};let _=wait_mutex(&m,u32::MAX);update_critical_fields(cs,&m);}
pub unsafe extern "win64" fn try_enter_critical_section(cs:*mut c_void)->i32{let Some(m)=critical_object(cs as usize)else{crate::process::set_last_error(87);return 0;};if try_mutex(&m){update_critical_fields(cs,&m);1}else{0}}
pub unsafe extern "win64" fn leave_critical_section(cs:*mut c_void){let Some(m)=critical_object(cs as usize)else{crate::process::set_last_error(87);return;};if leave_mutex(&m){update_critical_fields(cs,&m);}}
pub unsafe extern "win64" fn delete_critical_section(cs:*mut c_void){if cs.is_null(){return;}lock(CRITICALS.get_or_init(||Mutex::new(HashMap::new()))).remove(&(cs as usize));std::ptr::write(cs as *mut CriticalSection,CriticalSection{debug_info:std::ptr::null_mut(),lock_count:0,recursion_count:0,owning_thread:0,lock_semaphore:0,spin_count:0});}
pub unsafe extern "win64" fn create_thread(_attrs: usize, stack: usize, start: usize, param: usize, flags: u32, id: *mut u32) -> usize {
    static NEXT_THREAD: AtomicU32 = AtomicU32::new(1);
    if start==0||flags&!4!=0 {crate::process::set_last_error(if start==0{87}else{50});return 0;}
    let tid=NEXT_THREAD.fetch_add(1,Ordering::Relaxed);let state=Arc::new(ThreadObject{state:Mutex::new(ThreadState{finished:false,exit_code:259,suspend_count:usize::from(flags&4!=0)}),changed:Condvar::new()});
    let handle=add_sync(SyncHandle::Thread(state.clone()),None);let f:extern "win64" fn(usize)->u32=std::mem::transmute(start);
    let mut builder=std::thread::Builder::new().name(format!("win-thread-{tid}"));if stack!=0{builder=builder.stack_size(stack.max(64*1024));}
    if builder.spawn(move||{let mut s=lock(&state.state);while s.suspend_count!=0{s=state.changed.wait(s).unwrap_or_else(std::sync::PoisonError::into_inner);}drop(s);crate::process::thread_attach_current();let code=f(param);crate::process::thread_detach_current();let _gate=sync_gate();let mut s=lock(&state.state);s.exit_code=code;s.finished=true;state.changed.notify_all();}).is_err(){remove_sync(handle);crate::process::set_last_error(8);return 0;}
    if !id.is_null(){*id=tid;}handle
}
fn split_windows_command_line(line:&str)->Vec<String>{
    let mut out=Vec::new();let mut arg=String::new();let mut chars=line.chars().peekable();let mut quoted=false;
    while let Some(c)=chars.next(){
        if c=='\\'{let mut slashes=1usize;while chars.peek()==Some(&'\\'){chars.next();slashes+=1;}if chars.peek()==Some(&'"'){chars.next();arg.extend(std::iter::repeat('\\').take(slashes/2));if slashes%2==1{arg.push('"')}else{quoted=!quoted;}}else{arg.extend(std::iter::repeat('\\').take(slashes));}continue;}
        if c=='"'{quoted=!quoted;continue;}if c.is_whitespace()&&!quoted{if !arg.is_empty(){out.push(std::mem::take(&mut arg));}continue;}arg.push(c);
    }
    if !arg.is_empty(){out.push(arg);}out
}
fn process_error(e:&std::io::Error)->u32{io_error_code(e)}
unsafe fn create_process_common(application:Option<String>,command_line:*const u8,inherit_handles:i32,creation_flags:u32,environment:*const c_void,current_directory:*const u8,startup:*const StartupInfoA,info:*mut ProcessInformation)->i32{
    if info.is_null(){crate::process::set_last_error(87);return 0;}
    if !environment.is_null(){crate::process::set_last_error(120);return 0;}
    if creation_flags&!0x0800_0000!=0{crate::process::set_last_error(50);return 0;}
    if !startup.is_null()&&(*startup).cb < std::mem::size_of::<StartupInfoA>() as u32{crate::process::set_last_error(87);return 0;}
    let line=if command_line.is_null(){String::new()}else{crate::dlls::cstr(command_line)};
    let mut args=split_windows_command_line(&line);
    let exe=if let Some(app)=application{if !args.is_empty(){args.remove(0);}app}else if !args.is_empty(){args.remove(0)}else{crate::process::set_last_error(87);return 0;};
    let mut host_path=crate::paths::to_linux(&exe);
    if !host_path.exists()&&host_path.extension().is_none(){let mut x=host_path.as_os_str().to_os_string();x.push(".exe");let p=std::path::PathBuf::from(x);if p.exists(){host_path=p;}}
    let is_pe=host_path.extension().is_some_and(|e|e.to_string_lossy().eq_ignore_ascii_case("exe"));
    let mut cmd=if is_pe{let runtime=match std::env::current_exe(){Ok(p)=>p,Err(e)=>{crate::process::set_last_error(process_error(&e));return 0;}};let mut c=Command::new(runtime);c.arg(&host_path);c}else{Command::new(&host_path)};
    cmd.args(args.iter());
    if !current_directory.is_null(){let cwd=crate::paths::to_linux(&crate::dlls::cstr(current_directory));cmd.current_dir(cwd);}
    if inherit_handles!=0{crate::process::set_last_error(120);return 0;}
    let child=match cmd.spawn(){Ok(c)=>c,Err(e)=>{crate::process::set_last_error(process_error(&e));return 0;}};
    let pid=child.id();let object=Arc::new(ProcessObject{state:Mutex::new(ProcessState{child:Some(child),exit_code:259}),changed:Condvar::new()});
    let process_handle=add_sync(SyncHandle::Process(object.clone()),None);let thread_handle=add_sync(SyncHandle::Process(object),None);
    std::ptr::write(info,ProcessInformation{process:process_handle,thread:thread_handle,process_id:pid,thread_id:pid});1
}
pub unsafe extern "win64" fn create_process_a(application:*const u8,command_line:*const u8,process_attributes:usize,thread_attributes:usize,inherit:i32,flags:u32,environment:*const c_void,current_directory:*const u8,startup:*const StartupInfoA,info:*mut ProcessInformation)->i32{
    if application.is_null()&&command_line.is_null(){crate::process::set_last_error(87);return 0;}
    let app=if application.is_null(){None}else{Some(crate::dlls::cstr(application))};
    let _=(process_attributes,thread_attributes);create_process_common(app,command_line,inherit,flags,environment,current_directory,startup,info)
}
pub unsafe extern "win64" fn create_process_w(application:*const u16,command_line:*mut u16,process_attributes:usize,thread_attributes:usize,inherit:i32,flags:u32,environment:*const c_void,current_directory:*const u16,startup:*const StartupInfoA,info:*mut ProcessInformation)->i32{
    let app=if application.is_null(){None}else{let s=match cstring_from_wide(application){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};Some(s)};
    let line=if command_line.is_null(){None}else{let n=crate::printf::wide_len(command_line);let s=match String::from_utf16(std::slice::from_raw_parts(command_line,n)){Ok(s)=>s,Err(_)=>{crate::process::set_last_error(1113);return 0;}};let bytes=crate::dlls::encode_ansi(&s);match std::ffi::CString::new(bytes){Ok(c)=>Some(c),Err(_)=>{crate::process::set_last_error(87);return 0;}}};
    if app.is_none()&&line.is_none(){crate::process::set_last_error(87);return 0;}
    let cwd=if current_directory.is_null(){None}else{match cstring_from_wide(current_directory){Ok(s)=>Some(s),Err(e)=>{crate::process::set_last_error(e);return 0;}}};
    let _=(process_attributes,thread_attributes);
    create_process_a(app.as_ref().map_or(std::ptr::null(),|s|s.as_ptr()as *const u8),line.as_ref().map_or(std::ptr::null(),|s|s.as_ptr()as *const u8),0,0,inherit,flags,environment,cwd.as_ref().map_or(std::ptr::null(),|s|s.as_ptr()as *const u8),startup,info)
}
pub unsafe extern "win64" fn open_process(_access:u32,_inherit:i32,pid:u32)->usize{if pid==0{crate::process::set_last_error(87);return 0;}if pid==libc::getpid() as u32{return get_current_process();}crate::process::set_last_error(5);0}
pub unsafe extern "win64" fn get_exit_code_process(handle:usize,code:*mut u32)->i32{if code.is_null(){crate::process::set_last_error(87);return 0;}let Some(SyncHandle::Process(p))=lookup_sync(handle)else{crate::process::set_last_error(6);return 0;};poll_process(&p);*code=lock(&p.state).exit_code;1}
pub unsafe extern "win64" fn terminate_process(handle:usize,code:u32)->i32{let Some(SyncHandle::Process(p))=lookup_sync(handle)else{crate::process::set_last_error(6);return 0;};let mut s=lock(&p.state);let Some(child)=s.child.as_mut()else{crate::process::set_last_error(5);return 0;};if child.kill().is_err(){crate::process::set_last_error(5);return 0;}if let Some(mut child)=s.child.take(){let _=child.wait();}s.exit_code=code;p.changed.notify_all();1}
pub unsafe extern "win64" fn get_exit_code_thread(handle:usize,code:*mut u32)->i32{if code.is_null(){crate::process::set_last_error(87);return 0;}match lookup_sync(handle){Some(SyncHandle::Thread(t))=>{let s=lock(&t.state);*code=if s.finished{s.exit_code}else{259};1},Some(SyncHandle::Process(p))=>{poll_process(&p);*code=lock(&p.state).exit_code;1},_=>{crate::process::set_last_error(6);0}}}
pub extern "win64" fn resume_thread(handle:usize)->u32{let Some(SyncHandle::Thread(t))=lookup_sync(handle)else{crate::process::set_last_error(6);return u32::MAX;};let _gate=sync_gate();let mut s=lock(&t.state);if s.finished{crate::process::set_last_error(6);return u32::MAX;}let old=s.suspend_count;if old>0{s.suspend_count-=1;if s.suspend_count==0{t.changed.notify_all();}}old.min(u32::MAX as usize) as u32}
pub extern "win64" fn close_handle(handle: usize) -> i32 { if handle==1||handle==2||handle==(-10isize)as usize||handle==(-11isize)as usize||handle==(-12isize)as usize{crate::process::set_last_error(6);return 0;}if lock(HANDLES.get_or_init(||Mutex::new(HashMap::new()))).remove(&handle).is_some(){1}else if remove_sync(handle){1}else{crate::process::set_last_error(6);0} }
pub unsafe extern "win64" fn create_event_a(_sec:usize,manual:i32,initial:i32,name:*const u8)->usize{let obj=SyncHandle::Event(Arc::new(EventObject{manual_reset:manual!=0,signaled:Mutex::new(initial!=0),changed:Condvar::new()}));add_sync(obj,named_event(name))}
pub extern "win64" fn set_event(h:usize)->i32{let Some(SyncHandle::Event(v))=lookup_sync(h)else{crate::process::set_last_error(6);return 0;};let _gate=sync_gate();*lock(&v.signaled)=true;if v.manual_reset{v.changed.notify_all()}else{v.changed.notify_one()}1}
pub extern "win64" fn reset_event(h:usize)->i32{let Some(SyncHandle::Event(v))=lookup_sync(h)else{crate::process::set_last_error(6);return 0;};let _gate=sync_gate();*lock(&v.signaled)=false;1}
pub unsafe extern "win64" fn create_mutex_a(_sec:usize,initial:i32,name:*const u8)->usize{let obj=SyncHandle::Mutex(Arc::new(MutexObject{state:Mutex::new(MutexState{owner:if initial!=0{Some(std::thread::current().id())}else{None},recursion:usize::from(initial!=0)}),changed:Condvar::new()}));add_sync(obj,named_event(name))}
pub extern "win64" fn release_mutex(h:usize)->i32{let Some(SyncHandle::Mutex(m))=lookup_sync(h)else{crate::process::set_last_error(6);return 0;};if leave_mutex(&m){1}else{0}}
pub extern "win64" fn wait_for_single_object(h:usize,timeout:u32)->u32{let Some(obj)=lookup_sync(h)else{crate::process::set_last_error(6);return u32::MAX;};match obj{SyncHandle::Event(e)=>wait_event(&e,timeout),SyncHandle::Mutex(m)=>wait_mutex(&m,timeout),SyncHandle::Thread(t)=>wait_thread(&t,timeout),SyncHandle::Process(p)=>wait_process(&p,timeout)}}
pub unsafe extern "win64" fn wait_for_multiple_objects(count:u32,handles:*const usize,wait_all:i32,timeout:u32)->u32{
    if count==0||count>64||handles.is_null(){crate::process::set_last_error(87);return u32::MAX;}
    let hs=std::slice::from_raw_parts(handles,count as usize);let mut objects=Vec::with_capacity(hs.len());
    for &h in hs{let Some(o)=lookup_sync(h)else{crate::process::set_last_error(6);return u32::MAX;};objects.push(o);}
    let start=std::time::Instant::now();
    if wait_all!=0 {
        for i in 0..objects.len(){if objects[..i].iter().any(|o|same_sync(o,&objects[i])){crate::process::set_last_error(87);return u32::MAX;}}
        loop{
            let gate=sync_gate();let me=std::thread::current().id();
            let ready=objects.iter().all(|o|match o{SyncHandle::Event(e)=>*lock(&e.signaled),SyncHandle::Mutex(m)=>{let s=lock(&m.state);s.owner.is_none()||s.owner.as_ref()==Some(&me)},SyncHandle::Thread(t)=>lock(&t.state).finished,SyncHandle::Process(p)=>poll_process(p)});
            if ready{for o in &objects{match o{SyncHandle::Event(e)=>{let mut signaled=lock(&e.signaled);if !e.manual_reset{*signaled=false;}},SyncHandle::Mutex(m)=>{let mut s=lock(&m.state);if s.owner.as_ref()==Some(&me){s.recursion=s.recursion.saturating_add(1);}else{s.owner=Some(me);s.recursion=1;}},SyncHandle::Thread(_)|SyncHandle::Process(_)=>{}}}return 0;}
            drop(gate);if timeout==0{return 258;}if timeout!=u32::MAX&&start.elapsed().as_millis()>=timeout as u128{return 258;}std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    loop{for (i,obj) in objects.iter().enumerate(){let ready=match obj{SyncHandle::Event(e)=>wait_event(e,0)==0,SyncHandle::Mutex(m)=>wait_mutex(m,0)==0,SyncHandle::Thread(t)=>wait_thread(t,0)==0,SyncHandle::Process(p)=>poll_process(p)};if ready{return i as u32;}}
        if timeout==0{return 258;}if timeout!=u32::MAX&&start.elapsed().as_millis()>=timeout as u128{return 258;}std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn env_value(name:&str)->Option<String>{std::env::var_os(name).or_else(||std::env::vars_os().find(|(k,_)|k.to_string_lossy().eq_ignore_ascii_case(name)).map(|(_,v)|v)).map(|v|v.to_string_lossy().into_owned())}
fn set_env_value(name:&str,value:Option<&str>)->Result<(),u32>{if name.is_empty()||name.contains('='){return Err(87);}if let Some(v)=value{std::env::set_var(name,v);}else{std::env::remove_var(name);}Ok(())}
fn write_a_text(s:&str,buf:*mut u8,size:u32)->u32{let b=crate::dlls::encode_ansi(s);if buf.is_null()||size as usize<=b.len(){b.len().saturating_add(1).min(u32::MAX as usize)as u32}else{unsafe{std::ptr::copy_nonoverlapping(b.as_ptr(),buf,b.len());*buf.add(b.len())=0;}b.len()as u32}}
fn write_w_text(s:&str,buf:*mut u16,size:u32)->u32{unsafe{crate::dlls::put_w(s,buf,size)}}
unsafe fn cstring_from_wide(s:*const u16)->Result<std::ffi::CString,u32>{if s.is_null(){return Err(87);}let n=crate::printf::wide_len(s);let text=String::from_utf16(std::slice::from_raw_parts(s,n)).map_err(|_|1113u32)?;std::ffi::CString::new(text).map_err(|_|87)}
fn module_path(module:usize)->Option<String>{if module==0{crate::loader::main_module().map(|m|crate::paths::to_windows(&m.path))}else{crate::loader::find_module_by_addr(module).map(|m|crate::paths::to_windows(&m.path))}}
pub unsafe extern "win64" fn get_environment_variable_a(name: *const u8, buf: *mut u8, size: u32) -> u32 {
    if name.is_null(){crate::process::set_last_error(87);return 0;}let n=crate::dlls::cstr(name);let Some(v)=env_value(&n)else{crate::process::set_last_error(203);return 0;};
    if v.is_empty(){if !buf.is_null()&&size>0{*buf=0;}return 0;}let ret=write_a_text(&v,buf,size);if ret>size{crate::process::set_last_error(122);ret}else{ret}
}
pub unsafe extern "win64" fn set_environment_variable_a(name:*const u8,value:*const u8)->i32{if name.is_null(){crate::process::set_last_error(87);return 0;}let n=crate::dlls::cstr(name);let v=if value.is_null(){None}else{Some(crate::dlls::cstr(value))};match set_env_value(&n,v.as_deref()){Ok(())=>1,Err(e)=>{crate::process::set_last_error(e);0}}}
pub unsafe extern "win64" fn get_current_directory_a(size:u32,buf:*mut u8)->u32{let Ok(p)=std::env::current_dir()else{crate::process::set_last_error(31);return 0;};let ret=write_a_text(&crate::paths::to_windows(&p),buf,size);if ret>size{crate::process::set_last_error(122);}ret}
pub unsafe extern "win64" fn set_current_directory_a(path:*const u8)->i32{if path.is_null(){crate::process::set_last_error(87);return 0;}match std::env::set_current_dir(crate::paths::to_linux(&crate::dlls::cstr(path))){Ok(())=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
fn temp_path()->String{let p=std::env::var_os("TMPDIR").map(std::path::PathBuf::from).filter(|p|p.is_absolute()).unwrap_or_else(||std::path::PathBuf::from("/tmp"));let mut s=crate::paths::to_windows(&p);if !s.ends_with('\\'){s.push('\\');}s}
pub unsafe extern "win64" fn get_temp_path_a(size:u32,buf:*mut u8)->u32{let ret=write_a_text(&temp_path(),buf,size);if ret>size{crate::process::set_last_error(122);}ret}
pub unsafe extern "win64" fn get_module_file_name_a(module:usize,buf:*mut u8,size:u32)->u32{let Some(s)=module_path(module)else{crate::process::set_last_error(6);return 0;};if buf.is_null()||size==0{crate::process::set_last_error(122);return 0;}let ret=write_a_text(&s,buf,size);if ret>size{crate::process::set_last_error(122);0}else{ret}}
pub unsafe extern "win64" fn get_file_attributes_a(path:*const u8)->u32{if path.is_null(){crate::process::set_last_error(87);return u32::MAX;}let p=crate::paths::to_linux(&crate::dlls::cstr(path));match std::fs::metadata(&p){Ok(m)=>{let mut a=0u32;if m.is_dir(){a|=0x10;}if m.permissions().readonly(){a|=0x1;}if p.file_name().is_some_and(|n|n.to_string_lossy().starts_with('.')){a|=0x2;}if a==0{a=if m.is_file(){0x20}else{0x80};}a},Err(e)=>{crate::process::set_last_error(io_error_code(&e));u32::MAX}}}
pub unsafe extern "win64" fn delete_file_a(path:*const u8)->i32{if path.is_null(){crate::process::set_last_error(87);return 0;}match std::fs::remove_file(crate::paths::to_linux(&crate::dlls::cstr(path))){Ok(())=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
pub unsafe extern "win64" fn create_directory_a(path:*const u8,_sec:usize)->i32{if path.is_null(){crate::process::set_last_error(87);return 0;}match std::fs::create_dir(crate::paths::to_linux(&crate::dlls::cstr(path))){Ok(())=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
pub unsafe extern "win64" fn create_file_w(path:*const u16,access:u32,share:u32,sec:usize,creation:u32,flags:u32,template:usize)->usize{let s=match cstring_from_wide(path){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return usize::MAX;}};create_file_a(s.as_ptr()as *const u8,access,share,sec,creation,flags,template)}
pub unsafe extern "win64" fn get_environment_variable_w(name:*const u16,buf:*mut u16,size:u32)->u32{let n=match cstring_from_wide(name){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};let Some(v)=env_value(&n.to_string_lossy())else{crate::process::set_last_error(203);return 0;};if v.is_empty(){if !buf.is_null()&&size>0{*buf=0;}return 0;}let ret=write_w_text(&v,buf,size);if ret>size{crate::process::set_last_error(122);}ret}
pub unsafe extern "win64" fn set_environment_variable_w(name:*const u16,value:*const u16)->i32{let n=match cstring_from_wide(name){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};let v=if value.is_null(){None}else{match cstring_from_wide(value){Ok(s)=>Some(s),Err(e)=>{crate::process::set_last_error(e);return 0;}}};let value=v.map(|s|s.to_string_lossy().into_owned());match set_env_value(&n.to_string_lossy(),value.as_deref()){Ok(())=>1,Err(e)=>{crate::process::set_last_error(e);0}}}
pub unsafe extern "win64" fn get_current_directory_w(size:u32,buf:*mut u16)->u32{let Ok(p)=std::env::current_dir()else{crate::process::set_last_error(31);return 0;};let ret=write_w_text(&crate::paths::to_windows(&p),buf,size);if ret>size{crate::process::set_last_error(122);}ret}
pub unsafe extern "win64" fn set_current_directory_w(path:*const u16)->i32{let s=match cstring_from_wide(path){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};set_current_directory_a(s.as_ptr()as *const u8)}
pub unsafe extern "win64" fn get_temp_path_w(size:u32,buf:*mut u16)->u32{let ret=write_w_text(&temp_path(),buf,size);if ret>size{crate::process::set_last_error(122);}ret}
pub unsafe extern "win64" fn get_module_file_name_w(module:usize,buf:*mut u16,size:u32)->u32{let Some(s)=module_path(module)else{crate::process::set_last_error(6);return 0;};if buf.is_null()||size==0{crate::process::set_last_error(122);return 0;}let ret=write_w_text(&s,buf,size);if ret>size{crate::process::set_last_error(122);0}else{ret}}
pub unsafe extern "win64" fn get_file_attributes_w(path:*const u16)->u32{let s=match cstring_from_wide(path){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return u32::MAX;}};get_file_attributes_a(s.as_ptr()as *const u8)}
pub unsafe extern "win64" fn delete_file_w(path:*const u16)->i32{let s=match cstring_from_wide(path){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};delete_file_a(s.as_ptr()as *const u8)}
pub unsafe extern "win64" fn create_directory_w(path:*const u16,sec:usize)->i32{let s=match cstring_from_wide(path){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};create_directory_a(s.as_ptr()as *const u8,sec)}
fn normalize_host_path(path:std::path::PathBuf)->std::path::PathBuf{use std::path::Component;let mut out=std::path::PathBuf::new();for c in path.components(){match c{Component::CurDir=>{},Component::ParentDir=>{if !out.pop()&& !out.is_absolute(){out.push("..");}},other=>out.push(other.as_os_str())}}out}
fn full_windows_path(name:&str)->Result<String,u32>{let p=crate::paths::to_linux(name);let p=if p.is_absolute(){p}else{std::env::current_dir().map_err(|_|31u32)?.join(p)};Ok(crate::paths::to_windows(&normalize_host_path(p)))}
pub unsafe extern "win64" fn get_full_path_name_a(name:*const u8,size:u32,buf:*mut u8,file_part:*mut *mut u8)->u32{
    if name.is_null(){crate::process::set_last_error(87);return 0;}let path=match full_windows_path(&crate::dlls::cstr(name)){Ok(p)=>p,Err(e)=>{crate::process::set_last_error(e);return 0;}};let bytes=crate::dlls::encode_ansi(&path);let needed=bytes.len().saturating_add(1);if needed>u32::MAX as usize{crate::process::set_last_error(87);return 0;}if buf.is_null()||(size as usize)<needed{if !file_part.is_null(){*file_part=std::ptr::null_mut();}return needed as u32;}
    std::ptr::copy_nonoverlapping(bytes.as_ptr(),buf,bytes.len());*buf.add(bytes.len())=0;if !file_part.is_null(){let offset=bytes.iter().rposition(|b|*b==b'\\'||*b==b'/').map_or(0,|i|i+1);*file_part=buf.add(offset);}bytes.len()as u32
}
pub unsafe extern "win64" fn get_full_path_name_w(name:*const u16,size:u32,buf:*mut u16,file_part:*mut *mut u16)->u32{
    let wide=match cstring_from_wide(name){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};let path=match full_windows_path(&wide.to_string_lossy()){Ok(p)=>p,Err(e)=>{crate::process::set_last_error(e);return 0;}};let text:Vec<u16>=path.encode_utf16().collect();let needed=text.len().saturating_add(1);if needed>u32::MAX as usize{crate::process::set_last_error(87);return 0;}if buf.is_null()||(size as usize)<needed{if !file_part.is_null(){*file_part=std::ptr::null_mut();}return needed as u32;}
    std::ptr::copy_nonoverlapping(text.as_ptr(),buf,text.len());*buf.add(text.len())=0;if !file_part.is_null(){let offset=text.iter().rposition(|c|*c==(b'\\' as u16)||*c==(b'/' as u16)).map_or(0,|i|i+1);*file_part=buf.add(offset);}text.len()as u32
}
pub unsafe extern "win64" fn copy_file_a(src:*const u8,dst:*const u8,fail_if_exists:i32)->i32{if src.is_null()||dst.is_null(){crate::process::set_last_error(87);return 0;}let source=crate::paths::to_linux(&crate::dlls::cstr(src));let target=crate::paths::to_linux(&crate::dlls::cstr(dst));if fail_if_exists!=0&&target.exists(){crate::process::set_last_error(80);return 0;}match std::fs::copy(source,target){Ok(_)=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
pub unsafe extern "win64" fn copy_file_w(src:*const u16,dst:*const u16,fail_if_exists:i32)->i32{let s=match cstring_from_wide(src){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};let d=match cstring_from_wide(dst){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};let source=crate::paths::to_linux(&s.to_string_lossy());let target=crate::paths::to_linux(&d.to_string_lossy());if fail_if_exists!=0&&target.exists(){crate::process::set_last_error(80);return 0;}match std::fs::copy(source,target){Ok(_)=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
pub unsafe extern "win64" fn move_file_a(src:*const u8,dst:*const u8)->i32{if src.is_null()||dst.is_null(){crate::process::set_last_error(87);return 0;}let s=crate::paths::to_linux(&crate::dlls::cstr(src));let d=crate::paths::to_linux(&crate::dlls::cstr(dst));if d.exists(){crate::process::set_last_error(80);return 0;}match std::fs::rename(s,d){Ok(())=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
pub unsafe extern "win64" fn move_file_w(src:*const u16,dst:*const u16)->i32{let s=match cstring_from_wide(src){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};let d=match cstring_from_wide(dst){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};let s=crate::paths::to_linux(&s.to_string_lossy());let d=crate::paths::to_linux(&d.to_string_lossy());if d.exists(){crate::process::set_last_error(80);return 0;}match std::fs::rename(s,d){Ok(())=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
pub unsafe extern "win64" fn remove_directory_a(path:*const u8)->i32{if path.is_null(){crate::process::set_last_error(87);return 0;}match std::fs::remove_dir(crate::paths::to_linux(&crate::dlls::cstr(path))){Ok(())=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
pub unsafe extern "win64" fn remove_directory_w(path:*const u16)->i32{let s=match cstring_from_wide(path){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};match std::fs::remove_dir(crate::paths::to_linux(&s.to_string_lossy())){Ok(())=>1,Err(e)=>{crate::process::set_last_error(io_error_code(&e));0}}}
pub extern "win64" fn sleep(ms:u32) { std::thread::sleep(std::time::Duration::from_millis(ms as u64)); }
pub extern "win64" fn tls_alloc() -> u32 {
    let mut slots=lock(TLS_SLOTS.get_or_init(||Mutex::new(vec![0;TLS_LIMIT])));
    let Some((i,slot))=slots.iter_mut().enumerate().find(|(_,g)| **g==0) else {crate::process::set_last_error(8);return u32::MAX;};
    let _=crate::process::clear_tls_value_all_threads(i as u32);let generation=TLS_GENERATION.fetch_add(1,Ordering::Relaxed).max(1);*slot=generation;crate::process::set_last_error(0);i as u32
}
pub extern "win64" fn tls_free(i:u32)->i32 {
    let mut slots=lock(TLS_SLOTS.get_or_init(||Mutex::new(vec![0;TLS_LIMIT])));
    let Some(slot)=slots.get_mut(i as usize) else {crate::process::set_last_error(87);return 0;};
    if *slot==0 {crate::process::set_last_error(87);return 0;}let _=crate::process::clear_tls_value_all_threads(i);*slot=0;crate::process::set_last_error(0);1
}
pub unsafe extern "win64" fn tls_set_value(i:u32,v:*mut c_void)->i32 {
    let slots=lock(TLS_SLOTS.get_or_init(||Mutex::new(vec![0;TLS_LIMIT])));if slots.get(i as usize).copied().unwrap_or(0)==0{crate::process::set_last_error(87);return 0;}
    if !crate::process::set_tls_value(i,v as usize){crate::process::set_last_error(87);return 0;}crate::process::set_last_error(0);1
}
pub extern "win64" fn tls_get_value(i:u32)->*mut c_void {
    let slots=lock(TLS_SLOTS.get_or_init(||Mutex::new(vec![0;TLS_LIMIT])));if slots.get(i as usize).copied().unwrap_or(0)==0{crate::process::set_last_error(87);return std::ptr::null_mut();}
    let value=crate::process::tls_value(i).unwrap_or(0);crate::process::set_last_error(0);value as *mut c_void
}
fn cp1252_decode(b:u8)->char{const C:[u16;32]=[0x20ac,0x0081,0x201a,0x0192,0x201e,0x2026,0x2020,0x2021,0x02c6,0x2030,0x0160,0x2039,0x0152,0x008d,0x017d,0x008f,0x0090,0x2018,0x2019,0x201c,0x201d,0x2022,0x2013,0x2014,0x02dc,0x2122,0x0161,0x203a,0x0153,0x009d,0x017e,0x0178];if (0x80..=0x9f).contains(&b){char::from_u32(C[(b-0x80)as usize]as u32).unwrap()}else{char::from_u32(b as u32).unwrap()}}
fn decode_windows_bytes(cp:u32,flags:u32,bytes:&[u8])->Result<String,u32>{if flags&!0x0b!=0{return Err(87);}match cp{65001=>{if flags&8!=0{String::from_utf8(bytes.to_vec()).map_err(|_|1113)}else{Ok(String::from_utf8_lossy(bytes).into_owned())}},0|1252=>Ok(bytes.iter().copied().map(cp1252_decode).collect()),_=>Err(87)}}
fn cp1252_encode(c:char)->Option<u8>{let n=c as u32;if n<=0x7f||(0xa0..=0xff).contains(&n){return Some(n as u8);}const C:[u16;32]=[0x20ac,0x0081,0x201a,0x0192,0x201e,0x2026,0x2020,0x2021,0x02c6,0x2030,0x0160,0x2039,0x0152,0x008d,0x017d,0x008f,0x0090,0x2018,0x2019,0x201c,0x201d,0x2022,0x2013,0x2014,0x02dc,0x2122,0x0161,0x203a,0x0153,0x009d,0x017e,0x0178];C.iter().position(|&x|x as u32==n).map(|i|i as u8+0x80)}
pub unsafe extern "win64" fn multi_byte_to_wide(cp:u32,flags:u32,s:*const u8,n:i32,w:*mut u16,wn:i32)->i32{
    if s.is_null()||n==0||n< -1||wn<0{crate::process::set_last_error(87);return 0;}
    let count=if n== -1{crate::printf::narrow_len(s)+1}else{n as usize};let bytes=std::slice::from_raw_parts(s,count);
    let text=match decode_windows_bytes(cp,flags,bytes){Ok(s)=>s,Err(e)=>{crate::process::set_last_error(e);return 0;}};
    let out:Vec<u16>=text.encode_utf16().collect();if out.len()>i32::MAX as usize{crate::process::set_last_error(87);return 0;}if w.is_null(){return out.len()as i32;}if (wn as usize)<out.len(){crate::process::set_last_error(122);return 0;}if !out.is_empty(){std::ptr::copy_nonoverlapping(out.as_ptr(),w,out.len());}out.len()as i32
}
pub unsafe extern "win64" fn wide_char_to_multi_byte(cp:u32,flags:u32,w:*const u16,n:i32,out:*mut u8,cap:i32,default:*const u8,used:*mut i32)->i32{
    if w.is_null()||n==0||n< -1||cap<0{crate::process::set_last_error(87);return 0;}if flags&!0x480!=0{crate::process::set_last_error(87);return 0;}if cp==65001&&(!default.is_null()||!used.is_null()){crate::process::set_last_error(87);return 0;}
    let count=if n== -1{crate::printf::wide_len(w)+1}else{n as usize};let units=std::slice::from_raw_parts(w,count);
    let text=if flags&0x80!=0{match String::from_utf16(units){Ok(s)=>s,Err(_)=>{crate::process::set_last_error(1113);return 0;}}}else{String::from_utf16_lossy(units)};
    let mut substituted=false;let bytes=match cp{65001=>text.as_bytes().to_vec(),0|1252=>text.chars().map(|c|cp1252_encode(c).unwrap_or_else(||{substituted=true;if default.is_null(){b'?'}else{*default}})).collect(),_=>{crate::process::set_last_error(87);return 0;}};
    if !used.is_null(){*used=i32::from(substituted);}if bytes.len()>i32::MAX as usize{crate::process::set_last_error(87);return 0;}if out.is_null(){return bytes.len()as i32;}if (cap as usize)<bytes.len(){crate::process::set_last_error(122);return 0;}if !bytes.is_empty(){std::ptr::copy_nonoverlapping(bytes.as_ptr(),out,bytes.len());}bytes.len()as i32
}
pub unsafe extern "win64" fn write_console_w(_h:usize,buf:*const u16,n:u32,written:*mut u32,_r:usize)->i32 { if n!=0&&buf.is_null(){crate::process::set_last_error(87);return 0;}let units=if n==0{&[]}else{std::slice::from_raw_parts(buf,n as usize)};let x=String::from_utf16_lossy(units);if std::io::Write::write_all(&mut std::io::stdout(),x.as_bytes()).is_err(){return 0;}if !written.is_null(){*written=n;}1 }
pub extern "win64" fn query_performance_frequency(out:*mut i64)->i32 { unsafe{if out.is_null(){return 0;}*out=1_000_000_000;}1 }
pub extern "win64" fn query_performance_counter(out:*mut i64)->i32 { unsafe{if out.is_null(){return 0;}*out=CLOCK_START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as i64;}1 }
pub fn flush_and_exit(code:i32)->! { let _=std::io::Write::flush(&mut std::io::stdout()); std::process::exit(code) }
pub fn exports() -> crate::dlls::Exports { crate::exports! {
 "GetStdHandle"=>fn get_std_handle, "WriteFile"=>fn write_file, "ReadFile"=>fn read_file, "CreateFileA"=>fn create_file_a, "GetFileSize"=>fn get_file_size, "SetFilePointer"=>fn set_file_pointer, "FlushFileBuffers"=>fn flush_file_buffers, "ExitProcess"=>fn exit_process,
  "CreateEventA"=>fn create_event_a,"SetEvent"=>fn set_event,"ResetEvent"=>fn reset_event,"CreateMutexA"=>fn create_mutex_a,"ReleaseMutex"=>fn release_mutex,"WaitForSingleObject"=>fn wait_for_single_object,"WaitForMultipleObjects"=>fn wait_for_multiple_objects,"GetExitCodeThread"=>fn get_exit_code_thread,"ResumeThread"=>fn resume_thread,
  "InitializeCriticalSection"=>fn initialize_critical_section,"InitializeCriticalSectionAndSpinCount"=>fn initialize_critical_section_and_spin_count,"InitializeCriticalSectionEx"=>fn initialize_critical_section_ex,"EnterCriticalSection"=>fn enter_critical_section,"TryEnterCriticalSection"=>fn try_enter_critical_section,"LeaveCriticalSection"=>fn leave_critical_section,"DeleteCriticalSection"=>fn delete_critical_section,
  "GetCommandLineA"=>fn get_command_line_a,"GetCommandLineW"=>fn get_command_line_w,"GetModuleHandleA"=>fn get_module_handle_a,"GetModuleHandleW"=>fn get_module_handle_w,"GetProcAddress"=>fn get_proc_address,"LoadLibraryA"=>fn load_library_a,"LoadLibraryW"=>fn load_library_w,"FreeLibrary"=>fn free_library,"CreateProcessA"=>fn create_process_a,"CreateProcessW"=>fn create_process_w,"OpenProcess"=>fn open_process,"TerminateProcess"=>fn terminate_process,"GetExitCodeProcess"=>fn get_exit_code_process,"GetCurrentProcess"=>fn get_current_process,"GetCurrentThread"=>fn get_current_thread,"GetCurrentThreadId"=>fn get_current_thread_id,
 "GetLastError"=>fn get_last_error, "SetLastError"=>fn set_last_error, "GetProcessHeap"=>fn get_process_heap,
   "HeapAlloc"=>fn heap_alloc,"HeapReAlloc"=>fn heap_realloc,"HeapFree"=>fn heap_free,"VirtualAlloc"=>fn virtual_alloc,"VirtualFree"=>fn virtual_free,"VirtualProtect"=>fn virtual_protect,"VirtualQuery"=>fn virtual_query,"VirtualQueryEx"=>fn virtual_query_ex,"FlushInstructionCache"=>fn flush_instruction_cache,
  "GetTickCount"=>fn get_tick_count,"GetTickCount64"=>fn get_tick_count64,"GetCurrentProcessId"=>fn get_current_process_id,"CreateThread"=>fn create_thread,"CloseHandle"=>fn close_handle,"GetEnvironmentVariableA"=>fn get_environment_variable_a,"SetEnvironmentVariableA"=>fn set_environment_variable_a,
  "GetSystemInfo"=>fn get_system_info,"GetNativeSystemInfo"=>fn get_native_system_info,"GetSystemTimeAsFileTime"=>fn get_system_time_as_file_time,"GetSystemTimePreciseAsFileTime"=>fn get_system_time_as_file_time,"GetVersion"=>fn get_version,"GetVersionExA"=>fn get_version_ex_a,"GetVersionExW"=>fn get_version_ex_w,
   "GetCurrentDirectoryA"=>fn get_current_directory_a,"SetCurrentDirectoryA"=>fn set_current_directory_a,"GetTempPathA"=>fn get_temp_path_a,"GetModuleFileNameA"=>fn get_module_file_name_a,"GetFileAttributesA"=>fn get_file_attributes_a,"DeleteFileA"=>fn delete_file_a,"CreateDirectoryA"=>fn create_directory_a,"CreateFileW"=>fn create_file_w,"GetFullPathNameA"=>fn get_full_path_name_a,"CopyFileA"=>fn copy_file_a,"MoveFileA"=>fn move_file_a,"RemoveDirectoryA"=>fn remove_directory_a,
   "GetEnvironmentVariableW"=>fn get_environment_variable_w,"SetEnvironmentVariableW"=>fn set_environment_variable_w,"GetCurrentDirectoryW"=>fn get_current_directory_w,"SetCurrentDirectoryW"=>fn set_current_directory_w,
   "GetTempPathW"=>fn get_temp_path_w,"GetModuleFileNameW"=>fn get_module_file_name_w,"GetFileAttributesW"=>fn get_file_attributes_w,"DeleteFileW"=>fn delete_file_w,"CreateDirectoryW"=>fn create_directory_w,"GetFullPathNameW"=>fn get_full_path_name_w,"CopyFileW"=>fn copy_file_w,"MoveFileW"=>fn move_file_w,"RemoveDirectoryW"=>fn remove_directory_w,"Sleep"=>fn sleep,
  "TlsAlloc"=>fn tls_alloc, "TlsFree"=>fn tls_free, "TlsSetValue"=>fn tls_set_value, "TlsGetValue"=>fn tls_get_value,
  "MultiByteToWideChar"=>fn multi_byte_to_wide,"WideCharToMultiByte"=>fn wide_char_to_multi_byte,"WriteConsoleW"=>fn write_console_w,
 "QueryPerformanceFrequency"=>fn query_performance_frequency, "QueryPerformanceCounter"=>fn query_performance_counter
  } }

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn fichiers_win32_font_des_allers_retours_sans_perte() {
        init();
        let path = format!("Z:\\tmp\\winlux-k32-{}-{}.bin", std::process::id(), next_handle());
        let path_c = CString::new(path).unwrap();
        let h = unsafe { create_file_a(path_c.as_ptr() as *const u8, 0xC000_0000, 0, 0, 2, 0, 0) };
        assert_ne!(h, usize::MAX);
        let payload = [0x00, 0xff, b'W', b'X'];
        let mut done = 99;
        assert_eq!(unsafe { write_file(h, payload.as_ptr(), payload.len() as u32, &mut done, 0) }, 1);
        assert_eq!(done, payload.len() as u32);
        let mut high = 0;
        assert_eq!(unsafe { get_file_size(h, &mut high) }, payload.len() as u32);
        assert_eq!(high, 0);
        assert_eq!(unsafe { set_file_pointer(h, 0, std::ptr::null_mut(), 0) }, 0);
        let mut output = [0u8; 4];
        assert_eq!(unsafe { read_file(h, output.as_mut_ptr(), output.len() as u32, &mut done, 0) }, 1);
        assert_eq!(done, payload.len() as u32);
        assert_eq!(output, payload);
        assert_eq!(close_handle(h), 1);
        assert_eq!(unsafe { delete_file_a(path_c.as_ptr() as *const u8) }, 1);
    }

    #[test]
    fn evenements_auto_et_manuel_et_mutex_recursif() {
        init();
        let auto = unsafe { create_event_a(0, 0, 0, std::ptr::null()) };
        assert_eq!(wait_for_single_object(auto, 0), 258);
        assert_eq!(set_event(auto), 1);
        assert_eq!(wait_for_single_object(auto, 0), 0);
        assert_eq!(wait_for_single_object(auto, 0), 258); // auto-reset consomme le signal
        close_handle(auto);

        let manual = unsafe { create_event_a(0, 1, 0, std::ptr::null()) };
        assert_eq!(set_event(manual), 1);
        assert_eq!(wait_for_single_object(manual, 0), 0);
        assert_eq!(wait_for_single_object(manual, 0), 0); // le signal reste actif
        assert_eq!(reset_event(manual), 1);
        assert_eq!(wait_for_single_object(manual, 0), 258);
        close_handle(manual);

        let mutex = unsafe { create_mutex_a(0, 0, std::ptr::null()) };
        assert_eq!(wait_for_single_object(mutex, 0), 0);
        assert_eq!(wait_for_single_object(mutex, 0), 0); // mutex Win32 récursif
        assert_eq!(release_mutex(mutex), 1);
        assert_eq!(release_mutex(mutex), 1);
        assert_eq!(release_mutex(mutex), 0); // un mutex libre n'appartient pas au thread
        close_handle(mutex);
    }

    #[test]
    fn wait_for_multiple_objects_any_retourne_le_premier_handle_signale() {
        init();let a=unsafe{create_event_a(0,0,0,std::ptr::null())};let b=unsafe{create_event_a(0,0,0,std::ptr::null())};
        assert_eq!(set_event(b),1);let handles=[a,b];assert_eq!(unsafe{wait_for_multiple_objects(2,handles.as_ptr(),0,0)},1);
        assert_eq!(unsafe{wait_for_multiple_objects(2,handles.as_ptr(),1,0)},258);
        close_handle(a);close_handle(b);
    }

    #[test]
    fn wait_all_ne_consomme_aucun_evenement_avant_que_tous_soient_signales(){
        init();let a=unsafe{create_event_a(0,0,0,std::ptr::null())};let b=unsafe{create_event_a(0,0,0,std::ptr::null())};let handles=[a,b];
        set_event(b);assert_eq!(unsafe{wait_for_multiple_objects(2,handles.as_ptr(),1,0)},258);assert_eq!(wait_for_single_object(b,0),0);set_event(b);
        set_event(a);assert_eq!(unsafe{wait_for_multiple_objects(2,handles.as_ptr(),1,0)},0);assert_eq!(wait_for_single_object(a,0),258);assert_eq!(wait_for_single_object(b,0),258);
        close_handle(a);close_handle(b);
    }

    #[test]
    fn wait_all_prend_le_mutex_seulement_quand_tout_est_signale(){
        init();let mutex=unsafe{create_mutex_a(0,0,std::ptr::null())};let event=unsafe{create_event_a(0,1,0,std::ptr::null())};let handles=[mutex,event];
        assert_eq!(wait_for_single_object(mutex,0),0);
        assert_eq!(unsafe{wait_for_multiple_objects(2,handles.as_ptr(),1,0)},258);
        let Some(SyncHandle::Mutex(m))=lookup_sync(mutex)else{panic!("mutex absent")};
        assert_eq!(release_mutex(mutex),1);
        assert!(std::thread::spawn(move||{let acquired=wait_mutex(&m,0)==0;if acquired{leave_mutex(&m)}else{false}}).join().unwrap());
        set_event(event);assert_eq!(unsafe{wait_for_multiple_objects(2,handles.as_ptr(),1,0)},0);
        assert_eq!(release_mutex(mutex),1);assert_eq!(close_handle(mutex),1);assert_eq!(close_handle(event),1);
    }

    #[test]
    fn section_critique_est_recursive_et_supprimee_proprement(){
        init();let mut storage=[0usize;5];let cs=storage.as_mut_ptr()as *mut c_void;unsafe{initialize_critical_section(cs);enter_critical_section(cs);enter_critical_section(cs);assert_eq!(try_enter_critical_section(cs),1);leave_critical_section(cs);leave_critical_section(cs);leave_critical_section(cs);delete_critical_section(cs);}assert!(critical_object(cs as usize).is_none());
    }

    #[test]
    fn tls_valide_les_indices_et_invalide_les_valeurs_apres_tls_free() {
        init();
        crate::process::init_thread();
        let i=tls_alloc();assert_ne!(i,u32::MAX);
        assert_eq!(unsafe{tls_set_value(i,0x1234usize as *mut c_void)},1);
        assert_eq!(tls_get_value(i) as usize,0x1234);
        assert_eq!(tls_free(i),1);
        assert!(tls_get_value(i).is_null());
        assert_eq!(unsafe{tls_set_value(i,std::ptr::null_mut())},0);
        let reused=tls_alloc();assert_eq!(reused,i);
        assert!(tls_get_value(reused).is_null());
        tls_free(reused);
    }

    #[test]
    fn conversions_unicode_utf8_et_ansi_cp1252_sont_symetriques() {
        let input=[b'A',0x80];let mut wide=[0u16;4];
        assert_eq!(unsafe{multi_byte_to_wide(1252,0,input.as_ptr(),2,wide.as_mut_ptr(),4)},2);
        assert_eq!(&wide[..2],&[b'A' as u16,0x20ac]);
        let mut narrow=[0u8;4];let mut used=0;
        assert_eq!(unsafe{wide_char_to_multi_byte(1252,0,wide.as_ptr(),2,narrow.as_mut_ptr(),4,std::ptr::null(),&mut used)},2);
        assert_eq!(&narrow[..2],&input);assert_eq!(used,0);
        assert_eq!(unsafe{multi_byte_to_wide(65001,8,[0xffu8].as_ptr(),1,wide.as_mut_ptr(),4)},0);
    }

    #[test]
    fn api_environnement_wide_sert_a_lire_et_ecrire_les_variables() {
        let key=format!("WINLUX_TEST_WIDE_{}",std::process::id());
        let value="Ete été";
        let k:Vec<u16>=key.encode_utf16().chain([0]).collect();let v:Vec<u16>=value.encode_utf16().chain([0]).collect();
        unsafe {
            assert_eq!(set_environment_variable_w(k.as_ptr(),v.as_ptr()),1);
            let mut out=[0u16;32];
            assert_eq!(get_environment_variable_w(k.as_ptr(),out.as_mut_ptr(),out.len() as u32),value.encode_utf16().count() as u32);
            assert_eq!(String::from_utf16(&out[..value.encode_utf16().count()]).unwrap(),value);
            assert_eq!(set_environment_variable_w(k.as_ptr(),std::ptr::null()),1);
            assert_eq!(get_environment_variable_w(k.as_ptr(),out.as_mut_ptr(),out.len() as u32),0);
        }
    }

    extern "win64" fn thread_returns_42(_param: usize) -> u32 { 42 }

    #[test]
    fn create_thread_retourne_un_handle_attendable_et_un_code_de_sortie() {
        init();
        let mut tid = 0;
        let h = unsafe { create_thread(0, 0, thread_returns_42 as *const () as usize, 0, 0, &mut tid) };
        assert_ne!(h, 0);
        assert_ne!(tid, 0);
        assert_eq!(wait_for_single_object(h, 5_000), 0);
        let mut code = 259;
        assert_eq!(unsafe { get_exit_code_thread(h, &mut code) }, 1);
        assert_eq!(code, 42);
        assert_eq!(close_handle(h), 1);
    }

    #[test]
    fn create_thread_suspended_attend_resume_thread() {
        init();
        let h=unsafe{create_thread(0,0,thread_returns_42 as *const () as usize,0,4,std::ptr::null_mut())};
        assert_ne!(h,0);
        assert_eq!(wait_for_single_object(h,0),258);
        assert_eq!(resume_thread(h),1);
        assert_eq!(wait_for_single_object(h,5_000),0);
        close_handle(h);
    }

    #[test]
    fn create_process_execute_un_processus_local_et_retourne_son_code(){
        let app=std::ffi::CString::new("/bin/sh").unwrap();let line=std::ffi::CString::new("sh -c \"exit 42\"").unwrap();
        let startup=StartupInfoA{cb:std::mem::size_of::<StartupInfoA>()as u32,reserved:std::ptr::null_mut(),desktop:std::ptr::null_mut(),title:std::ptr::null_mut(),x:0,y:0,x_size:0,y_size:0,x_chars:0,y_chars:0,fill_attribute:0,flags:0,show_window:0,reserved2_size:0,reserved2:std::ptr::null_mut(),std_input:0,std_output:0,std_error:0};
        let mut info=ProcessInformation{process:0,thread:0,process_id:0,thread_id:0};
        assert_eq!(unsafe{create_process_a(app.as_ptr()as *const u8,line.as_ptr()as *const u8,0,0,0,0,std::ptr::null(),std::ptr::null(),&startup,&mut info)},1);
        assert_eq!(wait_for_single_object(info.process,5000),0);let mut code=0;
        assert_eq!(unsafe{get_exit_code_process(info.process,&mut code)},1);assert_eq!(code,42);
        assert_eq!(unsafe{get_exit_code_thread(info.thread,&mut code)},1);assert_eq!(code,42);
        close_handle(info.thread);close_handle(info.process);
    }
}
