//! Décodage WAVE PCM/IEEE-float et pont de sortie vers ALSA (Linux), sans
//! DLL Windows ni framework de compatibilité tiers.

use std::ffi::{c_char,c_int,c_void,CString};
use std::io::Read;
use std::sync::{Mutex,OnceLock};
use std::sync::atomic::{AtomicBool,Ordering};

#[link(name="dl")]
unsafe extern "C" { fn dlopen(name:*const c_char,flags:c_int)->*mut c_void;fn dlsym(handle:*mut c_void,name:*const c_char)->*mut c_void;fn dlclose(handle:*mut c_void)->c_int; }

type PcmOpen=unsafe extern "C" fn(*mut *mut c_void,*const c_char,c_int,c_int)->c_int;
type PcmSetParams=unsafe extern "C" fn(*mut c_void,c_int,c_int,u32,u32,c_int,u32)->c_int;
type PcmWrite=unsafe extern "C" fn(*mut c_void,*const c_void,usize)->isize;
type PcmRecover=unsafe extern "C" fn(*mut c_void,c_int,c_int)->c_int;
type PcmDrain=unsafe extern "C" fn(*mut c_void)->c_int;
type PcmDrop=unsafe extern "C" fn(*mut c_void)->c_int;
type PcmClose=unsafe extern "C" fn(*mut c_void)->c_int;

struct Alsa { lib:*mut c_void, open:PcmOpen, set_params:PcmSetParams, writei:PcmWrite, recover:PcmRecover, drain:PcmDrain, drop_pcm:PcmDrop, close:PcmClose }
unsafe impl Send for Alsa {}
unsafe impl Sync for Alsa {}
impl Drop for Alsa { fn drop(&mut self){unsafe{dlclose(self.lib);}} }
fn alsa()->Result<Alsa,()> {unsafe{
    let lib=dlopen(b"libasound.so.2\0".as_ptr() as *const c_char,2);
    if lib.is_null(){return Err(());}
    macro_rules! sym {($n:literal,$t:ty)=>{{let p=dlsym(lib,concat!($n,"\0").as_ptr()as *const c_char);if p.is_null(){dlclose(lib);return Err(());}std::mem::transmute::<*mut c_void,$t>(p)}}}
    Ok(Alsa{lib,open:sym!("snd_pcm_open",PcmOpen),set_params:sym!("snd_pcm_set_params",PcmSetParams),writei:sym!("snd_pcm_writei",PcmWrite),recover:sym!("snd_pcm_recover",PcmRecover),drain:sym!("snd_pcm_drain",PcmDrain),drop_pcm:sym!("snd_pcm_drop",PcmDrop),close:sym!("snd_pcm_close",PcmClose)})
}}

struct Wave { channels:u16, rate:u32, samples:Vec<i16> }
fn le16(b:&[u8],o:usize)->Option<u16>{Some(u16::from_le_bytes(b.get(o..o+2)?.try_into().ok()?))}
fn le32(b:&[u8],o:usize)->Option<u32>{Some(u32::from_le_bytes(b.get(o..o+4)?.try_into().ok()?))}
fn decode_wave(b:&[u8])->Option<Wave>{
    if b.len()<12||&b[..4]!=b"RIFF"||&b[8..12]!=b"WAVE"{return None;}let riff_end=8usize.checked_add(le32(b,4)? as usize)?;if riff_end>b.len()||riff_end<12{return None;}
    let mut off=12usize;let(mut fmt,mut data)=(None,None);
    while off.checked_add(8)?<=riff_end{let id=&b[off..off+4];let len=le32(b,off+4)? as usize;let start=off+8;let end=start.checked_add(len)?;if end>riff_end{return None;}
        if id==b"fmt "{if len<16{return None;}fmt=Some((le16(b,start)?,le16(b,start+2)?,le32(b,start+4)?,le16(b,start+12)?,le16(b,start+14)?));}
        if id==b"data"{data=Some(&b[start..end]);}
        off=end.checked_add(len&1)?;
    }
    let (format,channels,rate,block,bits)=fmt?;let src=data?;
    if !(channels==1||channels==2)||rate==0||!(format==1||format==3)||!(bits==8||bits==16||bits==24||bits==32){return None;}
    if format==3&&bits!=32{return None;}let bytes_per=(bits/8)as usize;if block as usize!=(channels as usize)*bytes_per||src.len()%(block as usize)!=0{return None;}
    let mut samples=Vec::with_capacity(src.len()/bytes_per);
    for s in src.chunks_exact(bytes_per){let v=match(format,bits){(1,8)=>(s[0]as i32-128)<<8,(1,16)=>i16::from_le_bytes([s[0],s[1]])as i32,(1,24)=>{let mut n=(s[0]as i32)|((s[1]as i32)<<8)|((s[2]as i32)<<16);if n&0x800000!=0{n|=!0xffffff;}n>>8},(1,32)=>i32::from_le_bytes(s.try_into().ok()?)>>16, (3,32)=>{let f=f32::from_bits(u32::from_le_bytes(s.try_into().ok()?));if !f.is_finite(){0}else{(f.clamp(-1.0,1.0)*32767.0)as i32}},_=>return None};samples.push(v.clamp(i16::MIN as i32,i16::MAX as i32)as i16);}
    Some(Wave{channels,rate,samples})
}

fn play_pcm(wave:Wave)->Result<(),u32>{
    let a=alsa().map_err(|_|50u32)?;let default=CString::new("default").unwrap();let mut pcm=std::ptr::null_mut();
    let rc=unsafe{(a.open)(&mut pcm,default.as_ptr(),0,0)};if rc<0{return Err(31);}
    // ALSA: S16_LE=2, RW_INTERLEAVED=3, playback=0.
    let rc=unsafe{(a.set_params)(pcm,2,3,wave.channels as u32,wave.rate,1,500_000)};if rc<0{unsafe{(a.close)(pcm);}return Err(50);}
    let frame_samples=wave.channels as usize;let mut offset=0usize;
    while offset<wave.samples.len(){let frames=((wave.samples.len()-offset)/frame_samples).min(4096);let rc=unsafe{(a.writei)(pcm,wave.samples[offset..].as_ptr()as *const c_void,frames)};
        if rc>0{offset+=rc as usize*frame_samples;continue;}
        if rc==-11{continue;}if unsafe{(a.recover)(pcm,rc as c_int,1)}<0{unsafe{(a.drop_pcm)(pcm);(a.close)(pcm);}return Err(31);}
    }
    let rc=unsafe{(a.drain)(pcm)};unsafe{(a.close)(pcm);}if rc<0{Err(31)}else{Ok(())}
}
fn probe_audio(wave:&Wave)->Result<(),u32>{let a=alsa().map_err(|_|50u32)?;let default=CString::new("default").unwrap();let mut pcm=std::ptr::null_mut();if unsafe{(a.open)(&mut pcm,default.as_ptr(),0,0)}<0{return Err(31);}let rc=unsafe{(a.set_params)(pcm,2,3,wave.channels as u32,wave.rate,1,500_000)};unsafe{(a.close)(pcm);}if rc<0{Err(50)}else{Ok(())}}

const SND_ASYNC:u32=0x0001;const SND_LOOP:u32=0x0008;const SND_MEMORY:u32=0x0004;const SND_PURGE:u32=0x0040;const SND_NOSTOP:u32=0x0010;
struct Job{cancel:std::sync::Arc<AtomicBool>,thread:std::thread::JoinHandle<()>}
static JOBS:OnceLock<Mutex<Vec<Job>>>=OnceLock::new();
fn jobs()->&'static Mutex<Vec<Job>>{JOBS.get_or_init(||Mutex::new(Vec::new()))}
fn lock<T>(m:&Mutex<T>)->std::sync::MutexGuard<'_,T>{m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)}
fn play_wave_cancellable(wave:Wave,cancel:std::sync::Arc<AtomicBool>)->Result<(),u32>{
    let a=alsa().map_err(|_|50u32)?;let default=CString::new("default").unwrap();let mut pcm=std::ptr::null_mut();if unsafe{(a.open)(&mut pcm,default.as_ptr(),0,0)}<0{return Err(31);}
    if unsafe{(a.set_params)(pcm,2,3,wave.channels as u32,wave.rate,1,500_000)}<0{unsafe{(a.close)(pcm);}return Err(50);}
    let frame_samples=wave.channels as usize;let mut offset=0usize;
    while offset<wave.samples.len()&&!cancel.load(Ordering::Relaxed){let frames=((wave.samples.len()-offset)/frame_samples).min(4096);let rc=unsafe{(a.writei)(pcm,wave.samples[offset..].as_ptr()as *const c_void,frames)};if rc>0{offset+=rc as usize*frame_samples;}else if rc==-11{std::thread::sleep(std::time::Duration::from_millis(2));}else if unsafe{(a.recover)(pcm,rc as c_int,1)}<0{unsafe{(a.drop_pcm)(pcm);(a.close)(pcm);}return Err(31);}}
    if cancel.load(Ordering::Relaxed){unsafe{(a.drop_pcm)(pcm);}}else{unsafe{(a.drain)(pcm);}}unsafe{(a.close)(pcm);}Ok(())
}

pub unsafe extern "win64" fn play_sound_a(name:*const u8,_module:usize,flags:u32)->i32{
    if flags&SND_PURGE!=0{for j in lock(jobs()).iter(){j.cancel.store(true,Ordering::Relaxed);}return 1;}
    if name.is_null()||flags&(SND_MEMORY|SND_LOOP)!=0||flags&!(SND_ASYNC|SND_LOOP|SND_MEMORY|SND_PURGE|SND_NOSTOP|0x0002|0x0002_0000)!=0{crate::process::set_last_error(50);return 0;}
    play_sound_path(&crate::dlls::cstr(name),flags)
}
fn decode_audio(raw: &[u8]) -> Option<Wave> {
    if let Some(w) = decode_wave(raw) { return Some(w); }
    if let Some(w) = decode_mp3(raw) { return Some(w); }
    if let Some(w) = decode_aac(raw) { return Some(w); }
    None
}

// ── Décodage MP3 via libmpg123 ────────────────────────────────────

type Mpg123Init = unsafe extern "C" fn() -> c_int;
type Mpg123New = unsafe extern "C" fn(*const c_char, *mut c_int) -> *mut c_void;
type Mpg123Delete = unsafe extern "C" fn(*mut c_void);
type Mpg123OpenFeed = unsafe extern "C" fn(*mut c_void) -> c_int;
type Mpg123Feed = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> c_int;
type Mpg123Read = unsafe extern "C" fn(*mut c_void, *mut u8, usize, *mut usize) -> c_int;
type Mpg123GetFormat = unsafe extern "C" fn(*mut c_void, *mut i64, *mut c_int, *mut c_int) -> c_int;
type Mpg123FormatNone = unsafe extern "C" fn(*mut c_void) -> c_int;
type Mpg123Format = unsafe extern "C" fn(*mut c_void, i64, c_int, c_int) -> c_int;
type Mpg123Exit = unsafe extern "C" fn();

struct Mpg123 {
    lib: *mut c_void,
    new: Mpg123New,
    delete: Mpg123Delete,
    open_feed: Mpg123OpenFeed,
    feed: Mpg123Feed,
    read: Mpg123Read,
    getformat: Mpg123GetFormat,
    format_none: Mpg123FormatNone,
    format: Mpg123Format,
    exit: Mpg123Exit,
}

unsafe impl Send for Mpg123 {}
impl Drop for Mpg123 {
    fn drop(&mut self) { unsafe { (self.exit)(); dlclose(self.lib); } }
}

fn load_mpg123() -> Option<Mpg123> {
    unsafe {
        let lib = dlopen(b"libmpg123.so.0\0".as_ptr() as *const c_char, 2);
        if lib.is_null() { return None; }
        macro_rules! sym {($n:literal,$t:ty)=>{{let p=dlsym(lib,concat!($n,"\0").as_ptr()as *const c_char);if p.is_null(){dlclose(lib);return None;}std::mem::transmute::<*mut c_void,$t>(p)}}}
        let init: Mpg123Init = sym!("mpg123_init", Mpg123Init);
        if init() != 0 { dlclose(lib); return None; }
        Some(Mpg123 {
            lib,
            new: sym!("mpg123_new", Mpg123New),
            delete: sym!("mpg123_delete", Mpg123Delete),
            open_feed: sym!("mpg123_open_feed", Mpg123OpenFeed),
            feed: sym!("mpg123_feed", Mpg123Feed),
            read: sym!("mpg123_read", Mpg123Read),
            getformat: sym!("mpg123_getformat", Mpg123GetFormat),
            format_none: sym!("mpg123_format_none", Mpg123FormatNone),
            format: sym!("mpg123_format", Mpg123Format),
            exit: sym!("mpg123_exit", Mpg123Exit),
        })
    }
}

fn decode_mp3(raw: &[u8]) -> Option<Wave> {
    if raw.len() < 4 { return None; }
    let is_mp3 = (raw[0] == 0xFF && (raw[1] & 0xE0) == 0xE0)
        || (raw.len() >= 3 && &raw[..3] == b"ID3");
    if !is_mp3 { return None; }
    let api = load_mpg123()?;
    unsafe {
        let mut err = 0i32;
        let mh = (api.new)(std::ptr::null(), &mut err);
        if mh.is_null() { return None; }
        let _guard = scopeguard(mh, |h| (api.delete)(h));
        (api.format_none)(mh);
        // MPG123_ENC_SIGNED_16 = 0x00D0
        (api.format)(mh, 44100, 3, 0x00D0); // 3 = mono+stereo
        (api.format)(mh, 48000, 3, 0x00D0);
        (api.format)(mh, 22050, 3, 0x00D0);
        if (api.open_feed)(mh) != 0 { return None; }
        // MPG123_NEW_FORMAT = -11, MPG123_NEED_MORE = -10, MPG123_OK = 0, MPG123_DONE = -12
        let rc = (api.feed)(mh, raw.as_ptr(), raw.len());
        if rc != 0 { return None; }
        let mut rate: i64 = 0;
        let mut channels: c_int = 0;
        let mut encoding: c_int = 0;
        let mut buf = vec![0u8; 16384];
        let mut pcm = Vec::new();
        loop {
            let mut done = 0usize;
            let rc = (api.read)(mh, buf.as_mut_ptr(), buf.len(), &mut done);
            if rc == -11 { // MPG123_NEW_FORMAT
                (api.getformat)(mh, &mut rate, &mut channels, &mut encoding);
                continue;
            }
            if done > 0 {
                for chunk in buf[..done].chunks_exact(2) {
                    pcm.push(i16::from_le_bytes([chunk[0], chunk[1]]));
                }
            }
            if rc == -12 || rc == -10 { break; } // DONE or NEED_MORE
            if rc != 0 && rc != -11 { break; }
        }
        if rate == 0 { (api.getformat)(mh, &mut rate, &mut channels, &mut encoding); }
        if pcm.is_empty() || rate == 0 { return None; }
        Some(Wave { channels: (channels as u16).max(1), rate: rate as u32, samples: pcm })
    }
}

struct ScopeGuard<T, F: FnOnce(T)> { val: Option<T>, f: Option<F> }
impl<T, F: FnOnce(T)> Drop for ScopeGuard<T, F> {
    fn drop(&mut self) { if let (Some(v), Some(f)) = (self.val.take(), self.f.take()) { f(v); } }
}
fn scopeguard<T, F: FnOnce(T)>(val: T, f: F) -> ScopeGuard<T, F> {
    ScopeGuard { val: Some(val), f: Some(f) }
}

// ── Décodage AAC via libfaad ──────────────────────────────────────

type FaadOpen = unsafe extern "C" fn() -> *mut c_void;
type FaadInit = unsafe extern "C" fn(*mut c_void, *const u8, u64, *mut u64, *mut u8) -> i64;
type FaadDecode = unsafe extern "C" fn(*mut c_void, *mut FaadFrameInfo, *const u8, u64) -> *mut c_void;
type FaadClose = unsafe extern "C" fn(*mut c_void);
type FaadSetConf = unsafe extern "C" fn(*mut c_void, *const FaadConf);

#[repr(C)]
struct FaadConf {
    def_object_type: u8,
    def_samplerate: u32,
    output_format: u8,
    downmatrix: u8,
    use_old_adts_syntax: u8,
}

#[repr(C)]
#[derive(Default)]
struct FaadFrameInfo {
    bytesconsumed: u64,
    samples: u64,
    channels: u8,
    error: u8,
    samplerate: u64,
    sbr: u8,
    object_type: u8,
    header_type: u8,
    num_front_channels: u8,
    num_side_channels: u8,
    num_back_channels: u8,
    num_lfe_channels: u8,
    _pad: [u8; 1],
    ps: u8,
}

fn decode_aac(raw: &[u8]) -> Option<Wave> {
    if raw.len() < 7 { return None; }
    let is_adts = raw[0] == 0xFF && (raw[1] & 0xF0) == 0xF0;
    if !is_adts { return None; }
    unsafe {
        let lib = dlopen(b"libfaad.so.2\0".as_ptr() as *const c_char, 2);
        if lib.is_null() { return None; }
        let _lib_guard = scopeguard(lib, |l| { dlclose(l); });
        macro_rules! sym {($n:literal,$t:ty)=>{{let p=dlsym(lib,concat!($n,"\0").as_ptr()as *const c_char);if p.is_null(){return None;}std::mem::transmute::<*mut c_void,$t>(p)}}}
        let open: FaadOpen = sym!("NeAACDecOpen", FaadOpen);
        let init: FaadInit = sym!("NeAACDecInit", FaadInit);
        let decode: FaadDecode = sym!("NeAACDecDecode", FaadDecode);
        let close: FaadClose = sym!("NeAACDecClose", FaadClose);
        let set_conf: FaadSetConf = sym!("NeAACDecSetConfiguration", FaadSetConf);
        let decoder = open();
        if decoder.is_null() { return None; }
        let _dec_guard = scopeguard(decoder, |d| close(d));
        // output_format 1 = FAAD_FMT_16BIT
        let conf = FaadConf { def_object_type: 2, def_samplerate: 44100, output_format: 1, downmatrix: 0, use_old_adts_syntax: 0 };
        set_conf(decoder, &conf);
        let mut samplerate: u64 = 0;
        let mut channels: u8 = 0;
        let consumed = init(decoder, raw.as_ptr(), raw.len() as u64, &mut samplerate, &mut channels);
        if consumed < 0 { return None; }
        let mut offset = consumed as usize;
        let mut pcm = Vec::new();
        while offset < raw.len() {
            let mut info = FaadFrameInfo::default();
            let out = decode(decoder, &mut info, raw[offset..].as_ptr(), (raw.len() - offset) as u64);
            if info.error != 0 || info.bytesconsumed == 0 { break; }
            offset += info.bytesconsumed as usize;
            if !out.is_null() && info.samples > 0 {
                let samples_slice = std::slice::from_raw_parts(out as *const i16, info.samples as usize);
                pcm.extend_from_slice(samples_slice);
                if samplerate == 0 { samplerate = info.samplerate; }
                if channels == 0 { channels = info.channels; }
            }
        }
        if pcm.is_empty() || samplerate == 0 { return None; }
        Some(Wave { channels: (channels as u16).max(1), rate: samplerate as u32, samples: pcm })
    }
}

fn play_sound_path(name:&str,flags:u32)->i32{
    {let mut list=lock(jobs());list.retain(|j|!j.thread.is_finished());if flags&SND_NOSTOP!=0&&!list.is_empty(){return 0;}for j in list.iter(){j.cancel.store(true,Ordering::Relaxed);}}
    let filename=crate::paths::to_linux(name);let f=match std::fs::File::open(filename){Ok(f)=>f,Err(e)=>{crate::process::set_last_error(if e.kind()==std::io::ErrorKind::NotFound{2}else{5});return 0;}};
    let mut raw=Vec::new();if f.take(256*1024*1024).read_to_end(&mut raw).is_err(){crate::process::set_last_error(31);return 0;}let Some(wave)=decode_audio(&raw)else{crate::process::set_last_error(50);return 0;};
    if flags&SND_ASYNC!=0{if let Err(e)=probe_audio(&wave){crate::process::set_last_error(e);return 0;}let cancel=std::sync::Arc::new(AtomicBool::new(false));let worker_cancel=cancel.clone();let Ok(thread)=std::thread::Builder::new().name("winlux-wave-playback".into()).spawn(move||{let _=play_wave_cancellable(wave,worker_cancel);})else{crate::process::set_last_error(8);return 0;};lock(jobs()).push(Job{cancel,thread});1}
    else{match play_pcm(wave){Ok(())=>1,Err(e)=>{crate::process::set_last_error(e);0}}}
}
pub unsafe extern "win64" fn play_sound_w(name:*const u16,_module:usize,flags:u32)->i32{if name.is_null()||flags&SND_PURGE!=0{return play_sound_a(std::ptr::null(),0,flags);}let n=crate::printf::wide_len(name);let s=match String::from_utf16(std::slice::from_raw_parts(name,n)){Ok(s)=>s,Err(_)=>{crate::process::set_last_error(1113);return 0;}};if flags&(SND_MEMORY|SND_LOOP)!=0||flags&!(SND_ASYNC|SND_LOOP|SND_MEMORY|SND_PURGE|SND_NOSTOP|0x0002|0x0002_0000)!=0{crate::process::set_last_error(50);return 0;}play_sound_path(&s,flags)}
pub unsafe extern "win64" fn mci_send_string_a(_cmd:*const u8,_ret:*mut u8,_len:u32,_callback:usize)->u32{crate::process::set_last_error(50);277}
pub extern "win64" fn time_get_time()->u32{crate::dlls::kernel32::get_tick_count()}
pub fn exports()->crate::dlls::Exports{crate::exports!{"PlaySoundA"=>fn play_sound_a,"PlaySoundW"=>fn play_sound_w,"mciSendStringA"=>fn mci_send_string_a,"timeGetTime"=>fn time_get_time}}

#[cfg(test)]
mod tests{
    use super::decode_wave;
    #[test]
    fn decode_wave_pcm_mono_16bits_and_rejette_un_fichier_tronque(){
        let mut w=Vec::from(&b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xac\0\0\x88\x58\x01\0\x02\0\x10\0data\x04\0\0\0\x00\x80\xff\x7f"[..]);
        let decoded=decode_wave(&w).unwrap();assert_eq!(decoded.channels,1);assert_eq!(decoded.rate,44100);assert_eq!(decoded.samples,[i16::MIN,i16::MAX]);
        w.pop();assert!(decode_wave(&w).is_none());
    }
}
