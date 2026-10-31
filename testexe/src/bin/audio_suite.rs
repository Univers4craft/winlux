//! Les API audio Windows depuis un vrai .exe : waveOut, DirectSound, WASAPI
//! (MMDeviceEnumerator par CoCreateInstance) et XAudio2 2.9 / 2.7, avec des
//! rappels écrits dans le programme et appelés par le thread audio de Winlux.
//! À lancer avec `WINLUX_AUDIO=null` en CI (sortie nulle cadencée).
#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr::{null, null_mut};
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use winlux_testexe::*;

#[link(name = "winmm", kind = "raw-dylib")]
extern "system" {
    fn waveOutOpen(
        h: *mut usize,
        dev: u32,
        fmt: *const u8,
        cb: usize,
        inst: usize,
        flags: u32,
    ) -> u32;
    fn waveOutPrepareHeader(h: usize, hdr: *mut Hdr, size: u32) -> u32;
    fn waveOutUnprepareHeader(h: usize, hdr: *mut Hdr, size: u32) -> u32;
    fn waveOutWrite(h: usize, hdr: *mut Hdr, size: u32) -> u32;
    fn waveOutGetPosition(h: usize, mmt: *mut u32, size: u32) -> u32;
    fn waveOutClose(h: usize) -> u32;
    fn waveOutGetNumDevs() -> u32;
}
#[link(name = "dsound", kind = "raw-dylib")]
extern "system" {
    fn DirectSoundCreate8(dev: *const Guid, out: *mut *mut c_void, outer: *mut c_void) -> i32;
}
#[link(name = "xaudio2_9", kind = "raw-dylib")]
extern "system" {
    fn XAudio2Create(out: *mut *mut c_void, flags: u32, processor: u32) -> i32;
}
#[link(name = "ole32", kind = "raw-dylib")]
extern "system" {
    fn CoCreateInstance(
        clsid: *const Guid,
        outer: *mut c_void,
        ctx: u32,
        iid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32;
}

/// WAVEHDR x64.
#[repr(C)]
struct Hdr {
    data: *const u8,
    len: u32,
    recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: usize,
    reserved: usize,
}

#[repr(C, align(8))]
struct Bytes<const N: usize>([u8; N]);

const fn g(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    Guid {
        data1: d1,
        data2: d2,
        data3: d3,
        data4: d4,
    }
}

/// Méthode `i` de la table virtuelle d'un objet COM.
unsafe fn slot(obj: *mut c_void, i: usize) -> usize {
    *(*(obj as *const *const usize)).add(i)
}
macro_rules! vcall {
    ($obj:expr, $i:expr, fn($($t:ty),* $(,)?) -> $r:ty, $($a:expr),*) => {{
        let f: unsafe extern "system" fn(*mut c_void $(, $t)*) -> $r = core::mem::transmute(slot($obj, $i));
        f($obj $(, $a)*)
    }};
}

/// WAVEFORMATEX PCM 16 bits.
fn pcm16(channels: u16, rate: u32) -> Bytes<20> {
    let mut b = [0u8; 20];
    let block = channels * 2;
    b[0..2].copy_from_slice(&1u16.to_le_bytes());
    b[2..4].copy_from_slice(&channels.to_le_bytes());
    b[4..8].copy_from_slice(&rate.to_le_bytes());
    b[8..12].copy_from_slice(&(rate * block as u32).to_le_bytes());
    b[12..14].copy_from_slice(&block.to_le_bytes());
    b[14..16].copy_from_slice(&16u16.to_le_bytes());
    Bytes(b)
}

static mut PCM: [i16; 4410] = [0; 4410];

fn fill_pcm() {
    unsafe {
        let pcm = &mut *core::ptr::addr_of_mut!(PCM);
        for (i, s) in pcm.iter_mut().enumerate() {
            *s = if (i / 25) % 2 == 0 { 4000 } else { -4000 };
        }
    }
}
fn pcm_ptr() -> *const u8 {
    core::ptr::addr_of!(PCM) as *const u8
}

// ---------------------------------------------------------------------------

static WOM_DONE: AtomicU32 = AtomicU32::new(0);
static WOM_OPEN: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn wave_cb(_h: usize, msg: u32, inst: usize, _p1: usize, _p2: usize) {
    if inst != 0x1234 {
        return;
    }
    match msg {
        0x3bb => {
            WOM_OPEN.fetch_add(1, Ordering::SeqCst);
        }
        0x3bd => {
            WOM_DONE.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    }
}

fn wait_until(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..400 {
        if f() {
            return true;
        }
        unsafe { Sleep(10) };
    }
    false
}

unsafe fn test_wave_out() {
    out(b"waveOut\n");
    check!(waveOutGetNumDevs() == 1, "un périphérique de sortie");
    let fmt = pcm16(1, 22050);
    let mut h = 0usize;
    check!(
        waveOutOpen(
            &mut h,
            u32::MAX,
            fmt.0.as_ptr(),
            wave_cb as *const () as usize,
            0x1234,
            0x3_0000
        ) == 0,
        "waveOutOpen avec CALLBACK_FUNCTION"
    );
    check!(WOM_OPEN.load(Ordering::SeqCst) == 1, "WOM_OPEN reçu");
    let mut a = Hdr {
        data: pcm_ptr(),
        len: 4410,
        recorded: 0,
        user: 0,
        flags: 0,
        loops: 0,
        next: 0,
        reserved: 0,
    };
    let mut b = Hdr {
        data: pcm_ptr().add(4410),
        len: 4410,
        recorded: 0,
        user: 0,
        flags: 0,
        loops: 0,
        next: 0,
        reserved: 0,
    };
    let size = core::mem::size_of::<Hdr>() as u32;
    check!(
        waveOutPrepareHeader(h, &mut a, size) == 0 && waveOutPrepareHeader(h, &mut b, size) == 0,
        "préparation"
    );
    check!(
        waveOutWrite(h, &mut a, size) == 0 && waveOutWrite(h, &mut b, size) == 0,
        "deux tampons en file"
    );
    check!(
        wait_until(|| WOM_DONE.load(Ordering::SeqCst) == 2),
        "deux WOM_DONE depuis le thread audio"
    );
    check!(
        core::ptr::read_volatile(&b.flags) & 1 != 0,
        "WHDR_DONE posé"
    );
    let mut mmt = [4u32, 0, 0];
    check!(
        waveOutGetPosition(h, mmt.as_mut_ptr(), 12) == 0 && mmt[1] == 8820,
        "position = 8820 octets"
    );
    waveOutUnprepareHeader(h, &mut a, size);
    waveOutUnprepareHeader(h, &mut b, size);
    check!(waveOutClose(h) == 0, "waveOutClose");
}

// ---------------------------------------------------------------------------

unsafe fn test_direct_sound() {
    out(b"DirectSound\n");
    let mut ds: *mut c_void = null_mut();
    check!(
        DirectSoundCreate8(null(), &mut ds, null_mut()) == 0,
        "DirectSoundCreate8"
    );
    if ds.is_null() {
        return;
    }
    check!(
        vcall!(ds, 6, fn(usize, u32) -> i32, 0, 2) == 0,
        "SetCooperativeLevel(PRIORITY)"
    );
    let fmt = pcm16(1, 22050);
    let mut desc = Bytes([0u8; 36]);
    desc.0[0..4].copy_from_slice(&36u32.to_le_bytes());
    // CTRLPOSITIONNOTIFY | CTRLVOLUME | CTRLPAN | GETCURRENTPOSITION2
    desc.0[4..8].copy_from_slice(&(0x100u32 | 0x80 | 0x40 | 0x10000).to_le_bytes());
    desc.0[8..12].copy_from_slice(&8820u32.to_le_bytes());
    desc.0[16..24].copy_from_slice(&(fmt.0.as_ptr() as usize).to_le_bytes());
    let mut buf: *mut c_void = null_mut();
    check!(
        vcall!(
            ds,
            3,
            fn(*const u8, *mut *mut c_void, *mut c_void) -> i32,
            desc.0.as_ptr(),
            &mut buf,
            null_mut()
        ) == 0,
        "CreateSoundBuffer"
    );
    if buf.is_null() {
        return;
    }
    let (mut p1, mut n1, mut p2, mut n2) = (null_mut::<u8>(), 0u32, null_mut::<u8>(), 0u32);
    let hr = vcall!(
        buf,
        11,
        fn(u32, u32, *mut *mut u8, *mut u32, *mut *mut u8, *mut u32, u32) -> i32,
        0,
        0,
        &mut p1,
        &mut n1,
        &mut p2,
        &mut n2,
        2
    );
    check!(hr == 0 && n1 == 8820 && p2.is_null(), "Lock(ENTIREBUFFER)");
    core::ptr::copy_nonoverlapping(pcm_ptr(), p1, 8820);
    check!(
        vcall!(
            buf,
            19,
            fn(*mut u8, u32, *mut u8, u32) -> i32,
            p1,
            n1,
            p2,
            n2
        ) == 0,
        "Unlock"
    );
    // IDirectSoundNotify
    let iid_notify = g(
        0xb0210783,
        0x89cd,
        0x11d0,
        [0xaf, 0x08, 0x00, 0xa0, 0xc9, 0x25, 0xcd, 0x16],
    );
    let mut notify: *mut c_void = null_mut();
    check!(
        vcall!(
            buf,
            0,
            fn(*const Guid, *mut *mut c_void) -> i32,
            &iid_notify,
            &mut notify
        ) == 0,
        "QueryInterface(IDirectSoundNotify)"
    );
    let ev = CreateEventA(null_mut(), 0, 0, null());
    let mut pos = Bytes([0u8; 16]);
    pos.0[0..4].copy_from_slice(&4410u32.to_le_bytes());
    pos.0[8..16].copy_from_slice(&(ev as usize).to_le_bytes());
    check!(
        vcall!(notify, 3, fn(u32, *const u8) -> i32, 1, pos.0.as_ptr()) == 0,
        "SetNotificationPositions"
    );
    check!(vcall!(buf, 16, fn(i32) -> i32, -2000) == 0, "SetPan");
    check!(
        vcall!(buf, 12, fn(u32, u32, u32) -> i32, 0, 0, 1) == 0,
        "Play(LOOPING)"
    );
    check!(
        WaitForSingleObject(ev, 3000) == 0,
        "notification à mi-tampon"
    );
    let (mut play, mut write) = (0u32, 0u32);
    check!(
        vcall!(buf, 4, fn(*mut u32, *mut u32) -> i32, &mut play, &mut write) == 0
            && play < 8820
            && write < 8820,
        "GetCurrentPosition"
    );
    check!(vcall!(buf, 18, fn() -> i32,) == 0, "Stop");
    vcall!(notify, 2, fn() -> u32,);
    vcall!(buf, 2, fn() -> u32,);
    check!(vcall!(ds, 2, fn() -> u32,) == 0, "Release du périphérique");
    CloseHandle(ev);
}

// ---------------------------------------------------------------------------

unsafe fn test_wasapi() {
    out(b"WASAPI\n");
    let clsid = g(
        0xbcde0395,
        0xe52f,
        0x467c,
        [0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e],
    );
    let iid_enum = g(
        0xa95664d2,
        0x9614,
        0x4f35,
        [0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6],
    );
    let iid_client = g(
        0x1cb9ad4c,
        0xdbfa,
        0x4c32,
        [0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2],
    );
    let iid_render = g(
        0xf294acfc,
        0x3146,
        0x4483,
        [0xa7, 0xbf, 0xad, 0xdc, 0xa7, 0xc2, 0x60, 0xe2],
    );
    CoInitializeEx(null_mut(), 0);
    let mut en: *mut c_void = null_mut();
    check!(
        CoCreateInstance(&clsid, null_mut(), 0x17, &iid_enum, &mut en) == 0,
        "CoCreateInstance(MMDeviceEnumerator)"
    );
    if en.is_null() {
        return;
    }
    let mut dev: *mut c_void = null_mut();
    check!(
        vcall!(en, 4, fn(u32, u32, *mut *mut c_void) -> i32, 0, 1, &mut dev) == 0,
        "GetDefaultAudioEndpoint(eRender)"
    );
    let mut ac: *mut c_void = null_mut();
    check!(
        vcall!(
            dev,
            3,
            fn(*const Guid, u32, *const c_void, *mut *mut c_void) -> i32,
            &iid_client,
            0x17,
            null(),
            &mut ac
        ) == 0,
        "Activate(IAudioClient)"
    );
    let mut mix: *mut u8 = null_mut();
    check!(
        vcall!(ac, 8, fn(*mut *mut u8) -> i32, &mut mix) == 0 && !mix.is_null(),
        "GetMixFormat"
    );
    let channels = u16::from_le_bytes([*mix.add(2), *mix.add(3)]);
    let block = u16::from_le_bytes([*mix.add(12), *mix.add(13)]) as usize;
    check!(channels == 2, "format de mixage stéréo");
    check!(
        vcall!(
            ac,
            3,
            fn(u32, u32, i64, i64, *const u8, *const Guid) -> i32,
            0,
            0x4_0000,
            0,
            0,
            mix,
            null()
        ) == 0,
        "Initialize(partagé, événementiel)"
    );
    let ev = CreateEventA(null_mut(), 0, 0, null());
    check!(vcall!(ac, 13, fn(Handle) -> i32, ev) == 0, "SetEventHandle");
    let mut frames = 0u32;
    vcall!(ac, 4, fn(*mut u32) -> i32, &mut frames);
    let mut rc: *mut c_void = null_mut();
    check!(
        vcall!(
            ac,
            14,
            fn(*const Guid, *mut *mut c_void) -> i32,
            &iid_render,
            &mut rc
        ) == 0,
        "GetService(IAudioRenderClient)"
    );
    let mut data: *mut u8 = null_mut();
    check!(
        vcall!(rc, 3, fn(u32, *mut *mut u8) -> i32, frames, &mut data) == 0,
        "GetBuffer"
    );
    core::ptr::write_bytes(data, 0, frames as usize * block);
    check!(
        vcall!(rc, 4, fn(u32, u32) -> i32, frames, 0) == 0,
        "ReleaseBuffer"
    );
    check!(vcall!(ac, 10, fn() -> i32,) == 0, "Start");
    let mut woke = 0;
    for _ in 0..5 {
        if WaitForSingleObject(ev, 2000) == 0 {
            woke += 1;
        }
    }
    check!(woke == 5, "cinq périodes signalées");
    let mut pad = 0u32;
    vcall!(ac, 6, fn(*mut u32) -> i32, &mut pad);
    check!(pad < frames, "le tampon se vide");
    check!(vcall!(ac, 11, fn() -> i32,) == 0, "Stop");
    vcall!(rc, 2, fn() -> u32,);
    vcall!(ac, 2, fn() -> u32,);
    vcall!(dev, 2, fn() -> u32,);
    vcall!(en, 2, fn() -> u32,);
    CoTaskMemFree(mix as *mut c_void);
    CloseHandle(ev);
}

// ---------------------------------------------------------------------------

/// IXAudio2VoiceCallback écrit dans le programme.
#[repr(C)]
struct VoiceCb {
    vt: *const usize,
    event: Handle,
}
static PASSES: AtomicU32 = AtomicU32::new(0);
static BUFFER_ENDS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "system" fn cb_pass_start(_c: *mut VoiceCb, _bytes: u32) {
    PASSES.fetch_add(1, Ordering::SeqCst);
}
unsafe extern "system" fn cb_nop0(_c: *mut VoiceCb) {}
unsafe extern "system" fn cb_nop1(_c: *mut VoiceCb, _ctx: usize) {}
unsafe extern "system" fn cb_stream_end(c: *mut VoiceCb) {
    SetEvent((*c).event);
}
unsafe extern "system" fn cb_buffer_end(_c: *mut VoiceCb, ctx: usize) {
    BUFFER_ENDS.store(ctx, Ordering::SeqCst);
}
unsafe extern "system" fn cb_error(_c: *mut VoiceCb, _ctx: usize, _hr: i32) {}

unsafe fn test_xaudio2() {
    out(b"XAudio2\n");
    let mut xa: *mut c_void = null_mut();
    check!(XAudio2Create(&mut xa, 0, 1) == 0, "XAudio2Create");
    if xa.is_null() {
        return;
    }
    let mut master: *mut c_void = null_mut();
    check!(
        vcall!(
            xa,
            7,
            fn(*mut *mut c_void, u32, u32, u32, *const u16, *const c_void, u32) -> i32,
            &mut master,
            0,
            0,
            0,
            null(),
            null(),
            6
        ) == 0,
        "CreateMasteringVoice"
    );
    let vt: [usize; 7] = [
        cb_pass_start as *const () as usize,
        cb_nop0 as *const () as usize,
        cb_stream_end as *const () as usize,
        cb_nop1 as *const () as usize,
        cb_buffer_end as *const () as usize,
        cb_nop1 as *const () as usize,
        cb_error as *const () as usize,
    ];
    let cb = VoiceCb {
        vt: vt.as_ptr(),
        event: CreateEventA(null_mut(), 0, 0, null()),
    };
    let fmt = pcm16(1, 22050);
    let mut src: *mut c_void = null_mut();
    check!(
        vcall!(
            xa,
            5,
            fn(
                *mut *mut c_void,
                *const u8,
                u32,
                f32,
                *const VoiceCb,
                *const c_void,
                *const c_void,
            ) -> i32,
            &mut src,
            fmt.0.as_ptr(),
            0,
            2.0,
            &cb,
            null(),
            null()
        ) == 0,
        "CreateSourceVoice avec rappel"
    );
    let mut b = Bytes([0u8; 48]);
    b.0[0..4].copy_from_slice(&0x40u32.to_le_bytes()); // END_OF_STREAM
    b.0[4..8].copy_from_slice(&8820u32.to_le_bytes());
    b.0[8..16].copy_from_slice(&(pcm_ptr() as usize).to_le_bytes());
    b.0[40..48].copy_from_slice(&0x5151usize.to_le_bytes());
    check!(
        vcall!(
            src,
            21,
            fn(*const u8, *const c_void) -> i32,
            b.0.as_ptr(),
            null()
        ) == 0,
        "SubmitSourceBuffer"
    );
    check!(
        vcall!(src, 12, fn(f32, u32) -> i32, 0.5, 0) == 0,
        "SetVolume"
    );
    check!(
        vcall!(src, 26, fn(f32, u32) -> i32, 1.5, 0) == 0,
        "SetFrequencyRatio"
    );
    check!(vcall!(src, 19, fn(u32, u32) -> i32, 0, 0) == 0, "Start");
    check!(
        WaitForSingleObject(cb.event, 3000) == 0,
        "OnStreamEnd appelé dans le programme"
    );
    check!(
        BUFFER_ENDS.load(Ordering::SeqCst) == 0x5151,
        "OnBufferEnd avec le bon contexte"
    );
    check!(
        PASSES.load(Ordering::SeqCst) > 5,
        "OnVoiceProcessingPassStart à chaque passe"
    );
    let mut state = Bytes([0u8; 24]);
    vcall!(src, 25, fn(*mut u8, u32) -> (), state.0.as_mut_ptr(), 0);
    let played = u64::from_le_bytes([
        state.0[16],
        state.0[17],
        state.0[18],
        state.0[19],
        state.0[20],
        state.0[21],
        state.0[22],
        state.0[23],
    ]);
    check!(played == 4410, "SamplesPlayed = 4410");
    vcall!(src, 18, fn() -> (),);
    vcall!(master, 18, fn() -> (),);
    check!(vcall!(xa, 2, fn() -> u32,) == 0, "Release du moteur");
    CloseHandle(cb.event);

    // XAudio2 2.7 : objet COM créé par CoCreateInstance puis Initialize.
    let clsid27 = g(
        0x5a508685,
        0xa254,
        0x4fba,
        [0x9b, 0x82, 0x9a, 0x24, 0xb0, 0x03, 0x06, 0xaf],
    );
    let iid27 = g(
        0x8bcf1f58,
        0x9fe7,
        0x4583,
        [0x8a, 0xc6, 0xe2, 0xad, 0xc4, 0x65, 0xc8, 0xbb],
    );
    let mut x27: *mut c_void = null_mut();
    check!(
        CoCreateInstance(&clsid27, null_mut(), 1, &iid27, &mut x27) == 0,
        "CoCreateInstance(XAudio2 2.7)"
    );
    if !x27.is_null() {
        let mut count = 0u32;
        check!(
            vcall!(x27, 3, fn(*mut u32) -> i32, &mut count) == 0 && count == 1,
            "GetDeviceCount"
        );
        check!(vcall!(x27, 5, fn(u32, u32) -> i32, 0, 1) == 0, "Initialize");
        let mut m27: *mut c_void = null_mut();
        check!(
            vcall!(
                x27,
                10,
                fn(*mut *mut c_void, u32, u32, u32, u32, *const c_void) -> i32,
                &mut m27,
                2,
                44100,
                0,
                0,
                null()
            ) == 0,
            "CreateMasteringVoice 2.7"
        );
        let mut details = [0u32; 3];
        vcall!(m27, 0, fn(*mut u32) -> (), details.as_mut_ptr());
        check!(
            details[1] == 2 && details[2] == 44100,
            "GetVoiceDetails 2.7"
        );
        vcall!(m27, 18, fn() -> (),);
        vcall!(x27, 2, fn() -> u32,);
    }
}

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe {
        fill_pcm();
        test_wave_out();
        test_direct_sound();
        test_wasapi();
        test_xaudio2();
    }
    finish()
}
