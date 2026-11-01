//! Réception d'un glisser-déposer X11 (protocole XDND, version 5) sur les
//! fenêtres des programmes Windows : fichiers depuis Thunar ou le bureau,
//! texte ou image depuis une appli Linux.
//!
//! Le backend X11 transmet ici les ClientMessage `Xdnd*` et le
//! SelectionNotify de `XdndSelection`. Les données reçues sont converties en
//! formats Windows (`clipbridge::import`) puis confiées à
//! `dataobject::queue_drop`, qui les livre à l'appli (IDropTarget ou
//! WM_DROPFILES) sur le thread de sa fenêtre.

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, PropMode,
    SelectionNotifyEvent, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::clipbridge::{self, LinuxClip};
use crate::dlls::gui::PendingEvent;

const VERSION: u32 = 5;

struct Session {
    types: Vec<String>,
    accepted: bool,
    root_pos: (i32, i32),
}

struct Waiting {
    hwnd: usize,
    xwin: Window,
    source: Window,
    target: String,
    root_pos: (i32, i32),
}

pub struct Xdnd {
    aware: Atom,
    enter: Atom,
    position: Atom,
    status: Atom,
    leave: Atom,
    drop: Atom,
    finished: Atom,
    selection: Atom,
    action_copy: Atom,
    type_list: Atom,
    prop: Atom,
    session: Option<Session>,
    waiting: Option<Waiting>,
}

/// Cible à demander selon les types offerts : fichiers, image, puis texte.
pub fn choose_target(types: &[String]) -> Option<String> {
    let plan = clipbridge::import_plan(types);
    plan.into_iter().find(|t| !t.starts_with("text/html"))
}

impl Xdnd {
    pub fn new(conn: &RustConnection) -> Option<Self> {
        let atom = |n: &[u8]| -> Option<Atom> {
            Some(conn.intern_atom(false, n).ok()?.reply().ok()?.atom)
        };
        Some(Xdnd {
            aware: atom(b"XdndAware")?,
            enter: atom(b"XdndEnter")?,
            position: atom(b"XdndPosition")?,
            status: atom(b"XdndStatus")?,
            leave: atom(b"XdndLeave")?,
            drop: atom(b"XdndDrop")?,
            finished: atom(b"XdndFinished")?,
            selection: atom(b"XdndSelection")?,
            action_copy: atom(b"XdndActionCopy")?,
            type_list: atom(b"XdndTypeList")?,
            prop: atom(b"WINLUX_XDND_DATA")?,
            session: None,
            waiting: None,
        })
    }

    /// Annonce que la fenêtre accepte les dépôts XDND.
    pub fn set_aware(&self, conn: &RustConnection, win: Window) {
        let _ = conn.change_property32(
            PropMode::REPLACE,
            win,
            self.aware,
            AtomEnum::ATOM,
            &[VERSION],
        );
    }

    fn atom_name(conn: &RustConnection, a: Atom) -> String {
        conn.get_atom_name(a)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| String::from_utf8_lossy(&r.name).into_owned())
            .unwrap_or_default()
    }

    fn send(&self, conn: &RustConnection, to: Window, kind: Atom, data: [u32; 5]) {
        let ev = ClientMessageEvent::new(32, to, kind, data);
        let _ = conn.send_event(false, to, EventMask::NO_EVENT, ev);
        let _ = conn.flush();
    }

    /// Traite un ClientMessage. `None` : ce n'est pas un message XDND.
    /// `Some(ev)` : message XDND consommé, avec éventuellement un événement Win32.
    pub fn client_message(
        &mut self,
        conn: &RustConnection,
        e: &ClientMessageEvent,
        hwnd: usize,
    ) -> Option<Option<PendingEvent>> {
        let d = e.data.as_data32();
        let t = e.type_;
        if t == self.enter {
            let source = d[0];
            let mut atoms: Vec<Atom> = d[2..5].iter().copied().filter(|a| *a != 0).collect();
            if d[1] & 1 != 0 {
                if let Some(r) = conn
                    .get_property(false, source, self.type_list, AtomEnum::ATOM, 0, 256)
                    .ok()
                    .and_then(|c| c.reply().ok())
                {
                    if let Some(list) = r.value32() {
                        atoms = list.collect();
                    }
                }
            }
            let types = atoms
                .into_iter()
                .map(|a| Self::atom_name(conn, a))
                .collect();
            self.session = Some(Session {
                types,
                accepted: false,
                root_pos: (0, 0),
            });
            return Some(None);
        }
        if t == self.position {
            let source = d[0];
            let root_pos = ((d[2] >> 16) as i16 as i32, (d[2] & 0xFFFF) as i16 as i32);
            let ok = self
                .session
                .as_ref()
                .is_some_and(|s| choose_target(&s.types).is_some())
                && crate::dlls::dataobject::accepts(hwnd);
            if let Some(s) = self.session.as_mut() {
                s.accepted = ok;
                s.root_pos = root_pos;
            }
            let action = if ok { self.action_copy } else { 0 };
            self.send(
                conn,
                source,
                self.status,
                [e.window, u32::from(ok) | 2, 0, 0, action],
            );
            return Some(None);
        }
        if t == self.leave {
            self.session = None;
            return Some(None);
        }
        if t == self.drop {
            let source = d[0];
            let time = d[2];
            let Some(s) = self.session.take() else {
                self.send(conn, source, self.finished, [e.window, 0, 0, 0, 0]);
                return Some(None);
            };
            match choose_target(&s.types).filter(|_| s.accepted) {
                Some(target) => {
                    let target_atom = conn
                        .intern_atom(false, target.as_bytes())
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|r| r.atom)
                        .unwrap_or(0);
                    let _ = conn.delete_property(e.window, self.prop);
                    let _ = conn.convert_selection(
                        e.window,
                        self.selection,
                        target_atom,
                        self.prop,
                        time,
                    );
                    let _ = conn.flush();
                    self.waiting = Some(Waiting {
                        hwnd,
                        xwin: e.window,
                        source,
                        target,
                        root_pos: s.root_pos,
                    });
                }
                None => self.send(conn, source, self.finished, [e.window, 0, 0, 0, 0]),
            }
            return Some(None);
        }
        None
    }

    /// Données du dépôt reçues : prévient la source et produit le message
    /// privé à livrer à la fenêtre. `scale` : facteur DPI du backend.
    pub fn selection_notify(
        &mut self,
        conn: &RustConnection,
        e: &SelectionNotifyEvent,
        scale: f32,
        root: Window,
    ) -> Option<Option<PendingEvent>> {
        if e.selection != self.selection {
            return None;
        }
        let w = self.waiting.take()?;
        let mut clip = LinuxClip::default();
        if e.property != x11rb::NONE {
            if let Some(r) = conn
                .get_property(true, w.xwin, self.prop, AtomEnum::ANY, 0, u32::MAX / 4)
                .ok()
                .and_then(|c| c.reply().ok())
            {
                clipbridge::absorb(&mut clip, &w.target, &r.value);
            }
        }
        let ok = !clip.is_empty();
        let action = if ok { self.action_copy } else { 0 };
        self.send(
            conn,
            w.source,
            self.finished,
            [w.xwin, u32::from(ok), action, 0, 0],
        );
        if !ok {
            return Some(None);
        }
        let (contents, _) = clipbridge::import(&clip, crate::paths::to_windows);
        let client = conn
            .translate_coordinates(root, w.xwin, w.root_pos.0 as i16, w.root_pos.1 as i16)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| (r.dst_x as i32, r.dst_y as i32))
            .unwrap_or((0, 0));
        let logical = |p: (i32, i32)| ((p.0 as f32 / scale) as i32, (p.1 as f32 / scale) as i32);
        let (hwnd, message, wparam) = crate::dlls::dataobject::queue_drop(
            w.hwnd,
            contents,
            logical(w.root_pos),
            logical(client),
        );
        Some(Some(PendingEvent {
            hwnd,
            message,
            wparam,
            lparam: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cible_de_depot_selon_les_types() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            choose_target(&s(&[
                "text/uri-list",
                "text/plain",
                "x-special/gnome-icon-list"
            ]))
            .as_deref(),
            Some("text/uri-list")
        );
        assert_eq!(
            choose_target(&s(&["text/html", "UTF8_STRING"])).as_deref(),
            Some("UTF8_STRING")
        );
        assert_eq!(
            choose_target(&s(&["image/png"])).as_deref(),
            Some("image/png")
        );
        assert_eq!(choose_target(&s(&["application/x-foo"])), None);
    }
}

// ---------------------------------------------------------------------------
// Source : DoDragDrop d'un programme Windows vers une appli Linux
// ---------------------------------------------------------------------------

const DRAGDROP_S_DROP: i32 = 0x0004_0100;
const DRAGDROP_S_CANCEL: i32 = 0x0004_0101;
const DRAGDROP_S_USEDEFAULTCURSORS: i32 = 0x0004_0102;
const E_UNEXPECTED: i32 = 0x8000_FFFFu32 as i32;

/// Cible survolée : fenêtre XDND d'une autre appli, ou fenêtre de ce processus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Over {
    None,
    Remote { win: Window, version: u32 },
    Local { top: usize, xwin: Window },
}

struct Source {
    conn: RustConnection,
    root: Window,
    win: Window,
    a: Xdnd,
    actions: [Atom; 3],
    type_atoms: Vec<Atom>,
    targets: Vec<&'static str>,
    contents: crate::dlls::clipboard::Contents,
}

impl Source {
    fn atom(&self, name: &str) -> Atom {
        self.conn
            .intern_atom(false, name.as_bytes())
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom)
            .unwrap_or(0)
    }

    /// Fenêtre XdndAware sous le pointeur (en descendant depuis la racine).
    fn find_target(&self) -> (Over, (i32, i32), u16) {
        let Some(p) = self
            .conn
            .query_pointer(self.root)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return (Over::None, (0, 0), 0);
        };
        let pos = (p.root_x as i32, p.root_y as i32);
        let mask = u16::from(p.mask);
        let mut w = p.child;
        for _ in 0..8 {
            if w == x11rb::NONE || w == self.win {
                break;
            }
            if let Some(top) = crate::dlls::dataobject::local_top_for(w as u64) {
                return (Over::Local { top, xwin: w }, pos, mask);
            }
            let aware = self
                .conn
                .get_property(false, w, self.a.aware, AtomEnum::ATOM, 0, 1)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().and_then(|mut v| v.next()));
            if let Some(v) = aware {
                return (
                    Over::Remote {
                        win: w,
                        version: v.min(VERSION),
                    },
                    pos,
                    mask,
                );
            }
            w = self
                .conn
                .query_pointer(w)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| r.child)
                .unwrap_or(x11rb::NONE);
        }
        (Over::None, pos, mask)
    }

    fn send(&self, to: Window, kind: Atom, data: [u32; 5]) {
        let ev = ClientMessageEvent::new(32, to, kind, data);
        let _ = self.conn.send_event(false, to, EventMask::NO_EVENT, ev);
        let _ = self.conn.flush();
    }

    fn enter(&self, to: Window, version: u32) {
        let mut data = [self.win, version << 24, 0, 0, 0];
        if self.type_atoms.len() > 3 {
            data[1] |= 1;
        }
        for (i, a) in self.type_atoms.iter().take(3).enumerate() {
            data[2 + i] = *a;
        }
        self.send(to, self.a.enter, data);
    }

    fn action_for(&self, allowed: u32) -> Atom {
        if allowed & 1 != 0 {
            self.actions[0]
        } else if allowed & 2 != 0 {
            self.actions[1]
        } else {
            self.actions[2]
        }
    }

    fn effect_of(&self, action: Atom) -> u32 {
        match self.actions.iter().position(|a| *a == action) {
            Some(0) => 1,
            Some(1) => 2,
            Some(2) => 4,
            _ => 1,
        }
    }

    /// Répond à une demande de données de la cible.
    fn serve(&self, e: &x11rb::protocol::xproto::SelectionRequestEvent) {
        let name = Xdnd::atom_name(&self.conn, e.target);
        let property = if e.property == x11rb::NONE {
            e.target
        } else {
            e.property
        };
        let ok = if name == "TARGETS" {
            let mut list = vec![self.atom("TARGETS")];
            list.extend(&self.type_atoms);
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    e.requestor,
                    property,
                    AtomEnum::ATOM,
                    &list,
                )
                .is_ok()
        } else {
            let max = self.conn.maximum_request_bytes().saturating_sub(1024);
            match crate::clipbridge::export_render(&self.contents, &name, crate::paths::to_linux) {
                Some(data) if data.len() <= max => self
                    .conn
                    .change_property8(PropMode::REPLACE, e.requestor, property, e.target, &data)
                    .is_ok(),
                _ => false,
            }
        };
        let ev = SelectionNotifyEvent {
            response_type: x11rb::protocol::xproto::SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: e.time,
            requestor: e.requestor,
            selection: e.selection,
            target: e.target,
            property: if ok { property } else { x11rb::NONE },
        };
        let _ = self
            .conn
            .send_event(false, e.requestor, EventMask::NO_EVENT, ev);
        let _ = self.conn.flush();
    }
}

unsafe fn com_call_query_continue(src: usize, escape: i32, keys: u32) -> i32 {
    let vtbl = *(src as *const *const usize);
    let f: extern "win64" fn(usize, i32, u32) -> i32 = std::mem::transmute(*vtbl.add(3));
    f(src, escape, keys)
}

unsafe fn com_call_feedback(src: usize, effect: u32) -> i32 {
    let vtbl = *(src as *const *const usize);
    let f: extern "win64" fn(usize, u32) -> i32 = std::mem::transmute(*vtbl.add(4));
    f(src, effect)
}

fn keys_from_mask(mask: u16) -> u32 {
    let mut k = 0;
    if mask & 0x100 != 0 {
        k |= 1; // MK_LBUTTON
    }
    if mask & 0x400 != 0 {
        k |= 2; // MK_RBUTTON
    }
    if mask & 0x1 != 0 {
        k |= 4; // MK_SHIFT
    }
    if mask & 0x4 != 0 {
        k |= 8; // MK_CONTROL
    }
    k
}

/// `DoDragDrop` : boucle modale qui suit le pointeur et parle XDND aux
/// fenêtres Linux survolées, ou livre directement aux fenêtres de ce processus.
pub unsafe extern "win64" fn do_drag_drop(
    data: usize,
    source: usize,
    allowed: u32,
    effect: *mut u32,
) -> i32 {
    if data == 0 || source == 0 {
        return 0x8000_4003u32 as i32; // E_POINTER
    }
    let set_effect = |e: u32| {
        if !effect.is_null() {
            *effect = e;
        }
    };
    let formats = crate::dlls::dataobject::read_data_object(data);
    let contents = crate::dlls::clipboard::Contents { seq: 0, formats };
    let Ok((conn, screen)) = x11rb::connect(None) else {
        set_effect(0);
        return E_UNEXPECTED;
    };
    let root = conn.setup().roots[screen].root;
    let Some(a) = Xdnd::new(&conn) else {
        return E_UNEXPECTED;
    };
    let Ok(win) = conn.generate_id() else {
        return E_UNEXPECTED;
    };
    let _ = conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        root,
        -10,
        -10,
        1,
        1,
        0,
        x11rb::protocol::xproto::WindowClass::INPUT_ONLY,
        0,
        &x11rb::protocol::xproto::CreateWindowAux::new(),
    );
    let targets = crate::clipbridge::export_targets(&contents);
    let mut src = Source {
        actions: [0; 3],
        type_atoms: Vec::new(),
        targets,
        contents,
        conn,
        root,
        win,
        a,
    };
    src.actions = [
        src.atom("XdndActionCopy"),
        src.atom("XdndActionMove"),
        src.atom("XdndActionLink"),
    ];
    src.type_atoms = src.targets.iter().map(|t| src.atom(t)).collect();
    let _ = src.conn.change_property32(
        PropMode::REPLACE,
        win,
        src.a.type_list,
        AtomEnum::ATOM,
        &src.type_atoms,
    );
    let _ = src
        .conn
        .set_selection_owner(win, src.a.selection, x11rb::CURRENT_TIME);
    let _ = src.conn.flush();

    let mut over = Over::None;
    let mut accepted = false;
    let mut accepted_action: Atom = 0;
    let mut last_pos = (i32::MIN, i32::MIN);
    let mut waiting_status = false;
    let started = std::time::Instant::now();
    let result;
    loop {
        let (now, pos, mask) = src.find_target();
        let keys = keys_from_mask(mask);
        let escape = i32::from(crate::dlls::gui::get_async_key_state(0x1B) < 0);
        let mut verdict = com_call_query_continue(source, escape, keys);
        if mask & 0x100 == 0 && verdict == 0 && started.elapsed().as_millis() > 50 {
            verdict = DRAGDROP_S_DROP;
        }
        if now != over {
            if let Over::Remote { win: old, .. } = over {
                src.send(old, src.a.leave, [win, 0, 0, 0, 0]);
            }
            if let Over::Remote { win: new, version } = now {
                src.enter(new, version);
            }
            over = now;
            accepted =
                matches!(over, Over::Local { top, .. } if crate::dlls::dataobject::accepts(top));
            accepted_action = src.action_for(allowed);
            last_pos = (i32::MIN, i32::MIN);
            waiting_status = false;
        }
        if let Over::Remote { win: to, .. } = over {
            if pos != last_pos && !waiting_status {
                let packed = ((pos.0 as u32 & 0xFFFF) << 16) | (pos.1 as u32 & 0xFFFF);
                src.send(
                    to,
                    src.a.position,
                    [win, 0, packed, x11rb::CURRENT_TIME, src.action_for(allowed)],
                );
                last_pos = pos;
                waiting_status = true;
            }
        }
        // Réponses des cibles et demandes de données.
        while let Ok(Some(ev)) = src.conn.poll_for_event() {
            match ev {
                x11rb::protocol::Event::ClientMessage(e) if e.type_ == src.a.status => {
                    let d = e.data.as_data32();
                    accepted = d[1] & 1 != 0;
                    accepted_action = if d[4] != 0 {
                        d[4]
                    } else {
                        src.action_for(allowed)
                    };
                    waiting_status = false;
                }
                x11rb::protocol::Event::SelectionRequest(e) => src.serve(&e),
                _ => {}
            }
        }
        let feedback = if accepted {
            src.effect_of(accepted_action) & allowed
        } else {
            0
        };
        let _ = com_call_feedback(source, feedback);
        if verdict == DRAGDROP_S_CANCEL {
            if let Over::Remote { win: old, .. } = over {
                src.send(old, src.a.leave, [win, 0, 0, 0, 0]);
            }
            result = (DRAGDROP_S_CANCEL, 0);
            break;
        }
        if verdict == DRAGDROP_S_DROP {
            result = match over {
                Over::Remote { win: to, .. } if accepted => {
                    src.send(to, src.a.drop, [win, 0, x11rb::CURRENT_TIME, 0, 0]);
                    // On sert les données jusqu'à XdndFinished (3 s au plus).
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                    let mut done = None;
                    while std::time::Instant::now() < deadline && done.is_none() {
                        match src.conn.poll_for_event() {
                            Ok(Some(x11rb::protocol::Event::SelectionRequest(e))) => src.serve(&e),
                            Ok(Some(x11rb::protocol::Event::ClientMessage(e)))
                                if e.type_ == src.a.finished =>
                            {
                                let d = e.data.as_data32();
                                let ok = d[1] & 1 != 0;
                                let action = if d[2] != 0 { d[2] } else { accepted_action };
                                done = Some(if ok {
                                    src.effect_of(action) & allowed
                                } else {
                                    0
                                });
                            }
                            Ok(Some(_)) => {}
                            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                            Err(_) => break,
                        }
                    }
                    let e = done.unwrap_or(src.effect_of(accepted_action) & allowed);
                    if e != 0 {
                        (DRAGDROP_S_DROP, e)
                    } else {
                        (DRAGDROP_S_CANCEL, 0)
                    }
                }
                Over::Local { top, xwin } => {
                    let client = src
                        .conn
                        .translate_coordinates(root, xwin, pos.0 as i16, pos.1 as i16)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|r| (r.dst_x as i32, r.dst_y as i32))
                        .unwrap_or((0, 0));
                    let e = crate::dlls::dataobject::deliver(
                        top,
                        data,
                        &src.contents,
                        pos,
                        client,
                        allowed,
                    );
                    if e != 0 {
                        (DRAGDROP_S_DROP, e)
                    } else {
                        (DRAGDROP_S_CANCEL, 0)
                    }
                }
                Over::Remote { win: to, .. } => {
                    src.send(to, src.a.leave, [win, 0, 0, 0, 0]);
                    (DRAGDROP_S_CANCEL, 0)
                }
                Over::None => (DRAGDROP_S_CANCEL, 0),
            };
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    let _ = DRAGDROP_S_USEDEFAULTCURSORS;
    let _ = src.conn.destroy_window(win);
    let _ = src.conn.flush();
    set_effect(result.1);
    result.0
}
