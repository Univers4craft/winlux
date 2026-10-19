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
fn play_sound_path(name:&str,flags:u32)->i32{
    {let mut list=lock(jobs());list.retain(|j|!j.thread.is_finished());if flags&SND_NOSTOP!=0&&!list.is_empty(){return 0;}for j in list.iter(){j.cancel.store(true,Ordering::Relaxed);}}
    let filename=crate::paths::to_linux(name);let f=match std::fs::File::open(filename){Ok(f)=>f,Err(e)=>{crate::process::set_last_error(if e.kind()==std::io::ErrorKind::NotFound{2}else{5});return 0;}};
    let mut raw=Vec::new();if f.take(256*1024*1024).read_to_end(&mut raw).is_err(){crate::process::set_last_error(31);return 0;}let Some(wave)=decode_wave(&raw)else{crate::process::set_last_error(50);return 0;};
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
