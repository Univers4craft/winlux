//! Utilitaires - stub module

use crate::gfx::*;
use crate::wm::{Frame, Gfx, Mouse};
use winlux_testexe::*;

pub const CUSTOM_TITLE: bool = false;

pub unsafe fn draw(_g: &mut Gfx, _f: &Frame) {}

pub unsafe fn mouse(_ev: Mouse, _x: i32, _y: i32, _f: &Frame) -> bool {
    false
}

pub unsafe fn key(_vk: u32, _ch: u16) -> bool {
    false
}

pub unsafe fn tick(_f: &Frame) -> bool {
    false
}
