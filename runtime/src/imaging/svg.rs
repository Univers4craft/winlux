//! SVG maison : analyseur XML, styles (attributs, `style`, feuilles CSS
//! simples), formes, chemins, `use`, dégradés, images intégrées.
//! Le document est conservé sous forme vectorielle et re-rastérisé à
//! chaque niveau de zoom (zoom sans perte).

use super::raster::{self, Canvas, Mat, Paint, Pt, Stop};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub kids: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    }
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(j) = rest.find(';') else {
            break;
        };
        let ent = &rest[1..j];
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16)
                .ok()
                .and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[j + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Analyse XML tolérante (renvoie la racine).
pub fn parse_xml(s: &str) -> Option<Node> {
    let b = s.as_bytes();
    let mut stack: Vec<Node> = vec![Node::default()];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'<' {
            if s[i..].starts_with("<!--") {
                i = s[i..].find("-->").map(|k| i + k + 3)?;
                continue;
            }
            if s[i..].starts_with("<![CDATA[") {
                let end = s[i..].find("]]>").map(|k| i + k)?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&s[i + 9..end]);
                }
                i = end + 3;
                continue;
            }
            if s[i..].starts_with("<?") {
                i = s[i..].find("?>").map(|k| i + k + 2)?;
                continue;
            }
            if s[i..].starts_with("<!") {
                // DOCTYPE éventuellement avec sous-ensemble interne
                let mut depth = 0;
                while i < b.len() {
                    match b[i] {
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        b'>' if depth <= 0 => break,
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }
            if b.get(i + 1) == Some(&b'/') {
                let end = s[i..].find('>').map(|k| i + k)?;
                i = end + 1;
                if stack.len() > 1 {
                    let n = stack.pop()?;
                    stack.last_mut()?.kids.push(n);
                }
                continue;
            }
            // balise ouvrante
            let mut j = i + 1;
            while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let mut node = Node {
                name: strip_ns(&s[i + 1..j]).to_string(),
                ..Default::default()
            };
            let mut k = j;
            let mut self_close = false;
            loop {
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k >= b.len() {
                    return None;
                }
                if b[k] == b'>' {
                    k += 1;
                    break;
                }
                if b[k] == b'/' {
                    self_close = true;
                    k += 1;
                    continue;
                }
                let ns = k;
                while k < b.len() && b[k] != b'=' && !b[k].is_ascii_whitespace() && b[k] != b'>' {
                    k += 1;
                }
                let name = s[ns..k].to_string();
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if b.get(k) == Some(&b'=') {
                    k += 1;
                    while k < b.len() && b[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    let q = *b.get(k)?;
                    if q == b'"' || q == b'\'' {
                        let vs = k + 1;
                        let ve = vs + s[vs..].find(q as char)?;
                        node.attrs.push((name, unescape(&s[vs..ve])));
                        k = ve + 1;
                    } else {
                        let vs = k;
                        while k < b.len() && !b[k].is_ascii_whitespace() && b[k] != b'>' {
                            k += 1;
                        }
                        node.attrs.push((name, unescape(&s[vs..k])));
                    }
                } else {
                    node.attrs.push((name, String::new()));
                }
            }
            i = k;
            if self_close {
                stack.last_mut()?.kids.push(node);
            } else {
                stack.push(node);
            }
        } else {
            let end = s[i..].find('<').map(|k| i + k).unwrap_or(b.len());
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&unescape(&s[i..end]));
            }
            i = end;
        }
    }
    while stack.len() > 1 {
        let n = stack.pop()?;
        stack.last_mut()?.kids.push(n);
    }
    let root = stack.pop()?;
    root.kids.into_iter().find(|n| !n.name.is_empty())
}

fn strip_ns(n: &str) -> &str {
    n.strip_prefix("svg:").unwrap_or(n)
}

pub fn parse_color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        let v = |i: usize, n: usize| u8::from_str_radix(h.get(i..i + n)?, 16).ok();
        return match h.len() {
            3 | 4 => {
                let r = v(0, 1)? * 17;
                let g = v(1, 1)? * 17;
                let b = v(2, 1)? * 17;
                let a = if h.len() == 4 { v(3, 1)? * 17 } else { 255 };
                Some([r, g, b, a])
            }
            6 | 8 => Some([
                v(0, 2)?,
                v(2, 2)?,
                v(4, 2)?,
                if h.len() == 8 { v(6, 2)? } else { 255 },
            ]),
            12 => Some([v(0, 2)?, v(4, 2)?, v(8, 2)?, 255]),
            _ => None,
        };
    }
    let l = s.to_ascii_lowercase();
    if l.starts_with("rgb") || l.starts_with("hsl") {
        let inner = l.split_once('(')?.1.trim_end_matches(')');
        let parts: Vec<&str> = inner
            .split([',', ' ', '/'])
            .filter(|p| !p.is_empty())
            .collect();
        let num = |p: &str, scale: f32| -> f32 {
            if let Some(v) = p.strip_suffix('%') {
                v.parse::<f32>().unwrap_or(0.0) * scale / 100.0
            } else {
                p.parse::<f32>().unwrap_or(0.0)
            }
        };
        let a = parts.get(3).map(|p| num(p, 1.0)).unwrap_or(1.0);
        if l.starts_with("hsl") {
            let h = num(parts.first()?, 360.0).rem_euclid(360.0) / 360.0;
            let sat = num(parts.get(1)?, 1.0);
            let lig = num(parts.get(2)?, 1.0);
            let q = if lig < 0.5 {
                lig * (1.0 + sat)
            } else {
                lig + sat - lig * sat
            };
            let p = 2.0 * lig - q;
            let f = |t: f32| {
                let t = t.rem_euclid(1.0);
                let v = if t < 1.0 / 6.0 {
                    p + (q - p) * 6.0 * t
                } else if t < 0.5 {
                    q
                } else if t < 2.0 / 3.0 {
                    p + (q - p) * (2.0 / 3.0 - t) * 6.0
                } else {
                    p
                };
                (v * 255.0).round().clamp(0.0, 255.0) as u8
            };
            return Some([f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0), (a * 255.0) as u8]);
        }
        let c = |i: usize| -> Option<u8> {
            Some(num(parts.get(i)?, 255.0).round().clamp(0.0, 255.0) as u8)
        };
        return Some([c(0)?, c(1)?, c(2)?, (a.clamp(0.0, 1.0) * 255.0) as u8]);
    }
    let named: &[(&str, u32)] = &[
        ("black", 0x000000),
        ("white", 0xFFFFFF),
        ("red", 0xFF0000),
        ("green", 0x008000),
        ("blue", 0x0000FF),
        ("yellow", 0xFFFF00),
        ("cyan", 0x00FFFF),
        ("aqua", 0x00FFFF),
        ("magenta", 0xFF00FF),
        ("fuchsia", 0xFF00FF),
        ("gray", 0x808080),
        ("grey", 0x808080),
        ("silver", 0xC0C0C0),
        ("maroon", 0x800000),
        ("olive", 0x808000),
        ("lime", 0x00FF00),
        ("teal", 0x008080),
        ("navy", 0x000080),
        ("purple", 0x800080),
        ("orange", 0xFFA500),
        ("pink", 0xFFC0CB),
        ("brown", 0xA52A2A),
        ("gold", 0xFFD700),
        ("darkgray", 0xA9A9A9),
        ("darkgrey", 0xA9A9A9),
        ("lightgray", 0xD3D3D3),
        ("lightgrey", 0xD3D3D3),
        ("darkblue", 0x00008B),
        ("darkred", 0x8B0000),
        ("darkgreen", 0x006400),
        ("lightblue", 0xADD8E6),
        ("skyblue", 0x87CEEB),
        ("steelblue", 0x4682B4),
        ("royalblue", 0x4169E1),
        ("crimson", 0xDC143C),
        ("tomato", 0xFF6347),
        ("coral", 0xFF7F50),
        ("salmon", 0xFA8072),
        ("khaki", 0xF0E68C),
        ("violet", 0xEE82EE),
        ("indigo", 0x4B0082),
        ("tan", 0xD2B48C),
        ("beige", 0xF5F5DC),
        ("ivory", 0xFFFFF0),
        ("wheat", 0xF5DEB3),
        ("chocolate", 0xD2691E),
        ("firebrick", 0xB22222),
        ("forestgreen", 0x228B22),
        ("seagreen", 0x2E8B57),
        ("limegreen", 0x32CD32),
        ("darkorange", 0xFF8C00),
        ("dodgerblue", 0x1E90FF),
        ("deepskyblue", 0x00BFFF),
        ("slategray", 0x708090),
        ("dimgray", 0x696969),
        ("whitesmoke", 0xF5F5F5),
        ("gainsboro", 0xDCDCDC),
        ("lavender", 0xE6E6FA),
        ("plum", 0xDDA0DD),
        ("orchid", 0xDA70D6),
        ("turquoise", 0x40E0D0),
        ("midnightblue", 0x191970),
        ("darkslategray", 0x2F4F4F),
        ("goldenrod", 0xDAA520),
        ("sienna", 0xA0522D),
        ("peru", 0xCD853F),
        ("hotpink", 0xFF69B4),
        ("deeppink", 0xFF1493),
    ];
    if l == "transparent" || l == "none" {
        return Some([0, 0, 0, 0]);
    }
    named
        .iter()
        .find(|(n, _)| *n == l)
        .map(|(_, v)| [(v >> 16) as u8, (v >> 8) as u8, *v as u8, 255])
}

fn nums(s: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'-' || c == b'+' || c == b'.' || c.is_ascii_digit() {
            let st = i;
            i += 1;
            let mut dot = c == b'.';
            let mut exp = false;
            while i < b.len() {
                let d = b[i];
                if d.is_ascii_digit() {
                    i += 1;
                } else if d == b'.' && !dot && !exp {
                    dot = true;
                    i += 1;
                } else if (d == b'e' || d == b'E') && !exp {
                    exp = true;
                    i += 1;
                    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if let Ok(v) = s[st..i].parse() {
                out.push(v);
            }
        } else {
            i += 1;
        }
    }
    out
}

fn length(s: Option<&str>, rel: f32) -> Option<f32> {
    let s = s?.trim();
    let n: String = s
        .chars()
        .take_while(|c| {
            c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+' || *c == 'e' || *c == 'E'
        })
        .collect();
    let v: f32 = n.parse().ok()?;
    let unit = s[n.len()..].trim();
    Some(match unit {
        "%" => v * rel / 100.0,
        "mm" => v * 3.779_528,
        "cm" => v * 37.795_28,
        "in" => v * 96.0,
        "pt" => v * 4.0 / 3.0,
        "pc" => v * 16.0,
        "em" => v * 16.0,
        "ex" => v * 8.0,
        _ => v,
    })
}

fn transform(s: &str) -> Mat {
    let mut m = Mat::ID;
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let v = nums(&rest[open + 1..open + close]);
        let g = |i: usize, d: f32| v.get(i).copied().unwrap_or(d);
        let t = match name {
            "matrix" if v.len() >= 6 => Mat([v[0], v[1], v[2], v[3], v[4], v[5]]),
            "translate" => Mat::translate(g(0, 0.0), g(1, 0.0)),
            "scale" => Mat::scale(g(0, 1.0), g(1, g(0, 1.0))),
            "rotate" => {
                let a = g(0, 0.0).to_radians();
                let r = Mat([a.cos(), a.sin(), -a.sin(), a.cos(), 0.0, 0.0]);
                if v.len() >= 3 {
                    Mat::translate(v[1], v[2])
                        .mul(&r)
                        .mul(&Mat::translate(-v[1], -v[2]))
                } else {
                    r
                }
            }
            "skewX" => Mat([1.0, 0.0, g(0, 0.0).to_radians().tan(), 1.0, 0.0, 0.0]),
            "skewY" => Mat([1.0, g(0, 0.0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => Mat::ID,
        };
        m = m.mul(&t);
        rest = &rest[open + close + 1..];
    }
    m
}

#[derive(Clone, Debug)]
pub enum Seg {
    Move(Pt),
    Line(Pt),
    Cubic(Pt, Pt, Pt),
    Close,
}

fn arc_to(
    out: &mut Vec<Seg>,
    p0: Pt,
    rx: f32,
    ry: f32,
    phi: f32,
    large: bool,
    sweep: bool,
    p1: Pt,
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-6 || ry < 1e-6 {
        out.push(Seg::Line(p1));
        return;
    }
    let (s, c) = phi.to_radians().sin_cos();
    let dx = (p0.0 - p1.0) / 2.0;
    let dy = (p0.1 - p1.1) / 2.0;
    let x1 = c * dx + s * dy;
    let y1 = -s * dx + c * dy;
    let lam = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if lam > 1.0 {
        rx *= lam.sqrt();
        ry *= lam.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut k = (num / den).max(0.0).sqrt();
    if large == sweep {
        k = -k;
    }
    let cx1 = k * rx * y1 / ry;
    let cy1 = -k * ry * x1 / rx;
    let cx = c * cx1 - s * cy1 + (p0.0 + p1.0) / 2.0;
    let cy = s * cx1 + c * cy1 + (p0.1 + p1.1) / 2.0;
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let d = (ux * vx + uy * vy) / ((ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt());
        let a = d.clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            -a
        } else {
            a
        }
    };
    let t1 = ang(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dt = ang(
        (x1 - cx1) / rx,
        (y1 - cy1) / ry,
        (-x1 - cx1) / rx,
        (-y1 - cy1) / ry,
    );
    if !sweep && dt > 0.0 {
        dt -= std::f32::consts::TAU;
    } else if sweep && dt < 0.0 {
        dt += std::f32::consts::TAU;
    }
    let n = (dt.abs() / (std::f32::consts::FRAC_PI_2)).ceil().max(1.0) as usize;
    let step = dt / n as f32;
    let alpha = 4.0 / 3.0 * (step / 4.0).tan();
    let pt = |t: f32| {
        let (st, ct) = t.sin_cos();
        (
            cx + rx * ct * c - ry * st * s,
            cy + rx * ct * s + ry * st * c,
        )
    };
    let der = |t: f32| {
        let (st, ct) = t.sin_cos();
        (-rx * st * c - ry * ct * s, -rx * st * s + ry * ct * c)
    };
    let mut t = t1;
    for _ in 0..n {
        let (a, b) = (pt(t), pt(t + step));
        let (da, db) = (der(t), der(t + step));
        out.push(Seg::Cubic(
            (a.0 + alpha * da.0, a.1 + alpha * da.1),
            (b.0 - alpha * db.0, b.1 - alpha * db.1),
            b,
        ));
        t += step;
    }
}

pub fn parse_path(d: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    let mut cmd = b'M';
    let mut cur: Pt = (0.0, 0.0);
    let mut start: Pt = (0.0, 0.0);
    let mut last_ctrl: Option<Pt> = None;
    let mut last_q: Option<Pt> = None;
    let num = |i: &mut usize| -> Option<f32> {
        while *i < b.len() && (b[*i].is_ascii_whitespace() || b[*i] == b',') {
            *i += 1;
        }
        let st = *i;
        if *i < b.len() && (b[*i] == b'-' || b[*i] == b'+') {
            *i += 1;
        }
        let mut dot = false;
        let mut digits = false;
        while *i < b.len() {
            let c = b[*i];
            if c.is_ascii_digit() {
                digits = true;
                *i += 1;
            } else if c == b'.' && !dot {
                dot = true;
                *i += 1;
            } else if (c == b'e' || c == b'E') && digits {
                *i += 1;
                if *i < b.len() && (b[*i] == b'-' || b[*i] == b'+') {
                    *i += 1;
                }
            } else {
                break;
            }
        }
        if !digits {
            *i = st;
            return None;
        }
        d[st..*i].parse().ok()
    };
    let flag = |i: &mut usize| -> Option<bool> {
        while *i < b.len() && (b[*i].is_ascii_whitespace() || b[*i] == b',') {
            *i += 1;
        }
        let c = *b.get(*i)?;
        *i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    };
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i].is_ascii_alphabetic() && b[i] != b'e' && b[i] != b'E' {
            cmd = b[i];
            i += 1;
            if cmd == b'Z' || cmd == b'z' {
                out.push(Seg::Close);
                cur = start;
                last_ctrl = None;
                last_q = None;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur } else { (0.0, 0.0) };
        let before = i;
        macro_rules! n {
            () => {
                match num(&mut i) {
                    Some(v) => v,
                    None => break,
                }
            };
        }
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = (n!(), n!());
                cur = (base.0 + x, base.1 + y);
                start = cur;
                out.push(Seg::Move(cur));
                cmd = if rel { b'l' } else { b'L' };
                last_ctrl = None;
                last_q = None;
            }
            b'L' => {
                let (x, y) = (n!(), n!());
                cur = (base.0 + x, base.1 + y);
                out.push(Seg::Line(cur));
                last_ctrl = None;
                last_q = None;
            }
            b'H' => {
                let x = n!();
                cur = (if rel { cur.0 + x } else { x }, cur.1);
                out.push(Seg::Line(cur));
                last_ctrl = None;
                last_q = None;
            }
            b'V' => {
                let y = n!();
                cur = (cur.0, if rel { cur.1 + y } else { y });
                out.push(Seg::Line(cur));
                last_ctrl = None;
                last_q = None;
            }
            b'C' => {
                let v = [n!(), n!(), n!(), n!(), n!(), n!()];
                let c1 = (base.0 + v[0], base.1 + v[1]);
                let c2 = (base.0 + v[2], base.1 + v[3]);
                cur = (base.0 + v[4], base.1 + v[5]);
                out.push(Seg::Cubic(c1, c2, cur));
                last_ctrl = Some(c2);
                last_q = None;
            }
            b'S' => {
                let v = [n!(), n!(), n!(), n!()];
                let c1 = match last_ctrl {
                    Some(c) => (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1),
                    None => cur,
                };
                let c2 = (base.0 + v[0], base.1 + v[1]);
                cur = (base.0 + v[2], base.1 + v[3]);
                out.push(Seg::Cubic(c1, c2, cur));
                last_ctrl = Some(c2);
                last_q = None;
            }
            b'Q' | b'T' => {
                let q = if cmd.eq_ignore_ascii_case(&b'Q') {
                    let (x, y) = (n!(), n!());
                    (base.0 + x, base.1 + y)
                } else {
                    match last_q {
                        Some(c) => (2.0 * cur.0 - c.0, 2.0 * cur.1 - c.1),
                        None => cur,
                    }
                };
                let (x, y) = (n!(), n!());
                let end = (base.0 + x, base.1 + y);
                let c1 = (
                    cur.0 + 2.0 / 3.0 * (q.0 - cur.0),
                    cur.1 + 2.0 / 3.0 * (q.1 - cur.1),
                );
                let c2 = (
                    end.0 + 2.0 / 3.0 * (q.0 - end.0),
                    end.1 + 2.0 / 3.0 * (q.1 - end.1),
                );
                out.push(Seg::Cubic(c1, c2, end));
                cur = end;
                last_q = Some(q);
                last_ctrl = None;
            }
            b'A' => {
                let rx = n!();
                let ry = n!();
                let rot = n!();
                let (Some(large), Some(sweep)) = (flag(&mut i), flag(&mut i)) else {
                    break;
                };
                let (x, y) = (n!(), n!());
                let end = (base.0 + x, base.1 + y);
                arc_to(&mut out, cur, rx, ry, rot, large, sweep, end);
                cur = end;
                last_ctrl = None;
                last_q = None;
            }
            _ => break,
        }
        if i == before {
            break;
        }
    }
    out
}

#[derive(Clone, Debug)]
enum PaintRef {
    None,
    Color([f32; 4]),
    Url(String, Option<[f32; 4]>),
}

#[derive(Clone, Debug)]
struct Style {
    fill: PaintRef,
    stroke: PaintRef,
    stroke_width: f32,
    fill_opacity: f32,
    stroke_opacity: f32,
    opacity: f32,
    evenodd: bool,
    cap: u8,
    join: u8,
    color: [f32; 4],
    display: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            fill: PaintRef::Color([0.0, 0.0, 0.0, 1.0]),
            stroke: PaintRef::None,
            stroke_width: 1.0,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            opacity: 1.0,
            evenodd: false,
            cap: 0,
            join: 0,
            color: [0.0, 0.0, 0.0, 1.0],
            display: true,
        }
    }
}

fn f4(c: [u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

#[derive(Clone, Debug)]
pub enum Item {
    Shape {
        segs: Vec<Seg>,
        ctm: Mat,
        fill: Option<(Paint, f32)>,
        stroke: Option<(Paint, f32, f32, u8, u8)>,
        evenodd: bool,
        bbox: [f32; 4],
    },
    Image {
        ctm: Mat,
        raster: super::Raster,
        rect: [f32; 4],
        opacity: f32,
    },
}

#[derive(Clone, Debug)]
pub struct Svg {
    pub w: f32,
    pub h: f32,
    pub items: Vec<Item>,
}

struct Builder<'a> {
    ids: HashMap<String, &'a Node>,
    css: Vec<(String, Vec<(String, String)>)>,
    items: Vec<Item>,
    vw: f32,
    vh: f32,
    depth: usize,
}

fn index<'a>(n: &'a Node, ids: &mut HashMap<String, &'a Node>, css: &mut String) {
    if let Some(id) = n.attr("id") {
        ids.insert(id.to_string(), n);
    }
    if n.name == "style" {
        css.push_str(&n.text);
        css.push('\n');
    }
    for k in &n.kids {
        index(k, ids, css);
    }
}

fn parse_css(s: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(o) = rest.find('{') {
        let Some(c) = rest[o..].find('}') else {
            break;
        };
        let sel = rest[..o].trim();
        let body = &rest[o + 1..o + c];
        let props = decls(body);
        for one in sel.split(',') {
            let one = one.trim();
            if !one.is_empty() && !one.starts_with('@') {
                out.push((one.to_string(), props.clone()));
            }
        }
        rest = &rest[o + c + 1..];
    }
    out
}

fn decls(s: &str) -> Vec<(String, String)> {
    s.split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            Some((
                k.trim().to_string(),
                v.trim().trim_end_matches("!important").trim().to_string(),
            ))
        })
        .collect()
}

impl<'a> Builder<'a> {
    fn props(&self, n: &Node) -> Vec<(String, String)> {
        let mut p: Vec<(String, String)> = n.attrs.clone();
        let classes: Vec<&str> = n
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        for (sel, d) in &self.css {
            let hit = if let Some(c) = sel.strip_prefix('.') {
                classes.contains(&c)
            } else if let Some(i) = sel.strip_prefix('#') {
                n.attr("id") == Some(i)
            } else {
                sel == &n.name || sel == "*"
            };
            if hit {
                p.extend(d.iter().cloned());
            }
        }
        if let Some(st) = n.attr("style") {
            p.extend(decls(st));
        }
        p
    }

    fn style(&self, n: &Node, parent: &Style) -> Style {
        let mut s = parent.clone();
        s.opacity = 1.0;
        s.display = true;
        let props = self.props(n);
        let paint = |v: &str, s: &Style| -> PaintRef {
            let v = v.trim();
            if v == "none" {
                PaintRef::None
            } else if v == "currentColor" {
                PaintRef::Color(s.color)
            } else if let Some(u) = v.strip_prefix("url(") {
                let (id, fb) = u.split_once(')').unwrap_or((u, ""));
                let id = id
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"')
                    .trim_start_matches('#');
                PaintRef::Url(id.to_string(), parse_color(fb).map(f4))
            } else {
                match parse_color(v) {
                    Some(c) => PaintRef::Color(f4(c)),
                    None => PaintRef::Color([0.0, 0.0, 0.0, 1.0]),
                }
            }
        };
        for (k, v) in props.iter() {
            if k == "color" {
                if let Some(c) = parse_color(v) {
                    s.color = f4(c);
                }
            }
        }
        for (k, v) in props.iter() {
            if v == "inherit" {
                continue;
            }
            match k.as_str() {
                "fill" => s.fill = paint(v, &s),
                "stroke" => s.stroke = paint(v, &s),
                "stroke-width" => s.stroke_width = length(Some(v), self.diag()).unwrap_or(1.0),
                "fill-opacity" => s.fill_opacity = opacity(v),
                "stroke-opacity" => s.stroke_opacity = opacity(v),
                "opacity" => s.opacity = opacity(v),
                "fill-rule" => s.evenodd = v == "evenodd",
                "stroke-linecap" => {
                    s.cap = match v.as_str() {
                        "round" => 1,
                        "square" => 2,
                        _ => 0,
                    }
                }
                "stroke-linejoin" => {
                    s.join = match v.as_str() {
                        "round" => 1,
                        "bevel" => 2,
                        _ => 0,
                    }
                }
                "display" if v == "none" => s.display = false,
                "visibility" if v == "hidden" || v == "collapse" => s.display = false,
                _ => {}
            }
        }
        s
    }

    fn diag(&self) -> f32 {
        ((self.vw * self.vw + self.vh * self.vh) / 2.0).sqrt()
    }

    fn resolve_paint(
        &self,
        p: &PaintRef,
        op: f32,
        bbox: [f32; 4],
        ctm: &Mat,
    ) -> Option<(Paint, f32)> {
        match p {
            PaintRef::None => None,
            PaintRef::Color(c) => Some((Paint::Solid(*c), op)),
            PaintRef::Url(id, fb) => match self.gradient(id, bbox, ctm, 0) {
                Some(g) => Some((g, op)),
                None => fb.map(|c| (Paint::Solid(c), op)),
            },
        }
    }

    /// Dégradé en espace utilisateur ; l'inverse de la matrice est calculé
    /// au rendu (voir `bind`).
    fn gradient(&self, id: &str, bbox: [f32; 4], ctm: &Mat, depth: usize) -> Option<Paint> {
        let n = *self.ids.get(id)?;
        if n.name != "linearGradient" && n.name != "radialGradient" {
            return None;
        }
        // héritage xlink:href
        let mut chain = vec![n];
        let mut cur = n;
        while let Some(h) = cur.attr("xlink:href").or(cur.attr("href")) {
            let Some(next) = self.ids.get(h.trim_start_matches('#')) else {
                break;
            };
            if chain.len() > 8 || depth > 8 {
                break;
            }
            chain.push(next);
            cur = next;
        }
        let a = |k: &str| chain.iter().find_map(|c| c.attr(k));
        let stops_node = chain
            .iter()
            .find(|c| c.kids.iter().any(|k| k.name == "stop"))?;
        let mut stops = Vec::new();
        for s in stops_node.kids.iter().filter(|k| k.name == "stop") {
            let props = self.props(s);
            let get = |k: &str| {
                props
                    .iter()
                    .rev()
                    .find(|(n, _)| n == k)
                    .map(|(_, v)| v.as_str())
            };
            let off = get("offset")
                .map(|o| {
                    if let Some(p) = o.strip_suffix('%') {
                        p.parse::<f32>().unwrap_or(0.0) / 100.0
                    } else {
                        o.parse().unwrap_or(0.0)
                    }
                })
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let mut c = get("stop-color")
                .and_then(parse_color)
                .map(f4)
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            c[3] *= get("stop-opacity").map(opacity).unwrap_or(1.0);
            let off = stops.last().map(|l: &Stop| off.max(l.off)).unwrap_or(off);
            stops.push(Stop { off, color: c });
        }
        if stops.is_empty() {
            return None;
        }
        let obb = a("gradientUnits") != Some("userSpaceOnUse");
        let gt = a("gradientTransform").map(transform).unwrap_or(Mat::ID);
        let unit = if obb {
            Mat([
                bbox[2] - bbox[0],
                0.0,
                0.0,
                bbox[3] - bbox[1],
                bbox[0],
                bbox[1],
            ])
        } else {
            Mat::ID
        };
        let m = ctm.mul(&unit).mul(&gt);
        let spread = match a("spreadMethod") {
            Some("reflect") => 1,
            Some("repeat") => 2,
            _ => 0,
        };
        let rel = |k: &str, d: f32, size: f32| -> f32 {
            match a(k) {
                Some(v) if obb => {
                    if let Some(p) = v.strip_suffix('%') {
                        p.parse::<f32>().unwrap_or(d * 100.0) / 100.0
                    } else {
                        v.parse().unwrap_or(d)
                    }
                }
                Some(v) => length(Some(v), size).unwrap_or(d * size),
                None => {
                    if obb {
                        d
                    } else {
                        d * size
                    }
                }
            }
        };
        if n.name == "linearGradient" {
            let l = [
                rel("x1", 0.0, self.vw),
                rel("y1", 0.0, self.vh),
                rel("x2", 1.0, self.vw),
                rel("y2", 0.0, self.vh),
            ];
            Some(Paint::Linear(m, l, stops, spread))
        } else {
            let cx = rel("cx", 0.5, self.vw);
            let cy = rel("cy", 0.5, self.vh);
            let r = rel("r", 0.5, self.diag());
            let fx = if a("fx").is_some() {
                rel("fx", 0.5, self.vw)
            } else {
                cx
            };
            let fy = if a("fy").is_some() {
                rel("fy", 0.5, self.vh)
            } else {
                cy
            };
            Some(Paint::Radial(m, [cx, cy, r, fx, fy], stops, spread))
        }
    }

    fn walk(&mut self, n: &'a Node, ctm: Mat, parent: &Style, op: f32) {
        self.depth += 1;
        if self.depth > 64 || self.items.len() > 200_000 {
            self.depth -= 1;
            return;
        }
        let st = self.style(n, parent);
        if !st.display {
            self.depth -= 1;
            return;
        }
        let ctm = match n.attr("transform") {
            Some(t) => ctm.mul(&transform(t)),
            None => ctm,
        };
        let op = op * st.opacity;
        let vw = self.vw;
        let vh = self.vh;
        let num = |k: &str, rel: f32| length(n.attr(k), rel).unwrap_or(0.0);
        let segs: Option<Vec<Seg>> = match n.name.as_str() {
            "g" | "a" | "switch" => {
                for k in &n.kids {
                    self.walk(k, ctm, &st, op);
                }
                None
            }
            "svg" if self.depth > 1 => {
                let x = num("x", vw);
                let y = num("y", vh);
                let m = ctm.mul(&Mat::translate(x, y)).mul(&viewbox(
                    n,
                    num("width", vw),
                    num("height", vh),
                ));
                for k in &n.kids {
                    self.walk(k, m, &st, op);
                }
                None
            }
            "svg" => {
                for k in &n.kids {
                    self.walk(k, ctm, &st, op);
                }
                None
            }
            "use" => {
                let href = n.attr("xlink:href").or(n.attr("href")).unwrap_or("");
                if let Some(&t) = self.ids.get(href.trim_start_matches('#')) {
                    let m = ctm.mul(&Mat::translate(num("x", vw), num("y", vh)));
                    if t.name == "symbol" {
                        let w = length(n.attr("width"), vw).unwrap_or(vw);
                        let h = length(n.attr("height"), vh).unwrap_or(vh);
                        let m = m.mul(&viewbox(t, w, h));
                        for k in &t.kids {
                            self.walk(k, m, &st, op);
                        }
                    } else {
                        self.walk(t, m, &st, op);
                    }
                }
                None
            }
            "path" => Some(parse_path(n.attr("d").unwrap_or(""))),
            "rect" => {
                let (x, y, w, h) = (
                    num("x", vw),
                    num("y", vh),
                    num("width", vw),
                    num("height", vh),
                );
                if w <= 0.0 || h <= 0.0 {
                    None
                } else {
                    let mut rx = length(n.attr("rx"), vw);
                    let mut ry = length(n.attr("ry"), vh);
                    if rx.is_none() {
                        rx = ry;
                    }
                    if ry.is_none() {
                        ry = rx;
                    }
                    let rx = rx.unwrap_or(0.0).min(w / 2.0);
                    let ry = ry.unwrap_or(0.0).min(h / 2.0);
                    if rx > 0.0 && ry > 0.0 {
                        let mut s = vec![Seg::Move((x + rx, y)), Seg::Line((x + w - rx, y))];
                        arc_to(
                            &mut s,
                            (x + w - rx, y),
                            rx,
                            ry,
                            0.0,
                            false,
                            true,
                            (x + w, y + ry),
                        );
                        s.push(Seg::Line((x + w, y + h - ry)));
                        arc_to(
                            &mut s,
                            (x + w, y + h - ry),
                            rx,
                            ry,
                            0.0,
                            false,
                            true,
                            (x + w - rx, y + h),
                        );
                        s.push(Seg::Line((x + rx, y + h)));
                        arc_to(
                            &mut s,
                            (x + rx, y + h),
                            rx,
                            ry,
                            0.0,
                            false,
                            true,
                            (x, y + h - ry),
                        );
                        s.push(Seg::Line((x, y + ry)));
                        arc_to(&mut s, (x, y + ry), rx, ry, 0.0, false, true, (x + rx, y));
                        s.push(Seg::Close);
                        Some(s)
                    } else {
                        Some(vec![
                            Seg::Move((x, y)),
                            Seg::Line((x + w, y)),
                            Seg::Line((x + w, y + h)),
                            Seg::Line((x, y + h)),
                            Seg::Close,
                        ])
                    }
                }
            }
            "circle" | "ellipse" => {
                let cx = num("cx", vw);
                let cy = num("cy", vh);
                let (rx, ry) = if n.name == "circle" {
                    let r = num("r", self.diag());
                    (r, r)
                } else {
                    (num("rx", vw), num("ry", vh))
                };
                if rx <= 0.0 || ry <= 0.0 {
                    None
                } else {
                    let mut s = vec![Seg::Move((cx + rx, cy))];
                    arc_to(
                        &mut s,
                        (cx + rx, cy),
                        rx,
                        ry,
                        0.0,
                        false,
                        true,
                        (cx - rx, cy),
                    );
                    arc_to(
                        &mut s,
                        (cx - rx, cy),
                        rx,
                        ry,
                        0.0,
                        false,
                        true,
                        (cx + rx, cy),
                    );
                    s.push(Seg::Close);
                    Some(s)
                }
            }
            "line" => Some(vec![
                Seg::Move((num("x1", vw), num("y1", vh))),
                Seg::Line((num("x2", vw), num("y2", vh))),
            ]),
            "polyline" | "polygon" => {
                let v = nums(n.attr("points").unwrap_or(""));
                let mut s = Vec::new();
                for (i, c) in v.chunks_exact(2).enumerate() {
                    s.push(if i == 0 {
                        Seg::Move((c[0], c[1]))
                    } else {
                        Seg::Line((c[0], c[1]))
                    });
                }
                if n.name == "polygon" && !s.is_empty() {
                    s.push(Seg::Close);
                }
                Some(s)
            }
            "image" => {
                let href = n.attr("xlink:href").or(n.attr("href")).unwrap_or("");
                if let Some(r) = data_uri_image(href) {
                    let w = length(n.attr("width"), vw).unwrap_or(r.w as f32);
                    let h = length(n.attr("height"), vh).unwrap_or(r.h as f32);
                    self.items.push(Item::Image {
                        ctm,
                        rect: [num("x", vw), num("y", vh), w, h],
                        raster: r,
                        opacity: op,
                    });
                }
                None
            }
            _ => None,
        };
        if let Some(segs) = segs {
            if !segs.is_empty() {
                let bbox = seg_bbox(&segs);
                let fill = self.resolve_paint(&st.fill, st.fill_opacity * op, bbox, &ctm);
                let stroke = self
                    .resolve_paint(&st.stroke, st.stroke_opacity * op, bbox, &ctm)
                    .map(|(p, o)| (p, o, st.stroke_width, st.cap, st.join));
                let fill = if n.name == "line" { None } else { fill };
                if fill.is_some() || stroke.is_some() {
                    self.items.push(Item::Shape {
                        segs,
                        ctm,
                        fill,
                        stroke,
                        evenodd: st.evenodd,
                        bbox,
                    });
                }
            }
        }
        self.depth -= 1;
    }
}

fn opacity(v: &str) -> f32 {
    let v = v.trim();
    if let Some(p) = v.strip_suffix('%') {
        p.parse::<f32>().unwrap_or(100.0) / 100.0
    } else {
        v.parse().unwrap_or(1.0)
    }
    .clamp(0.0, 1.0)
}

fn seg_bbox(segs: &[Seg]) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    let mut add = |p: &Pt| {
        b[0] = b[0].min(p.0);
        b[1] = b[1].min(p.1);
        b[2] = b[2].max(p.0);
        b[3] = b[3].max(p.1);
    };
    for s in segs {
        match s {
            Seg::Move(p) | Seg::Line(p) => add(p),
            Seg::Cubic(a, c, p) => {
                add(a);
                add(c);
                add(p);
            }
            Seg::Close => {}
        }
    }
    if b[0] > b[2] {
        return [0.0, 0.0, 1.0, 1.0];
    }
    if b[2] - b[0] < 1e-6 {
        b[2] = b[0] + 1e-6;
    }
    if b[3] - b[1] < 1e-6 {
        b[3] = b[1] + 1e-6;
    }
    b
}

fn data_uri_image(href: &str) -> Option<super::Raster> {
    let rest = href.trim().strip_prefix("data:")?;
    let (_, b64) = rest.split_once(";base64,")?;
    let bytes = super::base64(b64)?;
    let img = super::decode_bytes(&bytes, "").ok()?;
    match img.pages.into_iter().next()?.content {
        super::Content::Raster(r) => Some(r),
        _ => None,
    }
}

/// Transformation viewBox -> viewport (preserveAspectRatio xMidYMid meet
/// par défaut, `none` respecté).
fn viewbox(n: &Node, w: f32, h: f32) -> Mat {
    let Some(vb) = n
        .attr("viewBox")
        .map(nums)
        .filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0)
    else {
        return Mat::ID;
    };
    let par = n.attr("preserveAspectRatio").unwrap_or("xMidYMid meet");
    let sx = w / vb[2];
    let sy = h / vb[3];
    if par.starts_with("none") {
        return Mat::scale(sx, sy).mul(&Mat::translate(-vb[0], -vb[1]));
    }
    let s = if par.contains("slice") {
        sx.max(sy)
    } else {
        sx.min(sy)
    };
    let ax = if par.contains("xMin") {
        0.0
    } else if par.contains("xMax") {
        1.0
    } else {
        0.5
    };
    let ay = if par.contains("YMin") {
        0.0
    } else if par.contains("YMax") {
        1.0
    } else {
        0.5
    };
    let tx = (w - vb[2] * s) * ax;
    let ty = (h - vb[3] * s) * ay;
    Mat::translate(tx, ty)
        .mul(&Mat::scale(s, s))
        .mul(&Mat::translate(-vb[0], -vb[1]))
}

pub fn parse(data: &[u8]) -> Option<Svg> {
    let text = String::from_utf8_lossy(data);
    let root = parse_xml(&text)?;
    if root.name != "svg" {
        return None;
    }
    let mut ids = HashMap::new();
    let mut css = String::new();
    index(&root, &mut ids, &mut css);
    let vb = root.attr("viewBox").map(nums).filter(|v| v.len() == 4);
    let (dw, dh) = match &vb {
        Some(v) => (v[2], v[3]),
        None => (300.0, 150.0),
    };
    let w = length(root.attr("width"), dw)
        .filter(|v| *v > 0.0)
        .unwrap_or(dw);
    let h = length(root.attr("height"), dh)
        .filter(|v| *v > 0.0)
        .unwrap_or(dh);
    let (w, h) = if root.attr("width").is_some_and(|v| v.ends_with('%')) {
        (dw, dh)
    } else {
        (w, h)
    };
    let mut b = Builder {
        ids,
        css: parse_css(&css),
        items: Vec::new(),
        vw: vb.as_ref().map(|v| v[2]).unwrap_or(w),
        vh: vb.as_ref().map(|v| v[3]).unwrap_or(h),
        depth: 0,
    };
    let base = viewbox(&root, w, h);
    b.walk(&root, base, &Style::default(), 1.0);
    let items = b.items;
    Some(Svg {
        w: w.max(1.0),
        h: h.max(1.0),
        items,
    })
}

fn flatten(segs: &[Seg], m: &Mat, tol: f32) -> Vec<(Vec<Pt>, bool)> {
    let mut out: Vec<(Vec<Pt>, bool)> = Vec::new();
    let mut cur: Vec<Pt> = Vec::new();
    let mut pos: Pt = (0.0, 0.0);
    let mut start: Pt = (0.0, 0.0);
    for s in segs {
        match s {
            Seg::Move(p) => {
                if cur.len() > 1 {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur.clear();
                pos = *p;
                start = *p;
                cur.push(m.apply(*p));
            }
            Seg::Line(p) => {
                if cur.is_empty() {
                    cur.push(m.apply(pos));
                }
                pos = *p;
                cur.push(m.apply(*p));
            }
            Seg::Cubic(a, b, p) => {
                if cur.is_empty() {
                    cur.push(m.apply(pos));
                }
                let p0 = m.apply(pos);
                raster::cubic(&mut cur, p0, m.apply(*a), m.apply(*b), m.apply(*p), tol);
                pos = *p;
            }
            Seg::Close => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), true));
                }
                pos = start;
            }
        }
    }
    if cur.len() > 1 {
        out.push((cur, false));
    }
    out
}

fn bind(p: &Paint, view: &Mat) -> Paint {
    // Les matrices des dégradés vont de l'espace du dégradé vers l'espace
    // utilisateur racine : on compose avec la vue puis on inverse.
    match p {
        Paint::Solid(c) => Paint::Solid(*c),
        Paint::Linear(m, l, s, sp) => match view.mul(m).inv() {
            Some(i) => Paint::Linear(i, *l, s.clone(), *sp),
            None => Paint::Solid(s.last().map(|s| s.color).unwrap_or([0.0; 4])),
        },
        Paint::Radial(m, g, s, sp) => match view.mul(m).inv() {
            Some(i) => Paint::Radial(i, *g, s.clone(), *sp),
            None => Paint::Solid(s.last().map(|s| s.color).unwrap_or([0.0; 4])),
        },
    }
}

/// Dessine le document avec la matrice `view` (unités SVG -> pixels).
pub fn render(svg: &Svg, view: &Mat, cv: &mut Canvas) {
    for it in &svg.items {
        match it {
            Item::Shape {
                segs,
                ctm,
                fill,
                stroke,
                evenodd,
                bbox,
            } => {
                let m = view.mul(ctm);
                // élimine les formes hors champ
                let corners = [
                    m.apply((bbox[0], bbox[1])),
                    m.apply((bbox[2], bbox[1])),
                    m.apply((bbox[0], bbox[3])),
                    m.apply((bbox[2], bbox[3])),
                ];
                let sw = stroke
                    .as_ref()
                    .map(|s| s.2 * m.scale_factor())
                    .unwrap_or(0.0);
                let minx = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min) - sw;
                let maxx = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max) + sw;
                let miny = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min) - sw;
                let maxy = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max) + sw;
                if maxx < 0.0 || maxy < 0.0 || minx > cv.w as f32 || miny > cv.h as f32 {
                    continue;
                }
                let lines = flatten(segs, &m, 0.25);
                if let Some((p, o)) = fill {
                    let polys: Vec<Vec<Pt>> = lines.iter().map(|(l, _)| l.clone()).collect();
                    raster::fill(cv, &polys, *evenodd, &bind(p, view), *o);
                }
                if let Some((p, o, w, cap, join)) = stroke {
                    let polys = raster::stroke_polys(&lines, w * m.scale_factor(), *cap, *join);
                    raster::fill(cv, &polys, false, &bind(p, view), *o);
                }
            }
            Item::Image {
                ctm,
                raster: r,
                rect,
                opacity,
            } => {
                let m = view
                    .mul(ctm)
                    .mul(&Mat::translate(rect[0], rect[1]))
                    .mul(&Mat::scale(
                        rect[2] / r.w.max(1) as f32,
                        rect[3] / r.h.max(1) as f32,
                    ));
                let Some(inv) = m.inv() else {
                    continue;
                };
                let corners = [
                    m.apply((0.0, 0.0)),
                    m.apply((r.w as f32, 0.0)),
                    m.apply((0.0, r.h as f32)),
                    m.apply((r.w as f32, r.h as f32)),
                ];
                let x0 = corners
                    .iter()
                    .map(|c| c.0)
                    .fold(f32::MAX, f32::min)
                    .max(0.0) as usize;
                let x1 = (corners
                    .iter()
                    .map(|c| c.0)
                    .fold(f32::MIN, f32::max)
                    .ceil()
                    .max(0.0) as usize)
                    .min(cv.w);
                let y0 = corners
                    .iter()
                    .map(|c| c.1)
                    .fold(f32::MAX, f32::min)
                    .max(0.0) as usize;
                let y1 = (corners
                    .iter()
                    .map(|c| c.1)
                    .fold(f32::MIN, f32::max)
                    .ceil()
                    .max(0.0) as usize)
                    .min(cv.h);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let (u, v) = inv.apply((x as f32 + 0.5, y as f32 + 0.5));
                        if u < 0.0 || v < 0.0 || u >= r.w as f32 || v >= r.h as f32 {
                            continue;
                        }
                        let c = r.bilinear(u, v);
                        let a = c[3] * opacity;
                        let d = &mut cv.px[y * cv.w + x];
                        let k = 1.0 - a;
                        d[0] = c[0] * a + d[0] * k;
                        d[1] = c[1] * a + d[1] * k;
                        d[2] = c[2] * a + d[2] * k;
                        d[3] = a + d[3] * k;
                    }
                }
            }
        }
    }
}
