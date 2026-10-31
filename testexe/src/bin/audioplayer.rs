//! Lecteur audio : même lecteur et même moteur que `mediaplayer.exe`.
#![no_std]
#![no_main]

#[path = "../shell/gfx.rs"]
#[allow(dead_code)]
mod gfx;
#[path = "../shell/mediaplayer.rs"]
mod mediaplayer;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe { mediaplayer::run(c"WinluxAudioPlayer") }
}
