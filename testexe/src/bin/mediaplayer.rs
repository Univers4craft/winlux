//! Lecteur multimédia (vidéo et audio) : `mediaplayer.exe [fichier]`.
#![no_std]
#![no_main]

#[path = "../shell/gfx.rs"]
#[allow(dead_code)]
mod gfx;
#[path = "../shell/mediaplayer.rs"]
mod mediaplayer;

#[no_mangle]
pub extern "C" fn mainCRTStartup() -> ! {
    unsafe { mediaplayer::run(c"WinluxMediaPlayer") }
}
