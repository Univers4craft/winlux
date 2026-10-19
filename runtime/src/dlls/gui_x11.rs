//! Backend X11 de Winlux : crée de vraies fenêtres sur le serveur X et traduit
//! ses événements en messages Win32. Le fond utilise le thème « blob »
//! violet-bleu (dégradé vertical + accent) défini dans `gui::theme`.

use crate::dlls::gui::{self, PendingEvent, Rect};
use crate::dlls::gui::theme;
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeGCAux, ConfigureNotifyEvent, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, Gcontext, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

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

    fn translate(&mut self, ev: Event) -> Option<PendingEvent> {
        match ev {
            Event::Expose(e) => {
                let hwnd = *self.by_xwin.get(&e.window)?;
                crate::trace!("X11 expose hwnd={} count={}", hwnd, e.count);
                // Redessin du thème quand la zone est exposée.
                if let Some(w) = self.windows.get(&hwnd) {
                    let wref = XWin { window: w.window, gc: w.gc, width: w.width, height: w.height };
                    self.paint_theme(&wref);
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
                Some(PendingEvent { hwnd, message: gui::WM_KEYDOWN, wparam: e.detail as usize, lparam: 1 })
            }
            Event::KeyRelease(e) => {
                let hwnd = *self.by_xwin.get(&e.event)?;
                Some(PendingEvent { hwnd, message: gui::WM_KEYUP, wparam: e.detail as usize, lparam: 1 })
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

    fn destroy_window(&mut self, hwnd: usize) {
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

    fn invalidate(&mut self, hwnd: usize) {
        if let Some(w) = self.windows.get(&hwnd) {
            let _ = self.conn.clear_area(false, w.window, 0, 0, 0, 0);
            let _ = self.conn.flush();
        }
    }
}
