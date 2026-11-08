//! Contrôle SysLink (WC_LINK de comctl32 6) : du texte où les passages
//! balisés `<A HREF="…">…</A>` sont des liens. Un clic sur un lien envoie
//! NM_CLICK au parent avec une structure NMLINK (numéro du lien, URL, ID).
//!
//! wxHyperlinkCtrl s'en sert dès que comctl32 annonce la version 6 ; sans
//! lui, CreateWindowEx échouait et la boîte d'accueil de FileZilla
//! s'arrêtait en cours de construction : taille absurde, textes empilés en
//! haut à gauche (#865).
//!
//! Références : documentation publique Win32 « SysLink Control » (messages
//! LM_*, structures LITEM et NMLINK).

use super::*;

pub(super) fn is_link_class(lower: &str) -> bool {
    lower == "syslink"
}

const LM_HITTEST: u32 = 0x0700;
const LM_GETIDEALHEIGHT: u32 = 0x0701; // aussi LM_GETIDEALSIZE
const LM_SETITEM: u32 = 0x0702;
const LM_GETITEM: u32 = 0x0703;
const NM_CLICK: i32 = -2;
const NM_RETURN: i32 = -4;
const WM_PRINTCLIENT: u32 = 0x0318;
const WM_NOTIFY: u32 = 0x004E;
const LINK_COLOR: u32 = 0x00C8_5A00; // bleu des liens (COLORREF BGR)

/// Un morceau de texte, dans un lien (`Some(numéro)`) ou non.
#[derive(Debug, PartialEq)]
struct Piece {
    text: String,
    link: Option<usize>,
}

#[derive(Debug, Default, PartialEq)]
struct Link {
    url: String,
    id: String,
}

/// Valeur d'un attribut `name="…"` (ou sans guillemets) dans une balise.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(k) = lower[from..].find(name) {
        let at = from + k;
        from = at + name.len();
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
        let rest = tag[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let rest = rest[1..].trim_start();
        return Some(if let Some(q) = rest.strip_prefix('"') {
            q.split('"').next().unwrap_or("").to_string()
        } else if let Some(q) = rest.strip_prefix('\'') {
            q.split('\'').next().unwrap_or("").to_string()
        } else {
            rest.split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or("")
                .to_string()
        });
    }
    None
}

/// Découpe le texte balisé en morceaux et en liens.
fn parse(markup: &str) -> (Vec<Piece>, Vec<Link>) {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut links = Vec::new();
    let mut current: Option<usize> = None;
    let mut rest = markup;
    let push = |pieces: &mut Vec<Piece>, s: &str, link: Option<usize>| {
        if s.is_empty() {
            return;
        }
        match pieces.last_mut() {
            Some(p) if p.link == link => p.text.push_str(s),
            _ => pieces.push(Piece {
                text: s.to_string(),
                link,
            }),
        }
    };
    while let Some(lt) = rest.find('<') {
        push(&mut pieces, &rest[..lt], current);
        let after = &rest[lt..];
        let Some(gt) = after.find('>') else {
            push(&mut pieces, after, current);
            rest = "";
            break;
        };
        let tag = &after[1..gt];
        let name = tag.trim().to_ascii_lowercase();
        if current.is_none() && (name == "a" || name.starts_with("a ")) {
            links.push(Link {
                url: attribute(tag, "href").unwrap_or_default(),
                id: attribute(tag, "id").unwrap_or_default(),
            });
            current = Some(links.len() - 1);
        } else if current.is_some() && name == "/a" {
            current = None;
        } else {
            // Balise inconnue : Windows l'affiche telle quelle.
            push(&mut pieces, &after[..=gt], current);
        }
        rest = &after[gt + 1..];
    }
    push(&mut pieces, rest, current);
    (pieces, links)
}

/// Mots placés : (rectangle, texte, lien) pour la largeur `width`.
fn layout(font: usize, markup: &str, width: i32) -> Vec<(Rect, String, Option<usize>)> {
    let (pieces, _) = parse(markup);
    let lh = line_height(font);
    let space = measure(font, " ").0;
    let mut out = Vec::new();
    let (mut x, mut y) = (0, 0);
    for piece in &pieces {
        for (k, line) in piece.text.split('\n').enumerate() {
            if k > 0 {
                x = 0;
                y += lh;
            }
            let mut first = true;
            for word in line.split(' ') {
                if !first {
                    x += space;
                }
                first = false;
                if word.is_empty() {
                    continue;
                }
                let w = measure(font, word).0;
                if x > 0 && x + w > width && width > 0 {
                    x = 0;
                    y += lh;
                }
                out.push((Rect::from_size(x, y, w, lh), word.to_string(), piece.link));
                x += w;
            }
        }
    }
    out
}

/// Taille idéale sur une ligne (ou sous `max_width` si elle est donnée).
fn ideal_size(font: usize, markup: &str, max_width: i32) -> (i32, i32) {
    let words = layout(font, markup, max_width);
    let w = words.iter().map(|(r, ..)| r.right).max().unwrap_or(0);
    let h = words
        .iter()
        .map(|(r, ..)| r.bottom)
        .max()
        .unwrap_or_else(|| line_height(font));
    (w, h)
}

fn font(hwnd: usize) -> usize {
    font_of(&snapshot(hwnd).unwrap_or(Ctl::new(Kind::Static, true)))
}

fn link_at(hwnd: usize, x: i32, y: i32) -> Option<usize> {
    let width = client(hwnd).width();
    layout(font(hwnd), &title(hwnd), width)
        .into_iter()
        .find(|(r, _, l)| l.is_some() && x >= r.left && x < r.right && y >= r.top && y < r.bottom)
        .and_then(|(_, _, l)| l)
}

fn paint_link(hwnd: usize, target: usize) {
    let mut ps: PaintStruct = unsafe { std::mem::zeroed() };
    let hdc = if target != 0 {
        target
    } else {
        unsafe { user32::begin_paint(hwnd, &mut ps) }
    };
    if hdc == 0 {
        return;
    }
    let p = Painter {
        hdc,
        font: font(hwnd),
    };
    let r = client(hwnd);
    let text_color = background(hwnd, hdc, WM_CTLCOLORSTATIC, face_color(), &p, r);
    let on = enabled(hwnd);
    // Fin du mot précédent quand il est dans le même lien et sur la même
    // ligne : le soulignement continue sous l'espace.
    let mut previous: Option<(Option<usize>, i32, i32)> = None;
    for (w, word, link) in layout(p.font, &title(hwnd), r.width()) {
        let color = match (on, link) {
            (false, _) => TEXT_DISABLED,
            (true, Some(_)) => LINK_COLOR,
            (true, None) => text_color,
        };
        p.raw_text(r.left + w.left, r.top + w.top, w.height(), &word, color);
        if link.is_some() {
            let start = match previous {
                Some((l, top, right)) if l == link && top == w.top => right,
                _ => w.left,
            };
            let base = r.top + w.bottom - 2;
            p.line(r.left + start, base, r.left + w.right, base, color, 1);
        }
        previous = Some((link, w.top, w.right));
    }
    if target == 0 {
        unsafe {
            user32::end_paint(hwnd, &ps);
        }
    }
}

/// Envoie NM_CLICK (ou NM_RETURN) au parent pour le lien `index`.
fn notify_link(hwnd: usize, code: i32, index: usize) {
    let (_, links) = parse(&title(hwnd));
    let Some(link) = links.get(index) else {
        return;
    };
    // NMLINK x64 : NMHDR (24 octets) puis LITEM { mask, iLink, state,
    // stateMask, szID[48], szUrl[2084] } (16 + 96 + 4168 octets).
    let mut buf = vec![0u8; 24 + 16 + 96 + 4168];
    let id = user32::control_id(hwnd) as usize;
    buf[..8].copy_from_slice(&hwnd.to_le_bytes());
    buf[8..16].copy_from_slice(&id.to_le_bytes());
    buf[16..20].copy_from_slice(&code.to_le_bytes());
    buf[24..28].copy_from_slice(&0x7u32.to_le_bytes()); // LIF_ITEMINDEX|STATE|ITEMID
    buf[28..32].copy_from_slice(&(index as i32).to_le_bytes());
    let put = |buf: &mut [u8], at: usize, max: usize, s: &str| {
        for (k, ch) in s.encode_utf16().take(max - 1).enumerate() {
            buf[at + 2 * k..at + 2 * k + 2].copy_from_slice(&ch.to_le_bytes());
        }
    };
    put(&mut buf, 40, 48, &link.id);
    put(&mut buf, 136, 2084, &link.url);
    send(parent(hwnd), WM_NOTIFY, id, buf.as_mut_ptr() as isize);
}

pub(super) unsafe extern "win64" fn syslink_proc(
    hwnd: usize,
    msg: u32,
    wp: usize,
    lp: isize,
) -> isize {
    match msg {
        WM_PAINT => {
            paint_link(hwnd, wp);
            0
        }
        WM_PRINTCLIENT => {
            paint_link(hwnd, wp);
            0
        }
        LM_GETIDEALHEIGHT => {
            // LM_GETIDEALSIZE : wParam = largeur maximale, lParam = SIZE*.
            let (w, h) = ideal_size(font(hwnd), &title(hwnd), wp as i32);
            if lp != 0 {
                std::ptr::write_unaligned(lp as *mut [i32; 2], [w, h]);
            }
            h as isize
        }
        LM_HITTEST | LM_SETITEM | LM_GETITEM => 0,
        WM_LBUTTONDOWN => 0,
        WM_LBUTTONUP => {
            let (x, y) = point(lp);
            if let Some(i) = link_at(hwnd, x, y) {
                notify_link(hwnd, NM_CLICK, i);
            }
            0
        }
        WM_KEYDOWN if wp == 0x0D => {
            if !parse(&title(hwnd)).1.is_empty() {
                notify_link(hwnd, NM_RETURN, 0);
            }
            0
        }
        WM_GETDLGCODE => 0,
        _ => {
            let r = control_proc_impl(Kind::Static, hwnd, msg, wp, lp);
            if msg == WM_SETTEXT {
                invalidate(hwnd);
            }
            r
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balisage_des_liens() {
        let (pieces, links) =
            parse(r#"Voir <A HREF="https://exemple.org/a" ID="x">la page</A> et <a href=b>b</a>."#);
        assert_eq!(
            pieces,
            vec![
                Piece {
                    text: "Voir ".into(),
                    link: None
                },
                Piece {
                    text: "la page".into(),
                    link: Some(0)
                },
                Piece {
                    text: " et ".into(),
                    link: None
                },
                Piece {
                    text: "b".into(),
                    link: Some(1)
                },
                Piece {
                    text: ".".into(),
                    link: None
                },
            ]
        );
        assert_eq!(
            links,
            vec![
                Link {
                    url: "https://exemple.org/a".into(),
                    id: "x".into()
                },
                Link {
                    url: "b".into(),
                    id: String::new()
                },
            ]
        );
    }
}
