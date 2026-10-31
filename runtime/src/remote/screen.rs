//! Accès à l'écran X11 du bureau Winlux : capture du framebuffer du serveur
//! X (GetImage sur la fenêtre racine), image du pointeur (XFIXES) et
//! injection du clavier et de la souris (XTEST), via le protocole X natif.

use std::collections::HashMap;

use x11rb::connection::Connection;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat, ImageOrder, Window};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

/// Image 32 bits : un pixel 0x00RRGGBB par u32, ligne par ligne.
#[derive(Clone)]
pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u32>,
}

/// Pointeur ARGB non prémultiplié et point chaud.
pub struct Cursor {
    pub width: u16,
    pub height: u16,
    pub xhot: u16,
    pub yhot: u16,
    pub argb: Vec<u32>,
}

pub trait Screen: Send {
    fn capture(&mut self) -> Option<Frame>;
    /// Nouveau pointeur depuis le dernier appel, s'il a changé.
    fn cursor(&mut self) -> Option<Cursor>;
}

pub trait Input: Send {
    fn key(&mut self, down: bool, keysym: u32);
    fn pointer(&mut self, buttons: u8, x: u16, y: u16);
}

fn mask_shift(mask: u32) -> (u32, u32) {
    if mask == 0 {
        return (0, 0);
    }
    let shift = mask.trailing_zeros();
    (shift, (mask >> shift).count_ones())
}

fn scale_to_8(v: u32, bits: u32) -> u32 {
    match bits {
        0 => 0,
        8 => v,
        b if b > 8 => v >> (b - 8),
        b => (v << (8 - b)) | (v >> (2 * b).saturating_sub(8).min(b)),
    }
}

pub struct X11Screen {
    conn: RustConnection,
    root: Window,
    masks: [(u32, u32); 3],
    bpp: u8,
    msb_first: bool,
    xfixes: bool,
    cursor_serial: u32,
}

impl X11Screen {
    pub fn open(display: &str) -> Result<X11Screen, String> {
        let (conn, screen_num) = x11rb::connect(Some(display))
            .map_err(|e| format!("connexion à l'écran {display} impossible : {e}"))?;
        let setup = conn.setup();
        let screen = &setup.roots[screen_num];
        let root = screen.root;
        let depth = screen.root_depth;
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.visual_id == screen.root_visual)
            .ok_or("visuel racine introuvable")?;
        let bpp = setup
            .pixmap_formats
            .iter()
            .find(|f| f.depth == depth)
            .map(|f| f.bits_per_pixel)
            .unwrap_or(32);
        if bpp != 32 && bpp != 16 && bpp != 24 {
            return Err(format!(
                "profondeur d'écran non prise en charge : {bpp} bits"
            ));
        }
        let masks = [
            mask_shift(visual.red_mask),
            mask_shift(visual.green_mask),
            mask_shift(visual.blue_mask),
        ];
        let msb_first = setup.image_byte_order == ImageOrder::MSB_FIRST;
        let xfixes = conn
            .xfixes_query_version(4, 0)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some();
        Ok(X11Screen {
            conn,
            root,
            masks,
            bpp,
            msb_first,
            xfixes,
            cursor_serial: 0,
        })
    }
}

impl Screen for X11Screen {
    fn capture(&mut self) -> Option<Frame> {
        let geo = self.conn.get_geometry(self.root).ok()?.reply().ok()?;
        let (w, h) = (geo.width, geo.height);
        let reply = self
            .conn
            .get_image(ImageFormat::Z_PIXMAP, self.root, 0, 0, w, h, !0)
            .ok()?
            .reply()
            .ok()?;
        let bytes_pp = (self.bpp / 8) as usize;
        let stride = (w as usize * bytes_pp).div_ceil(4) * 4;
        if reply.data.len() < stride * h as usize {
            return None;
        }
        let mut pixels = Vec::with_capacity(w as usize * h as usize);
        let standard = self.masks == [(16, 8), (8, 8), (0, 8)] && bytes_pp == 4 && !self.msb_first;
        for row in reply.data.chunks_exact(stride).take(h as usize) {
            if standard {
                pixels.extend(
                    row[..w as usize * 4]
                        .chunks_exact(4)
                        .map(|p| u32::from_le_bytes([p[0], p[1], p[2], 0])),
                );
                continue;
            }
            for p in row[..w as usize * bytes_pp].chunks_exact(bytes_pp) {
                let mut v = 0u32;
                for (i, &b) in p.iter().enumerate() {
                    let shift = if self.msb_first {
                        (bytes_pp - 1 - i) * 8
                    } else {
                        i * 8
                    };
                    v |= (b as u32) << shift;
                }
                let c = |(s, bits): (u32, u32)| scale_to_8((v >> s) & ((1 << bits) - 1), bits);
                pixels.push(c(self.masks[0]) << 16 | c(self.masks[1]) << 8 | c(self.masks[2]));
            }
        }
        Some(Frame {
            width: w,
            height: h,
            pixels,
        })
    }

    fn cursor(&mut self) -> Option<Cursor> {
        if !self.xfixes {
            return None;
        }
        let img = self.conn.xfixes_get_cursor_image().ok()?.reply().ok()?;
        if img.cursor_serial == self.cursor_serial || img.width == 0 || img.height == 0 {
            return None;
        }
        self.cursor_serial = img.cursor_serial;
        // XFIXES fournit de l'ARGB prémultiplié.
        let argb = img
            .cursor_image
            .iter()
            .map(|&p| {
                let a = p >> 24;
                if a == 0 || a == 255 {
                    return p;
                }
                let un = |c: u32| ((c * 255 + a / 2) / a).min(255);
                a << 24 | un((p >> 16) & 0xff) << 16 | un((p >> 8) & 0xff) << 8 | un(p & 0xff)
            })
            .collect();
        Some(Cursor {
            width: img.width,
            height: img.height,
            xhot: img.xhot.min(img.width - 1),
            yhot: img.yhot.min(img.height - 1),
            argb,
        })
    }
}

const XK_SHIFT_L: u32 = 0xffe1;
const XK_SHIFT_R: u32 = 0xffe2;
const XK_ISO_LEVEL3_SHIFT: u32 = 0xfe03;

pub struct X11Input {
    conn: RustConnection,
    root: Window,
    min_keycode: u8,
    per_keycode: usize,
    keysyms: Vec<u32>,
    /// keysym -> (keycode, niveau 0..3 : normal, Maj, AltGr, AltGr+Maj).
    lookup: HashMap<u32, (u8, u8)>,
    pressed: HashMap<u32, (u8, bool, bool)>,
    shift_down: bool,
    level3_down: bool,
    spare: Vec<u8>,
    buttons: u8,
}

impl X11Input {
    pub fn open(display: &str) -> Result<X11Input, String> {
        let (conn, screen_num) = x11rb::connect(Some(display))
            .map_err(|e| format!("connexion à l'écran {display} impossible : {e}"))?;
        conn.xtest_get_version(2, 2)
            .ok()
            .and_then(|c| c.reply().ok())
            .ok_or("extension XTEST absente : le contrôle à distance est impossible")?;
        let root = conn.setup().roots[screen_num].root;
        let mut input = X11Input {
            conn,
            root,
            min_keycode: 8,
            per_keycode: 0,
            keysyms: Vec::new(),
            lookup: HashMap::new(),
            pressed: HashMap::new(),
            shift_down: false,
            level3_down: false,
            spare: Vec::new(),
            buttons: 0,
        };
        input.load_mapping()?;
        Ok(input)
    }

    fn load_mapping(&mut self) -> Result<(), String> {
        let setup = self.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let reply = self
            .conn
            .get_keyboard_mapping(min, max - min + 1)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        self.min_keycode = min;
        self.per_keycode = reply.keysyms_per_keycode as usize;
        self.keysyms = reply.keysyms;
        self.lookup.clear();
        self.spare.clear();
        let per = self.per_keycode.max(1);
        for (i, syms) in self.keysyms.chunks(per).enumerate() {
            let code = min as usize + i;
            if code > 255 {
                break;
            }
            if syms.iter().all(|&s| s == 0) {
                self.spare.push(code as u8);
                continue;
            }
            // Colonnes X : 0 normal, 1 Maj, 2-3 groupe 2, 4 AltGr, 5 AltGr+Maj.
            for (col, level) in [(0usize, 0u8), (1, 1), (4, 2), (5, 3)] {
                if let Some(&sym) = syms.get(col) {
                    if sym != 0 {
                        self.lookup.entry(sym).or_insert((code as u8, level));
                    }
                }
            }
            if syms.len() == 1 || syms.get(1) == Some(&0) {
                // Une lettre seule couvre aussi sa majuscule.
                let lower = syms[0];
                if (0x61..=0x7a).contains(&lower) {
                    self.lookup.entry(lower - 0x20).or_insert((code as u8, 1));
                }
            }
        }
        Ok(())
    }

    fn fake(&self, kind: u8, detail: u8) {
        let _ = self
            .conn
            .xtest_fake_input(kind, detail, 0, self.root, 0, 0, 0);
    }

    fn modifier(&self, keysym: u32, down: bool) {
        if let Some(&(code, _)) = self.lookup.get(&keysym) {
            self.fake(if down { 2 } else { 3 }, code);
        }
    }

    /// Associe un keysym absent de la disposition à un keycode libre.
    fn remap(&mut self, keysym: u32) -> Option<(u8, u8)> {
        let code = *self.spare.first()?;
        let syms = vec![keysym; self.per_keycode.max(1)];
        self.conn
            .change_keyboard_mapping(1, code, self.per_keycode.max(1) as u8, &syms)
            .ok()?;
        let _ = self.conn.flush();
        self.spare.remove(0);
        self.spare.push(code);
        self.lookup.retain(|_, v| v.0 != code);
        self.lookup.insert(keysym, (code, 0));
        Some((code, 0))
    }
}

impl Input for X11Input {
    fn key(&mut self, down: bool, keysym: u32) {
        if keysym == XK_SHIFT_L || keysym == XK_SHIFT_R {
            self.shift_down = down;
        }
        if keysym == XK_ISO_LEVEL3_SHIFT {
            self.level3_down = down;
        }
        if !down {
            if let Some((code, shift_added, level3_added)) = self.pressed.remove(&keysym) {
                self.fake(3, code);
                if shift_added {
                    self.modifier(XK_SHIFT_L, false);
                }
                if level3_added {
                    self.modifier(XK_ISO_LEVEL3_SHIFT, false);
                }
                let _ = self.conn.flush();
            }
            return;
        }
        let Some((code, level)) = self
            .lookup
            .get(&keysym)
            .copied()
            .or_else(|| self.remap(keysym))
        else {
            return;
        };
        // Les caractères imprimables suivent la disposition de l'écran :
        // Maj et AltGr sont ajoutés ou retirés autour de la touche.
        let printable = !(0xff00..0x0100_0000).contains(&keysym);
        let mut shift_added = false;
        let mut level3_added = false;
        if printable {
            let need_shift = level & 1 == 1;
            let need_level3 = level >= 2;
            if need_shift && !self.shift_down {
                self.modifier(XK_SHIFT_L, true);
                shift_added = true;
            } else if !need_shift && self.shift_down {
                self.modifier(XK_SHIFT_L, false);
                self.modifier(XK_SHIFT_R, false);
            }
            if need_level3 && !self.level3_down {
                self.modifier(XK_ISO_LEVEL3_SHIFT, true);
                level3_added = true;
            }
        }
        self.fake(2, code);
        if printable && !(level & 1 == 1) && self.shift_down {
            self.modifier(XK_SHIFT_L, true);
        }
        self.pressed
            .insert(keysym, (code, shift_added, level3_added));
        let _ = self.conn.flush();
    }

    fn pointer(&mut self, buttons: u8, x: u16, y: u16) {
        let _ = self
            .conn
            .xtest_fake_input(6, 0, 0, self.root, x as i16, y as i16, 0);
        for b in 0..8 {
            let bit = 1u8 << b;
            if (buttons ^ self.buttons) & bit != 0 {
                self.fake(if buttons & bit != 0 { 4 } else { 5 }, b + 1);
            }
        }
        self.buttons = buttons;
        let _ = self.conn.flush();
    }
}

impl Drop for X11Input {
    fn drop(&mut self) {
        // Ne laisser aucune touche ni aucun bouton enfoncé après la déconnexion.
        let codes: Vec<u8> = self.pressed.values().map(|v| v.0).collect();
        for code in codes {
            self.fake(3, code);
        }
        for b in 0..8 {
            if self.buttons & (1 << b) != 0 {
                self.fake(5, b + 1);
            }
        }
        let _ = self.conn.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masques_et_echelle() {
        assert_eq!(mask_shift(0xff0000), (16, 8));
        assert_eq!(mask_shift(0xf800), (11, 5));
        assert_eq!(scale_to_8(31, 5), 255);
        assert_eq!(scale_to_8(0, 5), 0);
        assert_eq!(scale_to_8(63, 6), 255);
    }
}
