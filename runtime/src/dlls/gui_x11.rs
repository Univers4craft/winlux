//! Backend X11 de Winlux : crée de vraies fenêtres sur le serveur X et traduit
//! ses événements en messages Win32. Le fond utilise le thème « blob »
//! violet-bleu (dégradé vertical + accent) défini dans `gui::theme`.

use crate::dlls::gui::{self, DrawCmd, PendingEvent, Rect};
use crate::dlls::gui::theme;
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeGCAux, ConfigureNotifyEvent, ConfigureWindowAux, ConnectionExt as _,
    CreateGCAux, CreateWindowAux, Gcontext, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

#[derive(Clone, Copy)]
struct XWin {
    window: Window,
    gc: Gcontext,
    width: u16,
    height: u16,
}

pub struct X11Backend {
    conn: RustConnection,
    screen: usize,
    colormap: u32,
    wm_protocols: Atom,
    wm_delete: Atom,
    /// hwnd → fenêtre X.
    windows: HashMap<usize, XWin>,
    /// Fenêtre X → hwnd (pour router les événements).
    by_xwin: HashMap<u32, usize>,
    /// Événements lus pendant `wait`, consommés par le prochain `pump`.
    pending: Vec<Event>,
    /// Dernière liste de commandes GDI par fenêtre, rejouée à chaque Expose.
    content: HashMap<usize, Vec<DrawCmd>>,
    /// Police « core » utilisée pour le texte (ouverte à la première demande).
    font: Option<u32>,
    /// Table de correspondance keycode X11 → keysyms (lue à la demande).
    keymap: HashMap<u8, Vec<u32>>,
}

impl X11Backend {
    /// Ouvre la connexion X et prépare les atomes; `None` si pas de serveur.
    pub fn connect() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let setup = conn.setup();
        let scr = &setup.roots[screen];
        let colormap = scr.default_colormap;
        let wm_protocols = conn.intern_atom(false, b"WM_PROTOCOLS").ok()?.reply().ok()?.atom;
        let wm_delete = conn.intern_atom(false, b"WM_DELETE_WINDOW").ok()?.reply().ok()?.atom;
        Some(X11Backend {
            conn,
            screen,
            colormap,
            wm_protocols,
            wm_delete,
            windows: HashMap::new(),
            by_xwin: HashMap::new(),
            pending: Vec::new(),
            content: HashMap::new(),
            font: None,
            keymap: HashMap::new(),
        })
    }

    /// Alloue un pixel vrai-color depuis un COLORREF du thème.
    fn alloc_color(&self, colorref: u32) -> u32 {
        let rgb = theme::colorref_to_rgb(colorref);
        let r = (((rgb >> 16) & 0xFF) as u16) * 257;
        let g = (((rgb >> 8) & 0xFF) as u16) * 257;
        let b = ((rgb & 0xFF) as u16) * 257;
        match self.conn.alloc_color(self.colormap, r, g, b) {
            Ok(reply) => match reply.reply() {
                Ok(c) => c.pixel,
                Err(_) => rgb,
            },
            Err(_) => rgb,
        }
    }

    /// Dessine le thème « blob » : dégradé vertical violet→bleu + accent.
    fn paint_theme(&self, w: &XWin) {
        let bands = 32u32;
        let h = w.height.max(1) as u32;
        for i in 0..bands {
            let t = i as f32 / (bands - 1) as f32;
            let c = theme::lerp_colorref(theme::GRADIENT_TOP, theme::GRADIENT_BOTTOM, t);
            let pixel = self.alloc_color(c);
            let _ = self.conn.change_gc(w.gc, &ChangeGCAux::new().foreground(pixel));
            let y = (i * h / bands) as i16;
            let band_h = ((i + 1) * h / bands - i * h / bands).max(1) as u16;
            let _ = self.conn.poly_fill_rectangle(
                w.window,
                w.gc,
                &[x11rb::protocol::xproto::Rectangle {
                    x: 0,
                    y,
                    width: w.width,
                    height: band_h,
                }],
            );
        }
        // Accent « blob » : une ellipse centrée en magenta-violet.
        let accent = self.alloc_color(theme::ACCENT_VIOLET);
        let _ = self.conn.change_gc(w.gc, &ChangeGCAux::new().foreground(accent));
        let size = w.width.min(w.height).saturating_mul(2) / 3;
        if size > 8 {
            let cx = w.width as i16 / 2;
            let cy = w.height as i16 / 2;
            let arc = x11rb::protocol::xproto::Arc {
                x: cx - size as i16 / 2,
                y: cy - size as i16 / 2,
                width: size,
                height: size,
                angle1: 0,
                angle2: 360 * 64,
            };
            let _ = self.conn.poly_fill_arc(w.window, w.gc, &[arc]);
        }
        let _ = self.conn.flush();
    }

    /// Convertit un rectangle logique en rectangle X rogné à la fenêtre.
    fn to_x_rect(w: &XWin, r: &Rect) -> Option<x11rb::protocol::xproto::Rectangle> {
        let x0 = r.left.max(0).min(w.width as i32);
        let y0 = r.top.max(0).min(w.height as i32);
        let x1 = r.right.max(0).min(w.width as i32);
        let y1 = r.bottom.max(0).min(w.height as i32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(x11rb::protocol::xproto::Rectangle {
            x: x0 as i16,
            y: y0 as i16,
            width: (x1 - x0) as u16,
            height: (y1 - y0) as u16,
        })
    }

    fn set_foreground(&self, w: &XWin, color: u32) {
        let pixel = self.alloc_color(color);
        let _ = self.conn.change_gc(w.gc, &ChangeGCAux::new().foreground(pixel));
    }

    /// Ouvre (une seule fois) la police « fixed » du serveur X.
    fn ensure_font(&mut self) -> Option<u32> {
        if let Some(f) = self.font {
            return Some(f);
        }
        let fid = self.conn.generate_id().ok()?;
        // « fixed » est un alias quasi universel; sinon on tente 6x13.
        for name in [&b"fixed"[..], &b"6x13"[..]] {
            if let Ok(cookie) = self.conn.open_font(fid, name) {
                cookie.check().ok()?;
                self.font = Some(fid);
                return Some(fid);
            }
        }
        None
    }

    /// Keysyms X11 associés à un keycode (table de niveaux chargée à la demande).
    fn key_syms(&mut self, keycode: u8) -> Option<Vec<u32>> {
        if !self.keymap.contains_key(&keycode) {
            let reply = self.conn.get_keyboard_mapping(keycode, 1).ok()?.reply().ok()?;
            self.keymap.insert(keycode, reply.keysyms);
        }
        self.keymap.get(&keycode).cloned()
    }

    fn paint_fill(&self, w: &XWin, r: &Rect, color: u32) {
        if let Some(xr) = Self::to_x_rect(w, r) {
            self.set_foreground(w, color);
            let _ = self.conn.poly_fill_rectangle(w.window, w.gc, &[xr]);
        }
    }

    fn paint_frame(&self, w: &XWin, r: &Rect, color: u32, width: u16) {
        self.set_foreground(w, color);
        let t = width.max(1) as i32;
        let edges = [
            Rect { left: r.left, top: r.top, right: r.right, bottom: r.top + t },
            Rect { left: r.left, top: r.bottom - t, right: r.right, bottom: r.bottom },
            Rect { left: r.left, top: r.top + t, right: r.left + t, bottom: r.bottom - t },
            Rect { left: r.right - t, top: r.top + t, right: r.right, bottom: r.bottom - t },
        ];
        let xs: Vec<_> = edges.iter().filter_map(|e| Self::to_x_rect(w, e)).collect();
        if !xs.is_empty() {
            let _ = self.conn.poly_fill_rectangle(w.window, w.gc, &xs);
        }
    }

    fn paint_ellipse(&self, w: &XWin, r: &Rect, color: u32) {
        if let Some(xr) = Self::to_x_rect(w, r) {
            self.set_foreground(w, color);
            let arc = x11rb::protocol::xproto::Arc {
                x: xr.x,
                y: xr.y,
                width: xr.width,
                height: xr.height,
                angle1: 0,
                angle2: 360 * 64,
            };
            let _ = self.conn.poly_fill_arc(w.window, w.gc, &[arc]);
        }
    }

    /// Pose un texte ANSI. `bk` est la couleur de fond (`Some`) quand le mode
    /// de fond est opaque, ou `None` pour un fond transparent : seul le fond
    /// décide du primitive X11 employée (`image_text8` repeint le fond,
    /// `poly_text8` ne pose que les glyphes).
    fn paint_text(&mut self, w: &XWin, x: i32, y: i32, text: &str, color: u32, bk: Option<u32>) {
        let font = match self.ensure_font() {
            Some(f) => f,
            None => return,
        };
        let pixel = self.alloc_color(color);
        let mut aux = ChangeGCAux::new().foreground(pixel).font(font);
        let opaque = match bk {
            Some(bg) => {
                aux = aux.background(self.alloc_color(bg));
                true
            }
            None => false,
        };
        let _ = self.conn.change_gc(w.gc, &aux);
        // `y` est la ligne de base ; la police `fixed` monte d'environ 11 px.
        let mut bytes = text.as_bytes();
        if bytes.len() > 255 {
            bytes = &bytes[..255];
        }
        if bytes.is_empty() {
            return;
        }
        let px = x.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let py = (y + 11).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        if opaque {
            let _ = self.conn.image_text8(w.window, w.gc, px, py, bytes);
        } else {
            // Élément TEXT8 : nombre d'octets, puis déplacement (0) et glyphes.
            // Le serveur X attend `n` avant `delta` ; le tampon est complété à
            // un multiple de 4 octets (éléments vides ignorés).
            let mut item = Vec::with_capacity(bytes.len() + 4);
            item.push(bytes.len() as u8);
            item.push(0u8);
            item.extend_from_slice(bytes);
            while item.len() % 4 != 0 {
                item.push(0);
            }
            let _ = self.conn.poly_text8(w.window, w.gc, px, py, &item);
        }
    }

    /// Rejoue la liste de commandes GDI d'une fenêtre.
    fn paint_cmds(&mut self, w: &XWin, cmds: &[DrawCmd]) {
        for cmd in cmds {
            match cmd {
                DrawCmd::FillRect { rect, color } => self.paint_fill(w, rect, *color),
                DrawCmd::FrameRect { rect, color, width } => {
                    self.paint_frame(w, rect, *color, *width)
                }
                DrawCmd::Ellipse { rect, color } => self.paint_ellipse(w, rect, *color),
                DrawCmd::Text { x, y, text, color, bk } => {
                    self.paint_text(w, *x, *y, text, *color, *bk)
                }
            }
        }
    }

    fn translate(&mut self, ev: Event) -> Option<PendingEvent> {
        match ev {
            Event::Expose(e) => {
                let hwnd = *self.by_xwin.get(&e.window)?;
                // count > 0 : d'autres Expose suivent, on attend le dernier.
                if e.count == 0 {
                    if let Some(w) = self.windows.get(&hwnd).copied() {
                        self.paint_theme(&w);
                        let content = self.content.get(&hwnd).cloned().unwrap_or_default();
                        self.paint_cmds(&w, &content);
                        let _ = self.conn.flush();
                    }
                }
                Some(PendingEvent { hwnd, message: gui::WM_PAINT, wparam: 0, lparam: 0 })
            }
            Event::ConfigureNotify(e) => self.on_configure(e),
            Event::ClientMessage(e) => {
                let hwnd = *self.by_xwin.get(&e.window)?;
                let data = e.data.as_data32();
                if e.type_ == self.wm_protocols && data[0] == self.wm_delete {
                    Some(PendingEvent { hwnd, message: gui::WM_CLOSE, wparam: 0, lparam: 0 })
                } else {
                    None
                }
            }
            Event::ButtonPress(e) => {
                let hwnd = *self.by_xwin.get(&e.event)?;
                let lparam = ((e.event_y as isize) << 16) | (e.event_x as isize & 0xFFFF);
                let wparam = button_wparam(e.detail);
                Some(PendingEvent { hwnd, message: gui::WM_LBUTTONDOWN, wparam, lparam })
            }
            Event::ButtonRelease(e) => {
                let hwnd = *self.by_xwin.get(&e.event)?;
                let lparam = ((e.event_y as isize) << 16) | (e.event_x as isize & 0xFFFF);
                let wparam = button_wparam(e.detail);
                Some(PendingEvent { hwnd, message: gui::WM_LBUTTONUP, wparam, lparam })
            }
            Event::MotionNotify(e) => {
                let hwnd = *self.by_xwin.get(&e.event)?;
                let lparam = ((e.event_y as isize) << 16) | (e.event_x as isize & 0xFFFF);
                Some(PendingEvent { hwnd, message: gui::WM_MOUSEMOVE, wparam: 0, lparam })
            }
            Event::KeyPress(e) => {
                let hwnd = *self.by_xwin.get(&e.event)?;
                let shift = e.state.contains(x11rb::protocol::xproto::KeyButMask::SHIFT);
                let ctrl = e.state.contains(x11rb::protocol::xproto::KeyButMask::CONTROL);
                let caps = e.state.contains(x11rb::protocol::xproto::KeyButMask::LOCK);
                let syms = self.key_syms(e.detail);
                let base = syms.as_deref().and_then(base_keysym);
                let sym = syms.as_deref().and_then(|s| effective_keysym(s, shift, caps));
                // Le VK désigne la touche physique (niveau non modifié), tandis
                // que le caractère tient compte de Maj/Caps Lock.
                let vk = base.and_then(keysym_to_vk).unwrap_or(e.detail as usize);
                let extended = vk_is_extended(vk);
                let lparam = key_lparam(e.detail, false, extended);
                // Caractère effectif pour `TranslateMessage` ; Ctrl+lettre ne
                // produit pas de caractère (raccourci clavier).
                let ch = if ctrl { None } else { sym.map(keysym_to_char).filter(|c| *c != 0) };
                gui::set_key_char(hwnd, vk, lparam, ch);
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_KEYDOWN,
                    wparam: vk,
                    lparam,
                })
            }
            Event::KeyRelease(e) => {
                let hwnd = *self.by_xwin.get(&e.event)?;
                let base = self.key_syms(e.detail).as_deref().and_then(base_keysym);
                let vk = base.and_then(keysym_to_vk).unwrap_or(e.detail as usize);
                Some(PendingEvent {
                    hwnd,
                    message: gui::WM_KEYUP,
                    wparam: vk,
                    lparam: key_lparam(e.detail, true, vk_is_extended(vk)),
                })
            }
            _ => None,
        }
    }

    fn on_configure(&mut self, e: ConfigureNotifyEvent) -> Option<PendingEvent> {
        let hwnd = *self.by_xwin.get(&e.window)?;
        let (old_w, old_h) = {
            let w = self.windows.get(&hwnd)?;
            (w.width, w.height)
        };
        if let Some(w) = self.windows.get_mut(&hwnd) {
            w.width = e.width;
            w.height = e.height;
        }
        if e.width != old_w || e.height != old_h {
            let lparam = ((e.height as isize) << 16) | (e.width as isize & 0xFFFF);
            return Some(PendingEvent { hwnd, message: gui::WM_SIZE, wparam: 0, lparam });
        }
        None
    }
}

/// Traduit le bouton X11 en bits `wParam` Win32 (MK_LBUTTON…).
fn button_wparam(detail: u8) -> usize {
    match detail {
        1 => 0x0001, // MK_LBUTTON
        2 => 0x0002, // MK_MBUTTON
        3 => 0x0010, // MK_RBUTTON
        _ => 0,
    }
}

/// Convertit un keysym X11 en code de touche virtuelle Win32 (`VK_*`).
fn keysym_to_vk(sym: u32) -> Option<usize> {
    let vk = match sym {
        0xFF08 => gui::VK_BACK,
        0xFF09 => gui::VK_TAB,
        0xFF0D => gui::VK_RETURN,
        0xFF1B => gui::VK_ESCAPE,
        0xFFFF => 0x2E,                                  // VK_DELETE
        0xFF51 => 0x25,                                  // VK_LEFT
        0xFF52 => 0x26,                                  // VK_UP
        0xFF53 => 0x27,                                  // VK_RIGHT
        0xFF54 => 0x28,                                  // VK_DOWN
        0xFF50 => 0x24,                                  // VK_HOME
        0xFF57 => 0x23,                                  // VK_END
        0xFF55 => 0x21,                                  // VK_PRIOR
        0xFF56 => 0x22,                                  // VK_NEXT
        0xFF63 => 0x2D,                                  // VK_INSERT
        0xFFBE..=0xFFC9 => 0x70 + (sym - 0xFFBE) as usize, // VK_F1..VK_F12
        0xFFE1 | 0xFFE2 => 0x10,                          // VK_SHIFT
        0xFFE3 | 0xFFE4 => 0x11,                          // VK_CONTROL
        0xFFE9 | 0xFFEA => 0x12,                          // VK_MENU
        0xFFE5 => 0x14,                                   // VK_CAPITAL
        // ASCII imprimable : VK_A..VK_Z, chiffres et touches OEM Windows.
        0x20 => gui::VK_SPACE,
        0x21..=0x7E => {
            let c = sym as u8;
            match c {
                b'0'..=b'9' | b'A'..=b'Z' => c as usize,
                b'a'..=b'z' => (c - b'a' + b'A') as usize,
                b'!' => b'1' as usize,
                b'@' => b'2' as usize,
                b'#' => b'3' as usize,
                b'$' => b'4' as usize,
                b'%' => b'5' as usize,
                b'^' => b'6' as usize,
                b'&' => b'7' as usize,
                b'*' => b'8' as usize,
                b'(' => b'9' as usize,
                b')' => b'0' as usize,
                b'-' | b'_' => 0xBD, // VK_OEM_MINUS
                b'=' | b'+' => 0xBB, // VK_OEM_PLUS
                b'[' | b'{' => 0xDB, // VK_OEM_4
                b']' | b'}' => 0xDD, // VK_OEM_6
                b'\\' | b'|' => 0xDC, // VK_OEM_5
                b';' | b':' => 0xBA, // VK_OEM_1
                b'\'' | b'"' => 0xDE, // VK_OEM_7
                b',' | b'<' => 0xBC, // VK_OEM_COMMA
                b'.' | b'>' => 0xBE, // VK_OEM_PERIOD
                b'/' | b'?' => 0xBF, // VK_OEM_2
                b'`' | b'~' => 0xC0, // VK_OEM_3
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(vk)
}

/// Premier keysym non nul : VK codes restent stables sous Maj et Caps Lock.
fn base_keysym(syms: &[u32]) -> Option<u32> {
    syms.first().copied().filter(|sym| *sym != 0)
}

/// Sélectionne le niveau effectif d'une touche, en appliquant Caps Lock aux
/// lettres mais pas aux chiffres ni à la ponctuation.
fn effective_keysym(syms: &[u32], shift: bool, caps: bool) -> Option<u32> {
    let base = base_keysym(syms)?;
    let is_letter = matches!(base, 0x41..=0x5A | 0x61..=0x7A);
    let level = (if is_letter { shift ^ caps } else { shift }) as usize;
    syms.get(level)
        .copied()
        .filter(|sym| *sym != 0)
        .or(Some(base))
}

fn vk_is_extended(vk: usize) -> bool {
    matches!(vk, 0x21..=0x2E)
}

/// Caractère produit par un keysym, 0 si la touche n'en produit pas.
fn keysym_to_char(sym: u32) -> u16 {
    match sym {
        0xFF08 => 0x08, // Retour arrière
        0xFF09 => 0x09, // Tabulation
        0xFF0D => 0x0D, // Entrée
        0xFF1B => 0x1B, // Échap
        0x20..=0x7E => sym as u16,
        0xA0..=0xFF => sym as u16, // Latin-1 (é, à, ç…)
        0x0100_0000..=0x0110_FFFF => {
            let codepoint = sym & 0x00FF_FFFF;
            char::from_u32(codepoint)
                .and_then(|ch| u16::try_from(ch as u32).ok())
                .unwrap_or(0)
        }
        _ => 0,
    }
}

/// `lParam` d'un message clavier : une répétition, code matériel, transition.
fn key_lparam(keycode: u8, up: bool, extended: bool) -> isize {
    let scan = ((keycode as isize) << 16) | if extended { 1 << 24 } else { 0 };
    if up {
        scan | (1 << 30) | (1 << 31)
    } else {
        scan | 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_keysyms_deviennent_des_codes_vk_win32() {
        assert_eq!(keysym_to_vk(b'a' as u32), Some(0x41));
        assert_eq!(keysym_to_vk(b'A' as u32), Some(0x41));
        assert_eq!(keysym_to_vk(b'7' as u32), Some(0x37));
        assert_eq!(keysym_to_vk(b';' as u32), Some(0xBA)); // VK_OEM_1
        assert_eq!(keysym_to_vk(b'!' as u32), Some(0x31)); // touche 1
        assert_eq!(keysym_to_vk(b'?' as u32), Some(0xBF)); // VK_OEM_2
        assert_eq!(keysym_to_vk(0xFF53), Some(0x27)); // VK_RIGHT
        assert_eq!(keysym_to_vk(0xFFC2), Some(0x74)); // VK_F5
        assert_eq!(keysym_to_vk(0xFFE3), Some(0x11)); // VK_CONTROL
        assert_eq!(keysym_to_vk(0x1008_FF12), None);
    }

    #[test]
    fn niveaux_clavier_gerent_maj_caps_et_ponctuation() {
        let letter = [b'a' as u32, b'A' as u32];
        assert_eq!(effective_keysym(&letter, false, false), Some(b'a' as u32));
        assert_eq!(effective_keysym(&letter, true, false), Some(b'A' as u32));
        assert_eq!(effective_keysym(&letter, false, true), Some(b'A' as u32));
        assert_eq!(effective_keysym(&letter, true, true), Some(b'a' as u32));

        let digit = [b'1' as u32, b'!' as u32];
        assert_eq!(effective_keysym(&digit, false, true), Some(b'1' as u32));
        assert_eq!(effective_keysym(&digit, true, false), Some(b'!' as u32));
        assert_eq!(base_keysym(&digit), Some(b'1' as u32));
    }

    #[test]
    fn caracteres_et_parametres_de_messages_clavier_sont_traduits() {
        assert_eq!(keysym_to_char(b'!' as u32), b'!' as u16);
        assert_eq!(keysym_to_char(0xE9), 0xE9); // é Latin-1
        assert_eq!(keysym_to_char(0x0100_00E9), 0xE9); // é Unicode keysym
        assert_eq!(keysym_to_char(0x0101_F600), 0); // supplementary plane non pris en charge
        assert_eq!(keysym_to_char(0xFF0D), 0x0D); // Entrée
        assert_eq!(keysym_to_char(0xFF51), 0); // flèche : pas de WM_CHAR

        assert_eq!(key_lparam(38, false, false), (38isize << 16) | 1);
        assert_eq!(
            key_lparam(113, true, true),
            (113isize << 16) | (1 << 24) | (1 << 30) | (1 << 31),
        );
    }
}

impl gui::GuiBackend for X11Backend {
    fn pump(&mut self) -> Vec<PendingEvent> {
        // Événements déjà lus pendant `wait`, puis lecture non bloquante.
        let mut evs: Vec<Event> = std::mem::take(&mut self.pending);
        while let Ok(Some(ev)) = self.conn.poll_for_event() {
            evs.push(ev);
        }
        let mut out = Vec::new();
        for ev in evs {
            if let Some(p) = self.translate(ev) {
                out.push(p);
            }
        }
        out
    }

    fn wait(&mut self, timeout_ms: u32) -> bool {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        loop {
            match self.conn.poll_for_event() {
                Ok(Some(ev)) => self.pending.push(ev),
                Ok(None) => {}
                // Connexion perdue : on laisse la boucle de messages se vider.
                Err(_) => return !self.pending.is_empty(),
            }
            if !self.pending.is_empty() {
                return true;
            }
            if Instant::now() >= deadline {
                // Backend réel : GetMessage doit continuer d'attendre.
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn create_window(&mut self, hwnd: usize, title: &str, rect: Rect) {
        let setup = self.conn.setup();
        let scr = &setup.roots[self.screen];
        // Profondeur CopyFromParent (0) : le serveur prend celle de la racine.
        let visual = scr.root_visual;

        let x = rect.left.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let y = rect.top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        let w = rect.width().max(1).clamp(1, u16::MAX as i32) as u16;
        let h = rect.height().max(1).clamp(1, u16::MAX as i32) as u16;

        let win = match self.conn.generate_id() {
            Ok(id) => id,
            Err(_) => return,
        };
        let bg = self.alloc_color(theme::BG_DEEP);
        let aux = CreateWindowAux::new()
            .background_pixel(bg)
            .event_mask(
                x11rb::protocol::xproto::EventMask::EXPOSURE
                    | x11rb::protocol::xproto::EventMask::STRUCTURE_NOTIFY
                    | x11rb::protocol::xproto::EventMask::BUTTON_PRESS
                    | x11rb::protocol::xproto::EventMask::BUTTON_RELEASE
                    | x11rb::protocol::xproto::EventMask::POINTER_MOTION
                    | x11rb::protocol::xproto::EventMask::KEY_PRESS
                    | x11rb::protocol::xproto::EventMask::KEY_RELEASE,
            );

        if self.conn
            .create_window(
                0, // CopyFromParent depth
                win,
                scr.root,
                x,
                y,
                w,
                h,
                0, // border
                WindowClass::INPUT_OUTPUT,
                visual,
                &aux,
            )
            .is_err()
        {
            return;
        }

        // Demande la fermeture propre via WM_DELETE_WINDOW.
        let _ = self.conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win,
            self.wm_protocols,
            AtomEnum::ATOM,
            &[self.wm_protocols, self.wm_delete],
        );
        let title_bytes = crate::dlls::encode_ansi(title);
        let _ = self.conn.change_property8(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            &title_bytes,
        );

        let gc = match self.conn.generate_id() {
            Ok(id) => id,
            Err(_) => return,
        };
        let _ = self.conn.create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0));

        let _ = self.conn.flush();
        self.windows.insert(hwnd, XWin { window: win, gc, width: w, height: h });
        self.by_xwin.insert(win, hwnd);
    }

    fn draw(&mut self, hwnd: usize, cmds: &[DrawCmd]) {
        self.content.insert(hwnd, cmds.to_vec());
        if let Some(w) = self.windows.get(&hwnd).copied() {
            self.paint_theme(&w);
            self.paint_cmds(&w, cmds);
            let _ = self.conn.flush();
        }
    }

    fn destroy_window(&mut self, hwnd: usize) {
        self.content.remove(&hwnd);
        if let Some(w) = self.windows.remove(&hwnd) {
            self.by_xwin.remove(&w.window);
            let _ = self.conn.free_gc(w.gc);
            let _ = self.conn.destroy_window(w.window);
            let _ = self.conn.flush();
        }
    }

    fn show_window(&mut self, hwnd: usize, show: bool) {
        if let Some(w) = self.windows.get(&hwnd) {
            if show {
                let _ = self.conn.map_window(w.window);
            } else {
                let _ = self.conn.unmap_window(w.window);
            }
            let _ = self.conn.flush();
        }
    }

    fn set_title(&mut self, hwnd: usize, title: &str) {
        if let Some(w) = self.windows.get(&hwnd) {
            let title_bytes = crate::dlls::encode_ansi(title);
            let _ = self.conn.change_property8(
                x11rb::protocol::xproto::PropMode::REPLACE,
                w.window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                &title_bytes,
            );
            let _ = self.conn.flush();
        }
    }

    fn set_window_rect(&mut self, hwnd: usize, rect: Rect) {
        let x = rect.left.clamp(i16::MIN as i32, i16::MAX as i32);
        let y = rect.top.clamp(i16::MIN as i32, i16::MAX as i32);
        let width = rect.width().max(1).min(u16::MAX as i32) as u16;
        let height = rect.height().max(1).min(u16::MAX as i32) as u16;
        let window = match self.windows.get_mut(&hwnd) {
            Some(w) => {
                w.width = width;
                w.height = height;
                w.window
            }
            None => return,
        };
        let aux = ConfigureWindowAux::new()
            .x(x)
            .y(y)
            .width(width as u32)
            .height(height as u32);
        let _ = self.conn.configure_window(window, &aux);
        let _ = self.conn.flush();
    }

    fn invalidate(&mut self, hwnd: usize) {
        if let Some(w) = self.windows.get(&hwnd) {
            let _ = self.conn.clear_area(false, w.window, 0, 0, 0, 0);
            let _ = self.conn.flush();
        }
    }
}
