//! Infos système - CPU, RAM, version Winlux

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    let r = f.client;
    fill(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        rgb(0x1E, 0x1E, 0x1E),
    );

    text(
        g.hdc,
        g.f.title,
        rgb(0xCC, 0xCC, 0xCC),
        r.left + 16,
        r.top + 16,
        b"Informations systeme",
    );

    let y = r.top + 50;
    let labels: [&[u8]; 6] = [
        b"Version Winlux",
        b"Version runtime",
        b"CPU",
        b"RAM",
        b"Resolution",
        b"Demarre depuis",
    ];

    let values: [&[u8]; 6] = [
        b"v0.2.3",
        b"0.1.0",
        b"x86_64 (4 coeurs)",
        b"4096 Mo",
        b"1920x1080",
        b"--",
    ];

    for i in 0..6 {
        text(
            g.hdc,
            g.f.body,
            rgb(0x88, 0x88, 0x88),
            r.left + 16,
            y + (i as i32) * 35,
            labels[i],
        );
        text(
            g.hdc,
            g.f.body,
            rgb(0xCC, 0xCC, 0xCC),
            r.left + 16,
            y + (i as i32) * 35 + 18,
            values[i],
        );
    }
}

pub unsafe fn mouse(_ev: Mouse, _x: i32, _y: i32, _f: &Frame) -> bool {
    false
}
