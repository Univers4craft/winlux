//! Assembleur de shaders D3D9 (vs_1_1 … vs_3_0, ps_1_1 … ps_3_0) pour
//! D3DXAssembleShader (#834).
//!
//! Écrit d'après la documentation publique du format de jetons (« Shader
//! Codes » de la référence Direct3D 9) et de la syntaxe des instructions
//! assembleur. Le résultat est le même flux de jetons que celui que
//! `d3d9_shader` traduit en GLSL.

use std::collections::HashMap;

const VS: u32 = 0xFFFE_0000;
const PS: u32 = 0xFFFF_0000;
const END: u32 = 0x0000_FFFF;

// Types de registres (D3DSHADER_PARAM_REGISTER_TYPE).
const R_TEMP: u32 = 0;
const R_INPUT: u32 = 1;
const R_CONST: u32 = 2;
const R_ADDR_TEXTURE: u32 = 3;
const R_RASTOUT: u32 = 4;
const R_ATTROUT: u32 = 5;
const R_OUTPUT: u32 = 6;
const R_CONSTINT: u32 = 7;
const R_COLOROUT: u32 = 8;
const R_DEPTHOUT: u32 = 9;
const R_SAMPLER: u32 = 10;
const R_CONSTBOOL: u32 = 14;
const R_LOOP: u32 = 15;
const R_MISCTYPE: u32 = 17;
const R_LABEL: u32 = 18;
const R_PREDICATE: u32 = 19;

/// Version du shader en cours d'assemblage.
#[derive(Clone, Copy)]
struct Ver {
    pixel: bool,
    major: u32,
    minor: u32,
}

impl Ver {
    fn token(&self) -> u32 {
        (if self.pixel { PS } else { VS }) | (self.major << 8) | self.minor
    }
    fn at_least(&self, major: u32, minor: u32) -> bool {
        (self.major, self.minor) >= (major, minor)
    }
}

fn parse_version(s: &str) -> Option<Ver> {
    let s = s.to_ascii_lowercase().replace('.', "_");
    let (kind, rest) = (s.get(..2)?, s.get(2..)?);
    let pixel = match kind {
        "vs" => false,
        "ps" => true,
        _ => return None,
    };
    let rest = rest.strip_prefix('_')?;
    let (maj, min) = rest.split_once('_')?;
    let major: u32 = maj.parse().ok()?;
    let minor = match min {
        "x" => 1,
        "sw" => 0xFF,
        m => m.parse().ok()?,
    };
    let ok = match (pixel, major) {
        (false, 1) => minor <= 1,
        (true, 1) => (1..=4).contains(&minor),
        (_, 2) => minor <= 1 || minor == 0xFF,
        (_, 3) => minor == 0 || minor == 0xFF,
        _ => false,
    };
    ok.then_some(Ver {
        pixel,
        major,
        minor,
    })
}

/// Nombre d'opérandes source attendus (None = variable selon la version).
struct Op {
    code: u32,
    dest: bool,
    srcs: Option<usize>,
}

fn opcode(name: &str) -> Option<Op> {
    let o = |code, dest, n: usize| {
        Some(Op {
            code,
            dest,
            srcs: Some(n),
        })
    };
    let v = |code, dest| {
        Some(Op {
            code,
            dest,
            srcs: None,
        })
    };
    match name {
        "nop" => o(0, false, 0),
        "mov" => o(1, true, 1),
        "add" => o(2, true, 2),
        "sub" => o(3, true, 2),
        "mad" => o(4, true, 3),
        "mul" => o(5, true, 2),
        "rcp" => o(6, true, 1),
        "rsq" => o(7, true, 1),
        "dp3" => o(8, true, 2),
        "dp4" => o(9, true, 2),
        "min" => o(10, true, 2),
        "max" => o(11, true, 2),
        "slt" => o(12, true, 2),
        "sge" => o(13, true, 2),
        "exp" => o(14, true, 1),
        "log" => o(15, true, 1),
        "lit" => o(16, true, 1),
        "dst" => o(17, true, 2),
        "lrp" => o(18, true, 3),
        "frc" => o(19, true, 1),
        "m4x4" => o(20, true, 2),
        "m4x3" => o(21, true, 2),
        "m3x4" => o(22, true, 2),
        "m3x3" => o(23, true, 2),
        "m3x2" => o(24, true, 2),
        "call" => o(25, false, 1),
        "callnz" => o(26, false, 2),
        "loop" => o(27, false, 2),
        "ret" => o(28, false, 0),
        "endloop" => o(29, false, 0),
        "label" => o(30, false, 1),
        "pow" => o(32, true, 2),
        "crs" => o(33, true, 2),
        "sgn" => o(34, true, 3),
        "abs" => o(35, true, 1),
        "nrm" => o(36, true, 1),
        "sincos" => v(37, true),
        "rep" => o(38, false, 1),
        "endrep" => o(39, false, 0),
        "if" => o(40, false, 1),
        "else" => o(42, false, 0),
        "endif" => o(43, false, 0),
        "break" => o(44, false, 0),
        "mova" => o(46, true, 1),
        "texcoord" => o(64, true, 0),
        "texcrd" => o(64, true, 1),
        "texkill" => o(65, true, 0),
        "tex" => o(66, true, 0),
        "texbem" => o(67, true, 1),
        "texbeml" => o(68, true, 1),
        "texreg2ar" => o(69, true, 1),
        "texreg2gb" => o(70, true, 1),
        "texm3x2pad" => o(71, true, 1),
        "texm3x2tex" => o(72, true, 1),
        "texm3x3pad" => o(73, true, 1),
        "texm3x3tex" => o(74, true, 1),
        "texm3x3spec" => o(76, true, 2),
        "texm3x3vspec" => o(77, true, 1),
        "expp" => o(78, true, 1),
        "logp" => o(79, true, 1),
        "cnd" => o(80, true, 3),
        "texreg2rgb" => o(82, true, 1),
        "texdp3tex" => o(83, true, 1),
        "texm3x2depth" => o(84, true, 1),
        "texdp3" => o(85, true, 1),
        "texm3x3" => o(86, true, 1),
        "texdepth" => o(87, true, 0),
        "cmp" => o(88, true, 3),
        "bem" => o(89, true, 2),
        "dp2add" => o(90, true, 3),
        "dsx" => o(91, true, 1),
        "dsy" => o(92, true, 1),
        "texldd" => o(93, true, 4),
        "setp" => o(94, true, 2),
        "texldl" => o(95, true, 2),
        "breakp" => o(96, false, 1),
        _ => None,
    }
}

fn compare(s: &str) -> Option<u32> {
    Some(match s {
        "gt" => 1,
        "eq" => 2,
        "ge" => 3,
        "lt" => 4,
        "ne" => 5,
        "le" => 6,
        _ => return None,
    })
}

fn usage(s: &str) -> Option<u32> {
    Some(match s {
        "position" => 0,
        "blendweight" => 1,
        "blendindices" => 2,
        "normal" => 3,
        "psize" => 4,
        "texcoord" => 5,
        "tangent" => 6,
        "binormal" => 7,
        "tessfactor" => 8,
        "positiont" => 9,
        "color" => 10,
        "fog" => 11,
        "depth" => 12,
        "sample" => 13,
        _ => return None,
    })
}

/// Registre lu : type, numéro, et adressage relatif éventuel.
struct Reg {
    ty: u32,
    num: u32,
    /// (type, numéro, composante) du registre d'adresse.
    rel: Option<(u32, u32, u32)>,
}

fn comp(c: char) -> Option<u32> {
    Some(match c {
        'x' | 'r' => 0,
        'y' | 'g' => 1,
        'z' | 'b' => 2,
        'w' | 'a' => 3,
        _ => return None,
    })
}

fn split_num(s: &str) -> (&str, Option<u32>) {
    let i = s
        .char_indices()
        .find(|(_, c)| c.is_ascii_digit())
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    (&s[..i], s[i..].parse().ok())
}

/// Registre d'adresse « a0.x » ou « aL ».
fn addr_reg(s: &str) -> Result<(u32, u32, u32), String> {
    let s = s.trim();
    let low = s.to_ascii_lowercase();
    if low == "al" {
        return Ok((R_LOOP, 0, 0));
    }
    let (name, sw) = low.split_once('.').unwrap_or((&low, "x"));
    let (p, n) = split_num(name);
    if p != "a" {
        return Err(format!("registre d'adresse attendu : {s}"));
    }
    let c = sw
        .chars()
        .next()
        .and_then(comp)
        .ok_or_else(|| format!("composante invalide : {s}"))?;
    Ok((R_ADDR_TEXTURE, n.unwrap_or(0), c))
}

fn parse_reg(s: &str, ver: Ver) -> Result<Reg, String> {
    let s = s.trim();
    // Partie entre crochets : c[a0.x + 5], c5[a0.x], v[aL].
    let (base, bracket) = match s.find('[') {
        Some(i) => {
            let j = s
                .rfind(']')
                .ok_or_else(|| format!("crochet non fermé : {s}"))?;
            (&s[..i], Some(&s[i + 1..j]))
        }
        None => (s, None),
    };
    let low = base.to_ascii_lowercase();
    let (prefix, num) = split_num(&low);
    let (ty, mut n) = match prefix {
        "r" => (R_TEMP, num),
        "v" => (R_INPUT, num),
        "c" => (R_CONST, num),
        "a" if !ver.pixel => (R_ADDR_TEXTURE, num),
        "t" if ver.pixel => (R_ADDR_TEXTURE, num),
        "opos" => (R_RASTOUT, Some(0)),
        "ofog" => (R_RASTOUT, Some(1)),
        "opts" => (R_RASTOUT, Some(2)),
        "od" => (R_ATTROUT, num),
        "ot" => (R_OUTPUT, num),
        "o" => (R_OUTPUT, num),
        "i" => (R_CONSTINT, num),
        "oc" => (R_COLOROUT, num),
        "odepth" => (R_DEPTHOUT, Some(0)),
        "s" => (R_SAMPLER, num),
        "b" => (R_CONSTBOOL, num),
        "al" => (R_LOOP, Some(0)),
        "vpos" => (R_MISCTYPE, Some(0)),
        "vface" => (R_MISCTYPE, Some(1)),
        "l" => (R_LABEL, num),
        "p" => (R_PREDICATE, num),
        _ => return Err(format!("registre inconnu : {s}")),
    };
    let mut rel = None;
    if let Some(b) = bracket {
        // Somme de termes : un registre d'adresse et des entiers.
        for term in b.split('+') {
            let t = term.trim();
            if t.is_empty() {
                continue;
            }
            if let Ok(v) = t.parse::<u32>() {
                n = Some(n.unwrap_or(0) + v);
            } else {
                rel = Some(addr_reg(t)?);
            }
        }
    }
    let num = n.ok_or_else(|| format!("numéro de registre manquant : {s}"))?;
    if num > 2047 {
        return Err(format!("numéro de registre trop grand : {s}"));
    }
    Ok(Reg { ty, num, rel })
}

fn reg_bits(ty: u32, num: u32) -> u32 {
    0x8000_0000 | (num & 0x7FF) | ((ty & 7) << 28) | (((ty >> 3) & 3) << 11)
}

/// Jetons d'un opérande source (jeton relatif compris en SM 2+).
fn src_tokens(s: &str, ver: Ver) -> Result<Vec<u32>, String> {
    let mut s = s.trim().to_string();
    let mut neg = false;
    let mut not = false;
    let mut complement = false;
    if let Some(r) = s.strip_prefix('-') {
        neg = true;
        s = r.trim().to_string();
    }
    if let Some(r) = s.strip_prefix('!') {
        not = true;
        s = r.trim().to_string();
    }
    if let Some(r) = s.strip_prefix("1-") {
        complement = true;
        s = r.trim().to_string();
    } else if let Some(r) = s.strip_prefix("1 -") {
        complement = true;
        s = r.trim().to_string();
    }
    // Suffixes : modificateur (_bx2…) et motif (.xyzw), dans n'importe quel ordre.
    let mut reg_part = s.clone();
    let mut swizzle = 0xE4u32;
    let mut modifier = None::<&str>;
    let after_bracket = s.rfind(']').map(|i| i + 1).unwrap_or(0);
    let tail = &s[after_bracket..];
    let cut = tail
        .find(['.', '_'])
        .map(|i| i + after_bracket)
        .unwrap_or(s.len());
    // « vPos », « vFace », « oPos » n'ont pas de suffixe à couper ici.
    if cut < s.len() {
        reg_part = s[..cut].to_string();
        let mut rest = &s[cut..];
        while !rest.is_empty() {
            let (kind, body) = rest.split_at(1);
            let end = body.find(['.', '_']).unwrap_or(body.len());
            let word = body[..end].to_ascii_lowercase();
            rest = &body[end..];
            if kind == "." {
                let comps: Vec<u32> = word.chars().filter_map(comp).collect();
                if comps.is_empty() || comps.len() != word.len() || comps.len() > 4 {
                    return Err(format!("motif invalide : {s}"));
                }
                let mut c = [0u32; 4];
                for (i, slot) in c.iter_mut().enumerate() {
                    *slot = comps[i.min(comps.len() - 1)];
                }
                swizzle = c[0] | (c[1] << 2) | (c[2] << 4) | (c[3] << 6);
            } else {
                modifier = Some(match word.as_str() {
                    "bias" => "bias",
                    "bx2" => "bx2",
                    "x2" => "x2",
                    "dz" | "db" => "dz",
                    "dw" | "da" => "dw",
                    "abs" => "abs",
                    _ => return Err(format!("modificateur de source inconnu : {s}")),
                });
            }
        }
    }
    let reg = parse_reg(&reg_part, ver)?;
    let m = match (modifier, neg) {
        (_, _) if not => 13,
        (_, _) if complement => 6,
        (None, false) => 0,
        (None, true) => 1,
        (Some("bias"), false) => 2,
        (Some("bias"), true) => 3,
        (Some("bx2"), false) => 4,
        (Some("bx2"), true) => 5,
        (Some("x2"), false) => 7,
        (Some("x2"), true) => 8,
        (Some("dz"), _) => 9,
        (Some("dw"), _) => 10,
        (Some("abs"), false) => 11,
        (Some("abs"), true) => 12,
        _ => 0,
    };
    let mut tok = reg_bits(reg.ty, reg.num) | (swizzle << 16) | (m << 24);
    let mut out = Vec::with_capacity(2);
    match reg.rel {
        Some((aty, anum, c)) => {
            tok |= 1 << 13;
            out.push(tok);
            if ver.major >= 2 {
                let rep = c | (c << 2) | (c << 4) | (c << 6);
                out.push(reg_bits(aty, anum) | (rep << 16));
            }
        }
        None => out.push(tok),
    }
    Ok(out)
}

/// Modificateurs du mnémonique repris par la destination.
#[derive(Default, Clone, Copy)]
struct DestMods {
    sat: bool,
    pp: bool,
    centroid: bool,
    shift: u32,
}

fn dest_tokens(s: &str, mods: DestMods, ver: Ver) -> Result<Vec<u32>, String> {
    let s = s.trim();
    let (reg_s, mask_s) = match s.rfind('.') {
        Some(i) if s[i..].find(']').is_none() => (&s[..i], Some(&s[i + 1..])),
        _ => (s, None),
    };
    let reg = parse_reg(reg_s, ver)?;
    let mask = match mask_s {
        None => 0xF,
        Some(m) => {
            let mut v = 0;
            for c in m.chars() {
                let i = comp(c.to_ascii_lowercase())
                    .ok_or_else(|| format!("masque d'écriture invalide : {s}"))?;
                v |= 1 << i;
            }
            v
        }
    };
    let rm = (mods.sat as u32) | ((mods.pp as u32) << 1) | ((mods.centroid as u32) << 2);
    let mut tok = reg_bits(reg.ty, reg.num) | (mask << 16) | (rm << 20) | (mods.shift << 24);
    let mut out = Vec::with_capacity(2);
    match reg.rel {
        Some((aty, anum, c)) => {
            tok |= 1 << 13;
            out.push(tok);
            if ver.major >= 2 {
                let rep = c | (c << 2) | (c << 4) | (c << 6);
                out.push(reg_bits(aty, anum) | (rep << 16));
            }
        }
        None => out.push(tok),
    }
    Ok(out)
}

/// Retire commentaires et applique les #define simples.
fn preprocess(src: &str, defines: &[(String, String)]) -> Result<Vec<(usize, String)>, String> {
    let mut macros: HashMap<String, String> = defines.iter().cloned().collect();
    let mut out = Vec::new();
    let mut in_block = false;
    for (i, raw) in src.lines().enumerate() {
        let mut line = String::new();
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if in_block {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_block = false;
                }
                continue;
            }
            match c {
                '/' if chars.peek() == Some(&'/') => break,
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    in_block = true;
                }
                ';' => break,
                _ => line.push(c),
            }
        }
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(d) = t.strip_prefix('#') {
            let d = d.trim();
            if let Some(rest) = d.strip_prefix("define") {
                let rest = rest.trim();
                let (k, v) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                macros.insert(k.to_string(), v.trim().to_string());
                continue;
            }
            if d.starts_with("line") || d.starts_with("pragma") || d.starts_with("undef") {
                continue;
            }
            return Err(format!(
                "({}) : directive non prise en charge : #{d}",
                i + 1
            ));
        }
        let mut text = t.to_string();
        if !macros.is_empty() {
            let mut r = String::new();
            let mut word = String::new();
            for c in text.chars().chain(std::iter::once('\0')) {
                if c.is_ascii_alphanumeric() || c == '_' {
                    word.push(c);
                    continue;
                }
                if !word.is_empty() {
                    r.push_str(macros.get(&word).map(String::as_str).unwrap_or(&word));
                    word.clear();
                }
                if c != '\0' {
                    r.push(c);
                }
            }
            text = r;
        }
        out.push((i + 1, text));
    }
    Ok(out)
}

fn parse_float(s: &str) -> Result<f32, String> {
    let t = s.trim().trim_end_matches(['f', 'F']);
    t.parse::<f32>()
        .map_err(|_| format!("nombre invalide : {}", s.trim()))
}

/// Assemble un shader : jetons D3D9 terminés par END.
pub(crate) fn assemble(src: &str, defines: &[(String, String)]) -> Result<Vec<u32>, String> {
    let lines = preprocess(src, defines)?;
    let mut it = lines.into_iter();
    let (vline, vtext) = it.next().ok_or("shader vide")?;
    let ver = parse_version(vtext.split_whitespace().next().unwrap_or(""))
        .ok_or_else(|| format!("({vline}) : version de shader attendue (vs_2_0, ps_1_4…)"))?;
    let mut out = vec![ver.token()];
    // La version peut être suivie d'une instruction sur la même ligne.
    let rest = vtext.split_once(char::is_whitespace).map(|x| x.1.trim());
    let mut queue: Vec<(usize, String)> = Vec::new();
    if let Some(r) = rest.filter(|r| !r.is_empty()) {
        queue.push((vline, r.to_string()));
    }
    queue.extend(it);
    for (line, text) in queue {
        instruction(&text, ver, &mut out).map_err(|e| format!("({line}) : {e}"))?;
    }
    out.push(END);
    Ok(out)
}

fn split_operands(s: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                v.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        v.push(cur.trim().to_string());
    }
    v
}

fn instruction(text: &str, ver: Ver, out: &mut Vec<u32>) -> Result<(), String> {
    let mut t = text.trim();
    let mut coissue = false;
    if let Some(r) = t.strip_prefix('+') {
        coissue = true;
        t = r.trim_start();
    }
    let mut predicate: Option<Vec<u32>> = None;
    if let Some(r) = t.strip_prefix('(') {
        let end = r.find(')').ok_or("prédicat non fermé")?;
        predicate = Some(src_tokens(&r[..end], ver)?);
        t = r[end + 1..].trim_start();
    }
    let (mn, args) = t
        .split_once(char::is_whitespace)
        .map(|(a, b)| (a, b.trim()))
        .unwrap_or((t, ""));
    let mn = mn.to_ascii_lowercase();
    let ops = split_operands(args);

    if mn == "phase" {
        out.push(0xFFFD);
        return Ok(());
    }
    if mn.starts_with("dcl") {
        return dcl(&mn, &ops, ver, out);
    }
    if mn == "def" || mn == "defi" || mn == "defb" {
        return def(&mn, &ops, ver, out);
    }

    // Mnémonique et suffixes.
    let mut parts: Vec<&str> = mn.split('_').collect();
    let mut mods = DestMods::default();
    let mut ctrl = 0u32;
    while parts.len() > 1 {
        let last = *parts.last().unwrap_or(&"");
        let used = match last {
            "sat" => {
                mods.sat = true;
                true
            }
            "pp" => {
                mods.pp = true;
                true
            }
            "centroid" => {
                mods.centroid = true;
                true
            }
            "x2" => {
                mods.shift = 1;
                true
            }
            "x4" => {
                mods.shift = 2;
                true
            }
            "x8" => {
                mods.shift = 3;
                true
            }
            "d2" => {
                mods.shift = 0xF;
                true
            }
            "d4" => {
                mods.shift = 0xE;
                true
            }
            "d8" => {
                mods.shift = 0xD;
                true
            }
            _ => false,
        };
        if !used {
            break;
        }
        parts.pop();
    }
    let mut base = parts.join("_");
    if let Some((b, c)) = base.split_once('_') {
        if let Some(v) = compare(c) {
            ctrl = v;
            base = match b {
                "if" => "ifc".into(),
                "break" => "breakc".into(),
                "setp" => "setp".into(),
                _ => return Err(format!("instruction inconnue : {mn}")),
            };
        }
    }
    // Instructions de texture selon la version.
    let (code, dest, srcs) = match base.as_str() {
        "ifc" => (41, false, Some(2)),
        "breakc" => (45, false, Some(2)),
        "texld" | "texldp" | "texldb" => {
            if base == "texldp" {
                ctrl = 1;
            } else if base == "texldb" {
                ctrl = 2;
            }
            if !ver.pixel && ver.major < 3 {
                return Err("texld n'existe que dans un pixel shader".into());
            }
            (66, true, Some(if ver.major >= 2 { 2 } else { 1 }))
        }
        "texcoord" if ver.pixel && ver.at_least(1, 4) => {
            return Err("ps_1_4 : utiliser texcrd".into());
        }
        _ => {
            let op = opcode(&base).ok_or_else(|| format!("instruction inconnue : {mn}"))?;
            (op.code, op.dest, op.srcs)
        }
    };
    if let Some(n) = srcs {
        let want = n + dest as usize;
        if ops.len() != want {
            return Err(format!(
                "{mn} : {want} opérande(s) attendu(s), {} donné(s)",
                ops.len()
            ));
        }
    }
    let mut body = Vec::new();
    let mut operands = ops.iter();
    if dest {
        let d = operands
            .next()
            .ok_or_else(|| format!("{mn} : destination manquante"))?;
        body.extend(dest_tokens(d, mods, ver)?);
    }
    if let Some(p) = &predicate {
        body.extend(p.iter().copied());
    }
    for s in operands {
        body.extend(src_tokens(s, ver)?);
    }
    let len = if ver.major >= 2 { body.len() as u32 } else { 0 };
    out.push(
        code | (ctrl << 16)
            | (len << 24)
            | ((predicate.is_some() as u32) << 28)
            | ((coissue as u32) << 30),
    );
    out.extend(body);
    Ok(())
}

fn dcl(mn: &str, ops: &[String], ver: Ver, out: &mut Vec<u32>) -> Result<(), String> {
    if ops.len() != 1 {
        return Err(format!("{mn} : un registre attendu"));
    }
    let mut mods = DestMods::default();
    let mut words: Vec<&str> = mn.split('_').skip(1).collect();
    words.retain(|w| match *w {
        "pp" => {
            mods.pp = true;
            false
        }
        "centroid" => {
            mods.centroid = true;
            false
        }
        _ => true,
    });
    let usage_tok = match words.first().copied() {
        None => 0x8000_0000,
        Some("2d") => 0x8000_0000 | (2 << 27),
        Some("cube") => 0x8000_0000 | (3 << 27),
        Some("volume") => 0x8000_0000 | (4 << 27),
        Some(w) => {
            let (name, idx) = split_num(w);
            let u = usage(name).ok_or_else(|| format!("usage inconnu : {mn}"))?;
            0x8000_0000 | u | (idx.unwrap_or(0) << 16)
        }
    };
    let d = dest_tokens(&ops[0], mods, ver)?;
    let len = if ver.major >= 2 {
        1 + d.len() as u32
    } else {
        0
    };
    out.push(31 | (len << 24));
    out.push(usage_tok);
    out.extend(d);
    Ok(())
}

fn def(mn: &str, ops: &[String], ver: Ver, out: &mut Vec<u32>) -> Result<(), String> {
    let (code, n) = match mn {
        "def" => (81, 4),
        "defi" => (48, 4),
        _ => (47, 1),
    };
    if ops.len() != n + 1 {
        return Err(format!("{mn} : {} valeurs attendues", n));
    }
    let d = dest_tokens(&ops[0], DestMods::default(), ver)?;
    let mut vals = Vec::with_capacity(n);
    for v in &ops[1..] {
        vals.push(match code {
            81 => parse_float(v)?.to_bits(),
            48 => v
                .trim()
                .parse::<i32>()
                .map_err(|_| format!("entier invalide : {v}"))? as u32,
            _ => match v.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => 1,
                "false" | "0" => 0,
                _ => return Err(format!("booléen invalide : {v}")),
            },
        });
    }
    let len = if ver.major >= 2 {
        (d.len() + vals.len()) as u32
    } else {
        0
    };
    out.push(code | (len << 24));
    out.extend(d);
    out.extend(vals);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// Table des constantes (commentaire CTAB du bytecode)
// ═══════════════════════════════════════════════════════════════════════

/// Une constante décrite par la table CTAB.
#[derive(Clone, Debug, Default)]
pub(crate) struct Constant {
    pub name: String,
    /// 0 bool, 1 int4, 2 float4, 3 sampler.
    pub set: u16,
    pub index: u16,
    pub count: u16,
    pub class: u16,
    pub ty: u16,
    pub rows: u16,
    pub columns: u16,
    pub elements: u16,
    pub struct_members: u16,
    pub bytes: u32,
    pub default: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ConstantTable {
    pub creator: String,
    pub version: u32,
    pub constants: Vec<Constant>,
    /// Octets bruts de la table (après « CTAB »).
    pub raw: Vec<u8>,
}

fn rd32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn rd16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn cstr_at(b: &[u8], at: usize) -> String {
    let s = b.get(at..).unwrap_or(&[]);
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..end]).into_owned()
}

/// Taille en octets d'un type (D3DXSHADER_TYPEINFO).
fn type_bytes(b: &[u8], at: usize) -> u32 {
    let (Some(rows), Some(cols), Some(elems)) = (rd16(b, at + 4), rd16(b, at + 6), rd16(b, at + 8))
    else {
        return 0;
    };
    rows as u32 * cols as u32 * 4 * elems.max(1) as u32
}

/// Découpe un shader en (début, longueur) : instructions et commentaires,
/// sans le jeton de version ni END.
pub(crate) fn walk(tokens: &[u32]) -> Vec<(usize, usize)> {
    let sm2 = tokens.first().is_some_and(|v| v & 0xFF00 >= 0x0200);
    let mut v = Vec::new();
    let mut i = 1;
    while i < tokens.len() {
        let t = tokens[i];
        if t == END {
            break;
        }
        let n = if t & 0xFFFF == 0xFFFE {
            1 + ((t >> 16) & 0x7FFF) as usize
        } else if sm2 {
            1 + ((t >> 24) & 0xF) as usize
        } else if t & 0xFFFF == 81 {
            6 // def : destination et quatre flottants (bit 31 quelconque)
        } else {
            1 + tokens[i + 1..]
                .iter()
                .take_while(|&&p| p & 0x8000_0000 != 0 && p != END)
                .count()
        };
        let n = n.min(tokens.len() - i);
        v.push((i, n));
        i += n;
    }
    v
}

/// Trouve et lit la table CTAB d'un shader.
pub(crate) fn constant_table(tokens: &[u32]) -> Option<ConstantTable> {
    for (i, n) in walk(tokens) {
        let t = tokens[i];
        if t & 0xFFFF == 0xFFFE && n >= 2 && tokens[i + 1] == u32::from_le_bytes(*b"CTAB") {
            let raw: Vec<u8> = tokens[i + 2..i + n]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect();
            return parse_ctab(&raw);
        }
    }
    None
}

fn parse_ctab(b: &[u8]) -> Option<ConstantTable> {
    let size = rd32(b, 0)? as usize;
    if size < 28 {
        return None;
    }
    let creator = cstr_at(b, rd32(b, 4)? as usize);
    let version = rd32(b, 8)?;
    let n = rd32(b, 12)? as usize;
    let info = rd32(b, 16)? as usize;
    let mut constants = Vec::with_capacity(n.min(4096));
    for k in 0..n.min(4096) {
        let at = info + k * 20;
        let name = cstr_at(b, rd32(b, at)? as usize);
        let ti = rd32(b, at + 12)? as usize;
        let def = rd32(b, at + 16)? as usize;
        let bytes = type_bytes(b, ti);
        constants.push(Constant {
            name,
            set: rd16(b, at + 4)?,
            index: rd16(b, at + 6)?,
            count: rd16(b, at + 8)?,
            class: rd16(b, ti).unwrap_or(0),
            ty: rd16(b, ti + 2).unwrap_or(0),
            rows: rd16(b, ti + 4).unwrap_or(0),
            columns: rd16(b, ti + 6).unwrap_or(0),
            elements: rd16(b, ti + 8).unwrap_or(1),
            struct_members: rd16(b, ti + 10).unwrap_or(0),
            bytes,
            default: (def != 0)
                .then(|| b.get(def..def + bytes as usize).map(<[u8]>::to_vec))
                .flatten(),
        });
    }
    Some(ConstantTable {
        creator,
        version,
        constants,
        raw: b.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_vs_1_1() {
        let t = assemble(
            "vs.1.1 ; transformation\n\
             dcl_position v0\n\
             dcl_texcoord0 v1\n\
             m4x4 oPos, v0, c0\n\
             mov oT0.xy, v1\n\
             mov oD0, c[a0.x + 4]\n",
            &[],
        )
        .unwrap();
        assert_eq!(t[0], 0xFFFE_0101);
        assert_eq!(t[1], 31); // dcl, sans longueur en 1.x
        assert_eq!(t[2], 0x8000_0000); // usage position 0
        assert_eq!(t[3], 0x900F_0000); // v0, masque xyzw
        assert_eq!(t[5], 0x8000_0005); // texcoord0
        assert_eq!(t[7], 20); // m4x4
        assert_eq!(t[8], 0xC00F_0000); // oPos (rastout 4)
        assert_eq!(*t.last().unwrap(), END);
        // c[a0.x + 4] : relatif sans jeton supplémentaire en vs_1_1.
        let mov = t.iter().rposition(|&x| x == 1).unwrap();
        assert_eq!(t[mov + 2], 0xA0E4_2004);
        let s = crate::dlls::d3d9_shader::translate(&t).unwrap();
        assert!(!s.pixel);
    }

    #[test]
    fn assemble_ps_2_0_et_traduction() {
        let t = assemble(
            "ps_2_0\n\
             def c1, 0.5, 1.0f, -2, 0\n\
             dcl t0.xy\n\
             dcl_2d s0\n\
             texld r0, t0, s0\n\
             mad_sat r0.rgb, r0, c0, -c1.x\n\
             mov oC0, r0\n",
            &[],
        )
        .unwrap();
        assert_eq!(t[0], 0xFFFF_0200);
        // def : longueur 5 jetons.
        assert_eq!(t[1], 81 | (5 << 24));
        assert_eq!(f32::from_bits(t[3]), 0.5);
        assert_eq!(f32::from_bits(t[5]), -2.0);
        let texld = t.iter().position(|&x| x & 0xFFFF == 66).unwrap();
        assert_eq!(t[texld] >> 24, 3);
        let mad = t.iter().position(|&x| x & 0xFFFF == 4).unwrap();
        assert_eq!(t[mad + 1] & (1 << 20), 1 << 20); // _sat
        assert_eq!(t[mad + 1] >> 16 & 0xF, 0x7); // .rgb
        assert_eq!(t[mad + 4], 0xA100_0001); // -c1.x : neg, motif xxxx
        let s = crate::dlls::d3d9_shader::translate(&t).unwrap();
        assert!(s.pixel);
    }

    #[test]
    fn assemble_vs_3_0_controle() {
        let t = assemble(
            "vs_3_0\n\
             #define ZERO c0\n\
             dcl_position v0\n\
             dcl_position o0\n\
             dcl_texcoord1 o1.xy\n\
             defi i0, 4, 0, 1, 0\n\
             mov r0, ZERO\n\
             loop aL, i0\n\
               add r0, r0, c10[aL]\n\
             endloop\n\
             if_lt r0.x, c1.y\n\
               mov r0, c2\n\
             endif\n\
             mov o0, v0\n\
             mov o1.xy, r0\n",
            &[],
        )
        .unwrap();
        assert_eq!(t[0], 0xFFFE_0300);
        let ifc = t.iter().position(|&x| x & 0xFFFF == 41).unwrap();
        assert_eq!(((t[ifc] >> 16) & 0xFF, t[ifc] >> 24), (4, 2)); // lt, 2 jetons
        let add = t.iter().position(|&x| x & 0xFFFF == 2).unwrap();
        assert_eq!(t[add] >> 24, 4); // dest, src, src relative + jeton aL
        assert_eq!(t[add + 4], 0xF000_0800); // aL (type 15), motif xxxx
    }

    #[test]
    fn erreurs() {
        assert!(assemble("ps_2_0\nmov oC0", &[]).is_err());
        assert!(assemble("vs_2_0\nfoo r0, r1", &[]).is_err());
        assert!(assemble("mov r0, r1", &[]).is_err());
        let e = assemble("vs_2_0\nmov r0, q1", &[]).unwrap_err();
        assert!(e.starts_with("(2)"), "{e}");
    }

    #[test]
    fn table_ctab() {
        // CTAB : en-tête 28 octets, une constante « gMat » float4x4 en c4.
        let mut b = vec![0u8; 28];
        let w =
            |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
        w(&mut b, 0, 28);
        w(&mut b, 8, 0xFFFE_0200);
        w(&mut b, 12, 1);
        w(&mut b, 16, 28);
        b.resize(28 + 20, 0);
        let name_at = b.len();
        b.extend_from_slice(b"gMat\0\0\0\0");
        let ti = b.len();
        b.extend_from_slice(&[3, 0, 3, 0, 4, 0, 4, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
        w(&mut b, 28, name_at as u32);
        b[32..34].copy_from_slice(&2u16.to_le_bytes());
        b[34..36].copy_from_slice(&4u16.to_le_bytes());
        b[36..38].copy_from_slice(&4u16.to_le_bytes());
        w(&mut b, 40, ti as u32);
        while b.len() % 4 != 0 {
            b.push(0);
        }
        let mut tokens = vec![0xFFFE_0200];
        let words = 1 + b.len() / 4;
        tokens.push(0xFFFE | ((words as u32) << 16));
        tokens.push(u32::from_le_bytes(*b"CTAB"));
        tokens.extend(
            b.chunks(4)
                .map(|c| u32::from_le_bytes(c.try_into().unwrap())),
        );
        tokens.push(END);
        let t = constant_table(&tokens).unwrap();
        assert_eq!(t.constants.len(), 1);
        let c = &t.constants[0];
        assert_eq!(
            (c.name.as_str(), c.set, c.index, c.count),
            ("gMat", 2, 4, 4)
        );
        assert_eq!((c.class, c.rows, c.columns, c.bytes), (3, 4, 4, 64));
    }
}
