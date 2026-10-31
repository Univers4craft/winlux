//! Types WM minimaux pour les binaires autonomes (sans le bureau complet).
//! Remplace wm.rs qui dépend de crate::explorer, crate::settings, etc.

use crate::gfx::{Brushes, Fonts};
use winlux_testexe::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Mouse {
    Down,
    Up,
    Move,
    Wheel(i32),
    DoubleClick,
    RightDown,
    MiddleDown,
    LeftClick,
    LeftDown,
    LeftDrag,
    LeftUp,
}

pub struct Frame {
    pub outer: Rect,
    pub title: Rect,
    pub client: Rect,
}

impl Frame {
    pub unsafe fn new(hwnd: Handle) -> Self {
        let mut r: Rect = core::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        Frame {
            outer: r,
            title: Rect {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.top,
            },
            client: r,
        }
    }
}

pub fn resize_frame(_f: &mut Frame) {}

pub struct Gfx<'a> {
    pub hdc: Handle,
    pub b: &'a mut Brushes,
    pub f: &'a Fonts,
}
