//! Fenêtres restées sur un écran débranché (#1412) : quand l'ensemble des
//! écrans change, toute fenêtre qui n'est plus entièrement visible est
//! ramenée sur l'écran qui en montre le plus (sinon l'écran principal), en
//! restant dans la zone de travail (hors barre des tâches). Les fenêtres
//! agrandies, plein écran ou réduites sont laissées au gestionnaire de
//! fenêtres, qui les replace lui-même.
//!
//! Le calcul est pur et testé ; le déplacement passe par la requête EWMH
//! `_NET_MOVERESIZE_WINDOW`, que le gestionnaire de fenêtres applique.

/// Rectangle (x, y, largeur, hauteur) dans l'écran X.
pub type Rect = (i32, i32, i32, i32);

fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.0.max(b.0);
    let y = a.1.max(b.1);
    let r = (a.0 + a.2).min(b.0 + b.2);
    let bottom = (a.1 + a.3).min(b.1 + b.3);
    (r > x && bottom > y).then_some((x, y, r - x, bottom - y))
}

fn area(r: Option<Rect>) -> i64 {
    r.map_or(0, |(_, _, w, h)| w as i64 * h as i64)
}

/// Surface du cadre visible sur l'ensemble des écrans (les écrans dupliqués
/// ne comptent qu'une fois).
fn visible(frame: Rect, monitors: &[Rect]) -> i64 {
    let mut seen: Vec<Rect> = Vec::new();
    let mut total = 0;
    for &m in monitors {
        if !seen.contains(&m) {
            seen.push(m);
            total += area(intersection(frame, m));
        }
    }
    total
}

/// Où ramener un cadre, ou `None` s'il est entièrement visible.
/// `monitors` : zones des écrans allumés ; `primary` : indice de l'écran
/// principal ; `workarea` : zone de travail globale (hors barre des
/// tâches), si le gestionnaire de fenêtres la publie.
pub fn rescue_frame(
    frame: Rect,
    monitors: &[Rect],
    primary: usize,
    workarea: Option<Rect>,
) -> Option<Rect> {
    if monitors.is_empty() || frame.2 <= 0 || frame.3 <= 0 {
        return None;
    }
    if visible(frame, monitors) >= area(Some(frame)) {
        return None;
    }
    let best = monitors
        .iter()
        .copied()
        .max_by_key(|&m| area(intersection(frame, m)))
        .filter(|&m| area(intersection(frame, m)) > 0)
        .unwrap_or(monitors[primary.min(monitors.len() - 1)]);
    let zone = workarea.and_then(|w| intersection(best, w)).unwrap_or(best);
    let w = frame.2.min(zone.2);
    let h = frame.3.min(zone.3);
    let x = frame.0.clamp(zone.0, zone.0 + zone.2 - w);
    let y = frame.1.clamp(zone.1, zone.1 + zone.3 - h);
    let target = (x, y, w, h);
    (target != frame).then_some(target)
}

/// Zones des écrans allumés et indice du principal.
pub fn monitors(outputs: &[super::Output]) -> (Vec<Rect>, usize) {
    let mut rects = Vec::new();
    let mut primary = 0;
    for o in outputs {
        if let Some((w, h, x, y)) = o.geometry {
            if o.primary {
                primary = rects.len();
            }
            rects.push((x, y, w as i32, h as i32));
        }
    }
    (rects, primary)
}

/// Ramène les fenêtres de la session sur les écrans restants ; renvoie le
/// nombre de fenêtres déplacées.
pub fn bring_back(monitors: &[Rect], primary: usize) -> Result<usize, String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, Window};
    let (conn, screen) = x11rb::connect(None).map_err(|e| format!("Affichage Xorg : {e}"))?;
    let root = conn.setup().roots[screen].root;
    let atom = |name: &str| -> Result<u32, String> {
        Ok(conn
            .intern_atom(false, name.as_bytes())
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?
            .atom)
    };
    let cardinals = |w: Window, prop: u32, kind: u32| -> Vec<u32> {
        conn.get_property(false, w, prop, kind, 0, 1024)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    };
    let client_list = atom("_NET_CLIENT_LIST")?;
    let wm_state = atom("_NET_WM_STATE")?;
    let wm_type = atom("_NET_WM_WINDOW_TYPE")?;
    let extents = atom("_NET_FRAME_EXTENTS")?;
    let workarea_atom = atom("_NET_WORKAREA")?;
    let moveresize = atom("_NET_MOVERESIZE_WINDOW")?;
    let mut skip_states = Vec::new();
    for n in [
        "_NET_WM_STATE_MAXIMIZED_VERT",
        "_NET_WM_STATE_MAXIMIZED_HORZ",
        "_NET_WM_STATE_FULLSCREEN",
        "_NET_WM_STATE_HIDDEN",
    ] {
        skip_states.push(atom(n)?);
    }
    let mut kept_types = Vec::new();
    for n in [
        "_NET_WM_WINDOW_TYPE_NORMAL",
        "_NET_WM_WINDOW_TYPE_DIALOG",
        "_NET_WM_WINDOW_TYPE_UTILITY",
    ] {
        kept_types.push(atom(n)?);
    }
    let wa = cardinals(root, workarea_atom, AtomEnum::CARDINAL.into());
    let workarea =
        (wa.len() >= 4).then(|| (wa[0] as i32, wa[1] as i32, wa[2] as i32, wa[3] as i32));
    let mut moved = 0;
    for w in cardinals(root, client_list, AtomEnum::WINDOW.into()) {
        let states = cardinals(w, wm_state, AtomEnum::ATOM.into());
        if states.iter().any(|s| skip_states.contains(s)) {
            continue;
        }
        let types = cardinals(w, wm_type, AtomEnum::ATOM.into());
        if !types.is_empty() && !types.iter().any(|t| kept_types.contains(t)) {
            continue;
        }
        let Some(g) = conn.get_geometry(w).ok().and_then(|c| c.reply().ok()) else {
            continue;
        };
        let Some(t) = conn
            .translate_coordinates(w, root, 0, 0)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            continue;
        };
        let e = cardinals(w, extents, AtomEnum::CARDINAL.into());
        let (l, r, top, b) = if e.len() >= 4 {
            (e[0] as i32, e[1] as i32, e[2] as i32, e[3] as i32)
        } else {
            (0, 0, 0, 0)
        };
        let frame = (
            t.dst_x as i32 - l,
            t.dst_y as i32 - top,
            g.width as i32 + l + r,
            g.height as i32 + top + b,
        );
        let Some((x, y, fw, fh)) = rescue_frame(frame, monitors, primary, workarea) else {
            continue;
        };
        // Gravité NorthWest (1) : x, y = coin du cadre ; largeur et hauteur
        // = zone cliente. Source 2 = outil du bureau (EWMH).
        let mut flags = 1 | (1 << 8) | (1 << 9) | (2 << 12);
        if fw != frame.2 || fh != frame.3 {
            flags |= (1 << 10) | (1 << 11);
        }
        let data = [
            flags,
            x as u32,
            y as u32,
            (fw - l - r).max(1) as u32,
            (fh - top - b).max(1) as u32,
        ];
        let ev = ClientMessageEvent::new(32, w, moveresize, data);
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        if conn.send_event(false, root, mask, ev).is_ok() {
            moved += 1;
        }
    }
    conn.flush().map_err(|e| e.to_string())?;
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Rect = (0, 0, 1366, 768);
    const RIGHT: Rect = (1366, 0, 1366, 768);

    #[test]
    fn fenetre_visible_laissee_en_place() {
        assert_eq!(rescue_frame((100, 100, 800, 600), &[LEFT], 0, None), None);
        // À cheval sur deux écrans branchés : entièrement visible.
        assert_eq!(
            rescue_frame((1000, 100, 800, 600), &[LEFT, RIGHT], 0, None),
            None
        );
    }

    #[test]
    fn fenetre_a_moitie_hors_de_l_ecran_restant() {
        // Cas du banc : Paramètres en 1804,178, second écran débranché.
        let r = rescue_frame((1804, 178, 900, 500), &[LEFT], 0, None).unwrap();
        assert_eq!(r, (466, 178, 900, 500));
        // Partiellement sur l'écran restant : ramenée sur lui.
        let r = rescue_frame((1000, 100, 800, 600), &[LEFT], 0, None).unwrap();
        assert_eq!(r, (566, 100, 800, 600));
    }

    #[test]
    fn fenetre_entierement_perdue_va_sur_le_principal() {
        let r = rescue_frame((3000, 900, 400, 300), &[LEFT, RIGHT], 1, None).unwrap();
        assert_eq!(r, (2332, 468, 400, 300));
    }

    #[test]
    fn trop_grande_reduite_et_hors_barre_des_taches() {
        let work = Some((0, 0, 2732, 720));
        let r = rescue_frame((1500, 50, 1600, 900), &[LEFT], 0, work).unwrap();
        assert_eq!(r, (0, 0, 1366, 720));
    }

    #[test]
    fn ecrans_dupliques_comptes_une_fois() {
        assert_eq!(
            rescue_frame((100, 100, 800, 600), &[LEFT, LEFT], 0, None),
            None
        );
        assert!(rescue_frame((1000, 100, 800, 600), &[LEFT, LEFT], 0, None).is_some());
    }
}
