//! Traduction des shaders Direct3D 9 (bytecode SM 1.x, 2.x et 3.0) en GLSL.
//!
//! Le bytecode D3D9 est un flux de jetons 32 bits : jeton de version,
//! instructions (opcode, destination, sources), commentaires, `END`. On le
//! décode entièrement et on produit du GLSL 1.20 (profil de compatibilité),
//! ce qui permet de réutiliser les tableaux de sommets « fixed-function »
//! et les variables intégrées (`gl_TexCoord`, `gl_Color`…).
//!
//! Garde-fou : toute construction non prise en charge (prédicats,
//! sous-routines, instructions de texture exotiques de ps_1_x) fait échouer
//! la traduction avec un message ; l'appareil retombe alors sur le pipeline
//! fixe au lieu de compiler un shader faux.
#![allow(dead_code)] // métadonnées de shader gardées pour le débogage

use std::fmt::Write as _;

pub(crate) const USAGE_POSITION: u8 = 0;
pub(crate) const USAGE_BLENDWEIGHT: u8 = 1;
pub(crate) const USAGE_BLENDINDICES: u8 = 2;
pub(crate) const USAGE_NORMAL: u8 = 3;
pub(crate) const USAGE_PSIZE: u8 = 4;
pub(crate) const USAGE_TEXCOORD: u8 = 5;
pub(crate) const USAGE_TANGENT: u8 = 6;
pub(crate) const USAGE_BINORMAL: u8 = 7;
pub(crate) const USAGE_POSITIONT: u8 = 9;
pub(crate) const USAGE_COLOR: u8 = 10;
pub(crate) const USAGE_FOG: u8 = 11;
pub(crate) const USAGE_DEPTH: u8 = 12;

// Types de registres.
const R_TEMP: u32 = 0;
const R_INPUT: u32 = 1;
const R_CONST: u32 = 2;
const R_ADDR_TEXTURE: u32 = 3; // a0 en VS, t# en PS
const R_RASTOUT: u32 = 4;
const R_ATTROUT: u32 = 5;
const R_OUTPUT: u32 = 6; // oT# (vs<3) / o# (vs3)
const R_CONSTINT: u32 = 7;
const R_COLOROUT: u32 = 8;
const R_DEPTHOUT: u32 = 9;
const R_SAMPLER: u32 = 10;
const R_CONST2: u32 = 11;
const R_CONST3: u32 = 12;
const R_CONST4: u32 = 13;
const R_CONSTBOOL: u32 = 14;
const R_LOOP: u32 = 15;
const R_MISCTYPE: u32 = 17;
const R_PREDICATE: u32 = 19;

// Opcodes.
const OP_NOP: u32 = 0;
const OP_MOV: u32 = 1;
const OP_ADD: u32 = 2;
const OP_SUB: u32 = 3;
const OP_MAD: u32 = 4;
const OP_MUL: u32 = 5;
const OP_RCP: u32 = 6;
const OP_RSQ: u32 = 7;
const OP_DP3: u32 = 8;
const OP_DP4: u32 = 9;
const OP_MIN: u32 = 10;
const OP_MAX: u32 = 11;
const OP_SLT: u32 = 12;
const OP_SGE: u32 = 13;
const OP_EXP: u32 = 14;
const OP_LOG: u32 = 15;
const OP_LIT: u32 = 16;
const OP_DST: u32 = 17;
const OP_LRP: u32 = 18;
const OP_FRC: u32 = 19;
const OP_M4X4: u32 = 20;
const OP_M4X3: u32 = 21;
const OP_M3X4: u32 = 22;
const OP_M3X3: u32 = 23;
const OP_M3X2: u32 = 24;
const OP_CALL: u32 = 25;
const OP_CALLNZ: u32 = 26;
const OP_LOOP: u32 = 27;
const OP_RET: u32 = 28;
const OP_ENDLOOP: u32 = 29;
const OP_LABEL: u32 = 30;
const OP_DCL: u32 = 31;
const OP_POW: u32 = 32;
const OP_CRS: u32 = 33;
const OP_SGN: u32 = 34;
const OP_ABS: u32 = 35;
const OP_NRM: u32 = 36;
const OP_SINCOS: u32 = 37;
const OP_REP: u32 = 38;
const OP_ENDREP: u32 = 39;
const OP_IF: u32 = 40;
const OP_IFC: u32 = 41;
const OP_ELSE: u32 = 42;
const OP_ENDIF: u32 = 43;
const OP_BREAK: u32 = 44;
const OP_BREAKC: u32 = 45;
const OP_MOVA: u32 = 46;
const OP_DEFB: u32 = 47;
const OP_DEFI: u32 = 48;
const OP_TEXCOORD: u32 = 64;
const OP_TEXKILL: u32 = 65;
const OP_TEX: u32 = 66;
const OP_EXPP: u32 = 78;
const OP_LOGP: u32 = 79;
const OP_CND: u32 = 80;
const OP_DEF: u32 = 81;
const OP_CMP: u32 = 88;
const OP_DP2ADD: u32 = 90;
const OP_DSX: u32 = 91;
const OP_DSY: u32 = 92;
const OP_TEXLDD: u32 = 93;
const OP_SETP: u32 = 94;
const OP_TEXLDL: u32 = 95;
const OP_BREAKP: u32 = 96;
const OP_PHASE: u32 = 0xfffd;
const OP_COMMENT: u32 = 0xfffe;
const OP_END: u32 = 0xffff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SamplerType {
    Tex2D,
    Cube,
    Volume,
}

/// Entrée de sommet attendue par un vertex shader (usage D3DDECLUSAGE).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VsInput {
    pub usage: u8,
    pub index: u8,
    /// Nom de l'attribut GLSL à relier au flux de sommets.
    pub name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Sm9Shader {
    pub pixel: bool,
    pub major: u32,
    pub minor: u32,
    pub glsl: String,
    pub inputs: Vec<VsInput>,
    pub samplers: Vec<(u32, SamplerType)>,
    pub defs: Vec<(u32, [f32; 4])>,
}

#[derive(Clone, Copy)]
struct Param {
    ty: u32,
    num: u32,
    swizzle: u32,
    srcmod: u32,
    mask: u32,
    resmod: u32,
    shift: u32,
    relative: bool,
    rel: Option<(u32, u32, u32)>, // (type, num, composante) du registre d'adresse
}

fn reg_type(tok: u32) -> u32 {
    ((tok >> 28) & 7) | ((tok >> 8) & 0x18)
}

fn parse_param(tok: u32) -> Param {
    Param {
        ty: reg_type(tok),
        num: tok & 0x7ff,
        swizzle: (tok >> 16) & 0xff,
        srcmod: (tok >> 24) & 0xf,
        mask: (tok >> 16) & 0xf,
        resmod: (tok >> 20) & 0xf,
        shift: (tok >> 24) & 0xf,
        relative: tok & (1 << 13) != 0,
        rel: None,
    }
}

const COMP: [char; 4] = ['x', 'y', 'z', 'w'];

fn swizzle_str(sw: u32) -> String {
    (0..4)
        .map(|i| COMP[((sw >> (i * 2)) & 3) as usize])
        .collect()
}

fn mask_str(mask: u32) -> String {
    (0..4)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| COMP[i])
        .collect()
}

struct Ctx {
    pixel: bool,
    major: u32,
    minor: u32,
    body: String,
    indent: usize,
    temps: u32,
    vs_inputs: Vec<(u32, u8, u8)>, // registre, usage, index
    ps_inputs: Vec<(u32, u8, u8)>,
    vs3_outputs: Vec<(u32, u8, u8)>,
    samplers: Vec<(u32, SamplerType)>,
    defs: Vec<(u32, [f32; 4])>,
    defi: Vec<(u32, [i32; 4])>,
    defb: Vec<(u32, bool)>,
    loop_depth: u32,
    uses_addr: bool,
    uses_texreg: u32,
    tex_regs_written: u32,
    uses_outputs: OutputUse,
    max_loop: u32,
}

#[derive(Default)]
struct OutputUse {
    pos: bool,
    fog: bool,
    psize: bool,
    d: [bool; 2],
    t: [bool; 8],
    c: [bool; 4],
    depth: bool,
}

impl Ctx {
    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.body.push_str("    ");
        }
        self.body.push_str(s);
        self.body.push('\n');
    }

    fn const_prefix(&self) -> &'static str {
        if self.pixel {
            "pc"
        } else {
            "vc"
        }
    }

    /// Nom GLSL d'un registre source/destination (vec4).
    fn reg_name(&mut self, p: &Param, as_dest: bool) -> Result<String, String> {
        let n = p.num;
        Ok(match p.ty {
            R_TEMP => {
                self.temps = self.temps.max(n + 1);
                format!("r{n}")
            }
            R_INPUT => {
                if self.pixel {
                    if self.major >= 3 {
                        match self.ps_inputs.iter().find(|i| i.0 == n) {
                            Some(&(_, USAGE_COLOR, idx)) => {
                                if idx == 0 {
                                    "gl_Color".into()
                                } else {
                                    "gl_SecondaryColor".into()
                                }
                            }
                            Some(&(_, USAGE_TEXCOORD, idx)) => format!("gl_TexCoord[{idx}]"),
                            Some(&(_, USAGE_FOG, _)) => "vec4(gl_FogFragCoord)".into(),
                            _ => format!("gl_TexCoord[{}]", n.min(7)),
                        }
                    } else if n == 0 {
                        "gl_Color".into()
                    } else {
                        "gl_SecondaryColor".into()
                    }
                } else {
                    let rel = if p.relative { "/*rel*/" } else { "" };
                    let _ = rel;
                    match self.vs_inputs.iter().find(|i| i.0 == n) {
                        Some(&(_, u, i)) => attr_name(u, i),
                        None => format!("a_v{n}"),
                    }
                }
            }
            R_CONST | R_CONST2 | R_CONST3 | R_CONST4 => {
                let base = n + match p.ty {
                    R_CONST2 => 2048,
                    R_CONST3 => 4096,
                    R_CONST4 => 6144,
                    _ => 0,
                };
                if let Some((t, rn, c)) = p.rel {
                    self.uses_addr |= t == R_ADDR_TEXTURE;
                    let a = if t == R_LOOP {
                        "aL".to_string()
                    } else {
                        format!("a0.{}", COMP[c as usize])
                    };
                    let _ = rn;
                    format!("{}[{} + {}]", self.const_prefix(), a, base)
                } else if p.relative {
                    // vs_1_1 : adressage relatif implicite par a0.x.
                    self.uses_addr = true;
                    format!("{}[a0.x + {}]", self.const_prefix(), base)
                } else if !as_dest && self.defs.iter().any(|d| d.0 == base) {
                    format!("cd{base}")
                } else {
                    format!("{}[{}]", self.const_prefix(), base)
                }
            }
            R_ADDR_TEXTURE => {
                if self.pixel {
                    self.uses_texreg |= 1 << n.min(7);
                    format!("t{n}")
                } else {
                    self.uses_addr = true;
                    "vec4(a0)".into()
                }
            }
            R_RASTOUT => match n {
                0 => {
                    self.uses_outputs.pos = true;
                    "o_pos".into()
                }
                1 => {
                    self.uses_outputs.fog = true;
                    "o_fog".into()
                }
                _ => {
                    self.uses_outputs.psize = true;
                    "o_psize".into()
                }
            },
            R_ATTROUT => {
                let i = (n as usize).min(1);
                self.uses_outputs.d[i] = true;
                format!("o_d{i}")
            }
            R_OUTPUT => {
                if !self.pixel && self.major >= 3 {
                    format!("o{n}")
                } else {
                    let i = (n as usize).min(7);
                    self.uses_outputs.t[i] = true;
                    format!("o_t{i}")
                }
            }
            R_CONSTINT => {
                if self.defi.iter().any(|d| d.0 == n) {
                    format!("vec4(ci{n})")
                } else {
                    format!("vec4({}i[{n}])", self.const_prefix())
                }
            }
            R_COLOROUT => {
                let i = (n as usize).min(3);
                self.uses_outputs.c[i] = true;
                format!("o_c{i}")
            }
            R_DEPTHOUT => {
                self.uses_outputs.depth = true;
                "o_depth".into()
            }
            R_LOOP => "vec4(float(aL))".into(),
            R_MISCTYPE => match n {
                0 => "gl_FragCoord".into(),
                _ => "vec4(gl_FrontFacing ? 1.0 : -1.0)".into(),
            },
            R_CONSTBOOL => format!("{}b[{n}]", self.const_prefix()),
            R_PREDICATE => return Err("registre de prédicat non pris en charge".into()),
            t => return Err(format!("type de registre {t} non pris en charge")),
        })
    }

    fn src(&mut self, p: &Param) -> Result<String, String> {
        let name = self.reg_name(p, false)?;
        let sw = swizzle_str(p.swizzle);
        let v = if sw == "xyzw" {
            name.to_string()
        } else {
            format!("{name}.{sw}")
        };
        Ok(match p.srcmod {
            0 => v,
            1 => format!("(-{v})"),
            2 => format!("({v} - 0.5)"),
            3 => format!("(-({v} - 0.5))"),
            4 => format!("({v} * 2.0 - 1.0)"),
            5 => format!("(-({v} * 2.0 - 1.0))"),
            6 => format!("(1.0 - {v})"),
            7 => format!("({v} * 2.0)"),
            8 => format!("(-({v} * 2.0))"),
            9 => format!("({v} / {v}.z)"),
            10 => format!("({v} / {v}.w)"),
            11 => format!("abs({v})"),
            12 => format!("(-abs({v}))"),
            13 => format!("(1.0 - {v})"),
            m => return Err(format!("modificateur de source {m} inconnu")),
        })
    }

    /// Composante scalaire (swizzle réplique, sinon `.x` du swizzle).
    fn scalar(&mut self, p: &Param) -> Result<String, String> {
        Ok(format!("({}).x", self.src(p)?))
    }

    fn assign(&mut self, d: &Param, expr: &str) -> Result<(), String> {
        let name = self.reg_name(d, true)?;
        if !self.pixel && d.ty == R_ADDR_TEXTURE {
            self.uses_addr = true;
            let m = mask_str(d.mask);
            let swz: String = m.chars().collect();
            let rounding = if self.major >= 2 {
                "floor(v + 0.5)"
            } else {
                "floor(v)"
            };
            let e = rounding.replace('v', &format!("vec4({expr})"));
            self.line(&format!("a0.{swz} = ivec4({e}).{swz};"));
            return Ok(());
        }
        if self.pixel && d.ty == R_ADDR_TEXTURE {
            self.tex_regs_written |= 1 << d.num.min(7);
        }
        let mut e = format!("vec4({expr})");
        match d.shift {
            0 => {}
            1 => e = format!("({e} * 2.0)"),
            2 => e = format!("({e} * 4.0)"),
            3 => e = format!("({e} * 8.0)"),
            13 => e = format!("({e} * 0.125)"),
            14 => e = format!("({e} * 0.25)"),
            15 => e = format!("({e} * 0.5)"),
            _ => {}
        }
        if d.resmod & 1 != 0 {
            e = format!("clamp({e}, 0.0, 1.0)");
        }
        let m = mask_str(d.mask);
        if m.is_empty() {
            return Ok(());
        }
        if d.ty == R_DEPTHOUT {
            self.line(&format!("{name} = ({e}).x;"));
        } else if m == "xyzw" {
            self.line(&format!("{name} = {e};"));
        } else {
            self.line(&format!("{name}.{m} = ({e}).{m};"));
        }
        Ok(())
    }
}

pub(crate) fn attr_name(usage: u8, index: u8) -> String {
    let u = match usage {
        USAGE_POSITION | USAGE_POSITIONT => "position",
        USAGE_BLENDWEIGHT => "blendweight",
        USAGE_BLENDINDICES => "blendindices",
        USAGE_NORMAL => "normal",
        USAGE_PSIZE => "psize",
        USAGE_TEXCOORD => "texcoord",
        USAGE_TANGENT => "tangent",
        USAGE_BINORMAL => "binormal",
        USAGE_COLOR => "color",
        USAGE_FOG => "fog",
        USAGE_DEPTH => "depth",
        _ => "other",
    };
    format!("a_{u}{index}")
}

/// Nombre de paramètres (jetons) d'une instruction SM 1.x, sans champ longueur.
fn sm1_param_count(op: u32) -> Option<usize> {
    Some(match op {
        OP_NOP | OP_RET | OP_ELSE | OP_ENDIF | OP_ENDLOOP | OP_ENDREP | OP_BREAK => 0,
        OP_MOV | OP_RCP | OP_RSQ | OP_EXP | OP_LOG | OP_LIT | OP_FRC | OP_EXPP | OP_LOGP
        | OP_ABS | OP_NRM | OP_SGN | OP_MOVA => 2,
        OP_ADD | OP_SUB | OP_MUL | OP_DP3 | OP_DP4 | OP_MIN | OP_MAX | OP_SLT | OP_SGE | OP_DST
        | OP_M4X4 | OP_M4X3 | OP_M3X4 | OP_M3X3 | OP_M3X2 | OP_POW | OP_CRS => 3,
        OP_MAD | OP_LRP | OP_CND | OP_CMP | OP_DP2ADD => 4,
        OP_DCL => 2,
        OP_DEF => 5,
        OP_TEXCOORD | OP_TEXKILL => 1,
        OP_TEX => 1,
        _ => return None,
    })
}

/// Traduit un shader D3D9. `tokens` commence par le jeton de version.
pub(crate) fn translate(tokens: &[u32]) -> Result<Sm9Shader, String> {
    let version = *tokens.first().ok_or("shader vide")?;
    let pixel = match version >> 16 {
        0xffff => true,
        0xfffe => false,
        _ => return Err(format!("jeton de version invalide 0x{version:08x}")),
    };
    let major = (version >> 8) & 0xff;
    let minor = version & 0xff;
    if !(1..=3).contains(&major) {
        return Err(format!(
            "modèle de shader {major}.{minor} non pris en charge"
        ));
    }
    let mut c = Ctx {
        pixel,
        major,
        minor,
        body: String::new(),
        indent: 1,
        temps: 0,
        vs_inputs: Vec::new(),
        ps_inputs: Vec::new(),
        vs3_outputs: Vec::new(),
        samplers: Vec::new(),
        defs: Vec::new(),
        defi: Vec::new(),
        defb: Vec::new(),
        loop_depth: 0,
        uses_addr: false,
        uses_texreg: 0,
        tex_regs_written: 0,
        uses_outputs: OutputUse::default(),
        max_loop: 0,
    };

    // Première passe : déclarations et constantes (pour nommer les registres
    // avant de générer le corps).
    let mut instrs: Vec<(u32, u32, Vec<u32>)> = Vec::new(); // (opcode, contrôles, jetons)
    let mut i = 1;
    while i < tokens.len() {
        let tok = tokens[i];
        let op = tok & 0xffff;
        if op == OP_END {
            break;
        }
        if op == OP_COMMENT {
            i += 1 + ((tok >> 16) & 0x7fff) as usize;
            continue;
        }
        if op == OP_PHASE {
            i += 1;
            continue;
        }
        let len = if major >= 2 {
            ((tok >> 24) & 0xf) as usize
        } else {
            sm1_param_count(op)
                .ok_or_else(|| format!("instruction ps/vs 1.x {op} non prise en charge"))?
        };
        if tok & (1 << 28) != 0 {
            return Err("instructions prédiquées non prises en charge".into());
        }
        let end = i + 1 + len;
        if end > tokens.len() {
            return Err("bytecode tronqué".into());
        }
        instrs.push((op, (tok >> 16) & 0xff, tokens[i + 1..end].to_vec()));
        i = end;
    }

    for (op, _, args) in &instrs {
        match *op {
            OP_DCL if args.len() >= 2 => {
                let usage_tok = args[0];
                let p = parse_param(args[1]);
                let usage = (usage_tok & 0x1f) as u8;
                let index = ((usage_tok >> 16) & 0xf) as u8;
                match p.ty {
                    R_SAMPLER => {
                        let ty = match (usage_tok >> 27) & 0xf {
                            3 => SamplerType::Cube,
                            4 => SamplerType::Volume,
                            _ => SamplerType::Tex2D,
                        };
                        c.samplers.push((p.num, ty));
                    }
                    R_INPUT if !pixel => c.vs_inputs.push((p.num, usage, index)),
                    R_INPUT => c.ps_inputs.push((p.num, usage, index)),
                    R_OUTPUT if !pixel && major >= 3 => c.vs3_outputs.push((p.num, usage, index)),
                    _ => {}
                }
            }
            OP_DEF if args.len() >= 5 => {
                let p = parse_param(args[0]);
                let base = p.num
                    + match p.ty {
                        R_CONST2 => 2048,
                        R_CONST3 => 4096,
                        R_CONST4 => 6144,
                        _ => 0,
                    };
                c.defs.push((
                    base,
                    [
                        f32::from_bits(args[1]),
                        f32::from_bits(args[2]),
                        f32::from_bits(args[3]),
                        f32::from_bits(args[4]),
                    ],
                ));
            }
            OP_DEFI if args.len() >= 5 => {
                let p = parse_param(args[0]);
                c.defi.push((
                    p.num,
                    [
                        args[1] as i32,
                        args[2] as i32,
                        args[3] as i32,
                        args[4] as i32,
                    ],
                ));
            }
            OP_DEFB if args.len() >= 2 => {
                let p = parse_param(args[0]);
                c.defb.push((p.num, args[1] != 0));
            }
            _ => {}
        }
    }

    // ps_1_x : t# déclarés implicitement, échantillonneur n ↔ t n.
    if pixel && major == 1 {
        for (op, _, args) in &instrs {
            if *op == OP_TEX && !args.is_empty() {
                let p = parse_param(args[0]);
                let n = p.num;
                if !c.samplers.iter().any(|s| s.0 == n) {
                    c.samplers.push((n, SamplerType::Tex2D));
                }
            }
        }
    }

    // Deuxième passe : corps.
    for (op, ctrl, args) in &instrs {
        emit(&mut c, *op, *ctrl, args)?;
    }
    if c.loop_depth != 0 {
        return Err("boucles mal imbriquées".into());
    }

    Ok(finish(c))
}

/// Décode destination + sources, avec jeton d'adresse relative (SM2+).
fn params(c: &Ctx, args: &[u32]) -> Vec<Param> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let mut p = parse_param(args[i]);
        i += 1;
        if p.relative && c.major >= 2 && i < args.len() {
            let a = parse_param(args[i]);
            // composante = premier sélecteur du swizzle du registre d'adresse
            p.rel = Some((a.ty, a.num, a.swizzle & 3));
            i += 1;
        }
        out.push(p);
    }
    out
}

fn cmp_op(ctrl: u32) -> Result<&'static str, String> {
    Ok(match ctrl & 7 {
        1 => ">",
        2 => "==",
        3 => ">=",
        4 => "<",
        5 => "!=",
        6 => "<=",
        x => return Err(format!("comparaison {x} invalide")),
    })
}

fn emit(c: &mut Ctx, op: u32, ctrl: u32, args: &[u32]) -> Result<(), String> {
    match op {
        OP_NOP | OP_DCL | OP_DEF | OP_DEFI | OP_DEFB => return Ok(()),
        OP_CALL | OP_CALLNZ | OP_LABEL | OP_RET => {
            return if op == OP_RET && c.loop_depth == 0 && c.indent == 1 {
                Ok(()) // `ret` final du programme principal
            } else {
                Err("sous-routines (call/label) non prises en charge".into())
            };
        }
        OP_SETP | OP_BREAKP => return Err("prédicats non pris en charge".into()),
        _ => {}
    }
    let p = params(c, args);
    let need = |n: usize| -> Result<(), String> {
        if p.len() < n {
            Err(format!("opcode {op} : {n} paramètres attendus"))
        } else {
            Ok(())
        }
    };
    match op {
        OP_MOV | OP_MOVA => {
            need(2)?;
            let s = c.src(&p[1])?;
            c.assign(&p[0], &s)?;
        }
        OP_ADD | OP_SUB | OP_MUL => {
            need(3)?;
            let (a, b) = (c.src(&p[1])?, c.src(&p[2])?);
            let o = match op {
                OP_ADD => "+",
                OP_SUB => "-",
                _ => "*",
            };
            c.assign(&p[0], &format!("{a} {o} {b}"))?;
        }
        OP_MAD => {
            need(4)?;
            let (a, b, d) = (c.src(&p[1])?, c.src(&p[2])?, c.src(&p[3])?);
            c.assign(&p[0], &format!("{a} * {b} + {d}"))?;
        }
        OP_RCP | OP_RSQ | OP_EXP | OP_LOG | OP_EXPP | OP_LOGP => {
            need(2)?;
            let s = c.scalar(&p[1])?;
            let e = match op {
                OP_RCP => format!("({s} == 0.0 ? 3.4e38 : 1.0 / {s})"),
                OP_RSQ => format!("({s} == 0.0 ? 3.4e38 : inversesqrt(abs({s})))"),
                OP_EXP | OP_EXPP => format!("exp2({s})"),
                _ => format!("({s} == 0.0 ? -3.4e38 : log2(abs({s})))"),
            };
            c.assign(&p[0], &e)?;
        }
        OP_DP3 | OP_DP4 => {
            need(3)?;
            let (a, b) = (c.src(&p[1])?, c.src(&p[2])?);
            let e = if op == OP_DP3 {
                format!("dot(({a}).xyz, ({b}).xyz)")
            } else {
                format!("dot({a}, {b})")
            };
            c.assign(&p[0], &e)?;
        }
        OP_MIN | OP_MAX => {
            need(3)?;
            let (a, b) = (c.src(&p[1])?, c.src(&p[2])?);
            let f = if op == OP_MIN { "min" } else { "max" };
            c.assign(&p[0], &format!("{f}({a}, {b})"))?;
        }
        OP_SLT | OP_SGE => {
            need(3)?;
            let (a, b) = (c.src(&p[1])?, c.src(&p[2])?);
            let f = if op == OP_SLT {
                "lessThan"
            } else {
                "greaterThanEqual"
            };
            c.assign(&p[0], &format!("vec4({f}(vec4({a}), vec4({b})))"))?;
        }
        OP_LIT => {
            need(2)?;
            let s = c.src(&p[1])?;
            c.assign(
                &p[0],
                &format!(
                    "vec4(1.0, max(({s}).x, 0.0), (({s}).x > 0.0 ? pow(max(({s}).y, 0.0), clamp(({s}).w, -128.0, 128.0)) : 0.0), 1.0)"
                ),
            )?;
        }
        OP_DST => {
            need(3)?;
            let (a, b) = (c.src(&p[1])?, c.src(&p[2])?);
            c.assign(
                &p[0],
                &format!("vec4(1.0, ({a}).y * ({b}).y, ({a}).z, ({b}).w)"),
            )?;
        }
        OP_LRP => {
            need(4)?;
            let (t, a, b) = (c.src(&p[1])?, c.src(&p[2])?, c.src(&p[3])?);
            c.assign(&p[0], &format!("mix({b}, {a}, {t})"))?;
        }
        OP_FRC => {
            need(2)?;
            let s = c.src(&p[1])?;
            c.assign(&p[0], &format!("fract({s})"))?;
        }
        OP_M4X4 | OP_M4X3 | OP_M3X4 | OP_M3X3 | OP_M3X2 => {
            need(3)?;
            let (rows, dim) = match op {
                OP_M4X4 => (4, 4),
                OP_M4X3 => (3, 4),
                OP_M3X4 => (4, 3),
                OP_M3X3 => (3, 3),
                _ => (2, 3),
            };
            let a = c.src(&p[1])?;
            let mut parts = Vec::new();
            for r in 0..rows {
                let mut row = p[2];
                row.num += r;
                let b = c.src(&row)?;
                parts.push(if dim == 4 {
                    format!("dot({a}, {b})")
                } else {
                    format!("dot(({a}).xyz, ({b}).xyz)")
                });
            }
            while parts.len() < 4 {
                parts.push("0.0".into());
            }
            let mut d = p[0];
            d.mask &= (1 << rows) - 1;
            c.assign(&d, &format!("vec4({})", parts.join(", ")))?;
        }
        OP_POW => {
            need(3)?;
            let (a, b) = (c.scalar(&p[1])?, c.scalar(&p[2])?);
            c.assign(&p[0], &format!("pow(abs({a}), {b})"))?;
        }
        OP_CRS => {
            need(3)?;
            let (a, b) = (c.src(&p[1])?, c.src(&p[2])?);
            c.assign(&p[0], &format!("vec4(cross(({a}).xyz, ({b}).xyz), 0.0)"))?;
        }
        OP_SGN => {
            need(2)?;
            let s = c.src(&p[1])?;
            c.assign(&p[0], &format!("sign({s})"))?;
        }
        OP_ABS => {
            need(2)?;
            let s = c.src(&p[1])?;
            c.assign(&p[0], &format!("abs({s})"))?;
        }
        OP_NRM => {
            need(2)?;
            let s = c.src(&p[1])?;
            c.assign(
                &p[0],
                &format!("({s}) * inversesqrt(max(dot(({s}).xyz, ({s}).xyz), 1e-30))"),
            )?;
        }
        OP_SINCOS => {
            need(2)?;
            let s = c.scalar(&p[1])?;
            c.assign(&p[0], &format!("vec4(cos({s}), sin({s}), 0.0, 0.0)"))?;
        }
        OP_CMP => {
            need(4)?;
            let (t, a, b) = (c.src(&p[1])?, c.src(&p[2])?, c.src(&p[3])?);
            c.assign(
                &p[0],
                &format!("mix(vec4({b}), vec4({a}), vec4(greaterThanEqual(vec4({t}), vec4(0.0))))"),
            )?;
        }
        OP_CND => {
            need(4)?;
            let (t, a, b) = (c.src(&p[1])?, c.src(&p[2])?, c.src(&p[3])?);
            let cond = if c.major == 1 && c.minor < 4 {
                format!("vec4(({t}).w > 0.5)")
            } else {
                format!("vec4(greaterThan(vec4({t}), vec4(0.5)))")
            };
            c.assign(&p[0], &format!("mix(vec4({b}), vec4({a}), {cond})"))?;
        }
        OP_DP2ADD => {
            need(4)?;
            let (a, b, d) = (c.src(&p[1])?, c.src(&p[2])?, c.scalar(&p[3])?);
            c.assign(&p[0], &format!("dot(({a}).xy, ({b}).xy) + {d}"))?;
        }
        OP_DSX | OP_DSY => {
            need(2)?;
            let s = c.src(&p[1])?;
            let f = if op == OP_DSX { "dFdx" } else { "dFdy" };
            c.assign(&p[0], &format!("{f}({s})"))?;
        }
        OP_TEXCOORD => {
            need(1)?;
            if c.major == 1 && c.minor >= 4 {
                // texcrd rN, tM
                need(2)?;
                let s = c.src(&p[1])?;
                c.assign(&p[0], &s)?;
            } else {
                let n = p[0].num.min(7);
                c.assign(&p[0], &format!("clamp(gl_TexCoord[{n}], 0.0, 1.0)"))?;
            }
        }
        OP_TEXKILL => {
            need(1)?;
            let n = c.reg_name(&p[0], false)?;
            c.line(&format!(
                "if (any(lessThan(({n}).xyz, vec3(0.0)))) discard;"
            ));
        }
        OP_TEX | OP_TEXLDD | OP_TEXLDL => {
            if c.major == 1 && c.minor < 4 {
                need(1)?;
                let n = p[0].num.min(7);
                c.assign(&p[0], &format!("texture2D(s{n}, gl_TexCoord[{n}].xy)"))?;
                return Ok(());
            }
            if c.major == 1 {
                // ps_1_4 : texld rN, tM (échantillonneur N)
                need(2)?;
                let coord = c.src(&p[1])?;
                let n = p[0].num;
                if !c.samplers.iter().any(|s| s.0 == n) {
                    c.samplers.push((n, SamplerType::Tex2D));
                }
                c.assign(&p[0], &format!("texture2D(s{n}, ({coord}).xy)"))?;
                return Ok(());
            }
            need(3)?;
            let coord = c.src(&p[1])?;
            let s = p[2].num;
            let ty = c
                .samplers
                .iter()
                .find(|x| x.0 == s)
                .map(|x| x.1)
                .unwrap_or(SamplerType::Tex2D);
            let project = op == OP_TEX && ctrl & 1 != 0;
            let bias = op == OP_TEX && ctrl & 2 != 0;
            let lod_vs = op == OP_TEXLDL && !c.pixel;
            let e = match ty {
                SamplerType::Cube => format!("textureCube(s{s}, ({coord}).xyz)"),
                SamplerType::Volume => format!("texture3D(s{s}, ({coord}).xyz)"),
                SamplerType::Tex2D if project => format!("texture2DProj(s{s}, {coord})"),
                SamplerType::Tex2D if bias => format!("texture2D(s{s}, ({coord}).xy, ({coord}).w)"),
                SamplerType::Tex2D if lod_vs => {
                    format!("texture2DLod(s{s}, ({coord}).xy, ({coord}).w)")
                }
                SamplerType::Tex2D => format!("texture2D(s{s}, ({coord}).xy)"),
            };
            c.assign(&p[0], &e)?;
        }
        OP_IF => {
            need(1)?;
            let b = c.reg_name(&p[0], false)?;
            c.line(&format!("if ({b}) {{"));
            c.indent += 1;
        }
        OP_IFC => {
            need(2)?;
            let (a, b) = (c.scalar(&p[0])?, c.scalar(&p[1])?);
            let o = cmp_op(ctrl)?;
            c.line(&format!("if ({a} {o} {b}) {{"));
            c.indent += 1;
        }
        OP_ELSE => {
            c.indent = c.indent.saturating_sub(1).max(1);
            c.line("} else {");
            c.indent += 1;
        }
        OP_ENDIF => {
            c.indent = c.indent.saturating_sub(1).max(1);
            c.line("}");
        }
        OP_REP => {
            need(1)?;
            let n = c.reg_name(&p[0], false)?;
            let d = c.loop_depth;
            c.line(&format!(
                "for (int k{d} = 0; k{d} < int(({n}).x) && k{d} < 255; k{d}++) {{"
            ));
            c.loop_depth += 1;
            c.max_loop = c.max_loop.max(c.loop_depth);
            c.indent += 1;
        }
        OP_LOOP => {
            need(2)?;
            let n = c.reg_name(&p[1], false)?;
            let d = c.loop_depth;
            c.line(&format!(
                "aL = int(({n}).y); for (int k{d} = 0; k{d} < int(({n}).x) && k{d} < 255; k{d}++, aL += int(({n}).z)) {{"
            ));
            c.loop_depth += 1;
            c.max_loop = c.max_loop.max(c.loop_depth);
            c.indent += 1;
        }
        OP_ENDREP | OP_ENDLOOP => {
            if c.loop_depth == 0 {
                return Err("fin de boucle sans début".into());
            }
            c.loop_depth -= 1;
            c.indent = c.indent.saturating_sub(1).max(1);
            c.line("}");
        }
        OP_BREAK => c.line("break;"),
        OP_BREAKC => {
            need(2)?;
            let (a, b) = (c.scalar(&p[0])?, c.scalar(&p[1])?);
            let o = cmp_op(ctrl)?;
            c.line(&format!("if ({a} {o} {b}) break;"));
        }
        other => return Err(format!("opcode {other} non pris en charge")),
    }
    Ok(())
}

fn finish(c: Ctx) -> Sm9Shader {
    let mut s = String::from("#version 120\n");
    let pre = c.const_prefix();
    let nconst = if c.pixel { 224 } else { 256 };
    let _ = writeln!(s, "uniform vec4 {pre}[{nconst}];");
    let _ = writeln!(s, "uniform ivec4 {pre}i[16];");
    let _ = writeln!(s, "uniform bool {pre}b[16];");
    for (n, v) in &c.defs {
        let _ = writeln!(
            s,
            "const vec4 cd{n} = vec4({:?}, {:?}, {:?}, {:?});",
            v[0], v[1], v[2], v[3]
        );
    }
    for (n, v) in &c.defi {
        let _ = writeln!(
            s,
            "const ivec4 ci{n} = ivec4({}, {}, {}, {});",
            v[0], v[1], v[2], v[3]
        );
    }
    let mut inputs = Vec::new();
    if c.pixel {
        for (n, ty) in &c.samplers {
            let t = match ty {
                SamplerType::Tex2D => "sampler2D",
                SamplerType::Cube => "samplerCube",
                SamplerType::Volume => "sampler3D",
            };
            let _ = writeln!(s, "uniform {t} s{n};");
        }
    } else {
        let _ = writeln!(s, "uniform vec4 wl_pos_fixup;");
        for (n, ty) in &c.samplers {
            let t = if *ty == SamplerType::Cube {
                "samplerCube"
            } else {
                "sampler2D"
            };
            let _ = writeln!(s, "uniform {t} s{n};");
        }
        let mut seen = Vec::new();
        for &(reg, u, i) in &c.vs_inputs {
            let name = attr_name(u, i);
            if !seen.contains(&name) {
                let _ = writeln!(s, "attribute vec4 {name};");
                seen.push(name.clone());
                inputs.push(VsInput {
                    usage: u,
                    index: i,
                    name,
                });
            }
            let _ = reg;
        }
        // Registres v# lus sans dcl (vs_1_1 permissif).
        for n in 0..16 {
            let name = format!("a_v{n}");
            if c.body.contains(&name) && !seen.contains(&name) {
                let _ = writeln!(s, "attribute vec4 {name};");
                inputs.push(VsInput {
                    usage: if n == 0 {
                        USAGE_POSITION
                    } else {
                        USAGE_TEXCOORD
                    },
                    index: if n == 0 { 0 } else { (n - 1) as u8 },
                    name,
                });
            }
        }
    }
    s.push_str("void main() {\n");
    for r in 0..c.temps {
        let _ = writeln!(s, "    vec4 r{r} = vec4(0.0);");
    }
    if c.uses_addr {
        s.push_str("    ivec4 a0 = ivec4(0);\n");
    }
    if c.max_loop > 0 || c.body.contains("aL") {
        s.push_str("    int aL = 0;\n");
    }
    if c.pixel {
        for n in 0..8 {
            if c.uses_texreg & (1 << n) != 0 || c.tex_regs_written & (1 << n) != 0 {
                if c.major >= 2 || c.minor >= 4 {
                    let _ = writeln!(s, "    vec4 t{n} = gl_TexCoord[{n}];");
                } else {
                    let _ = writeln!(s, "    vec4 t{n} = vec4(0.0);");
                }
            }
        }
        for i in 0..4 {
            if c.uses_outputs.c[i] {
                let _ = writeln!(s, "    vec4 o_c{i} = vec4(0.0);");
            }
        }
        if c.uses_outputs.depth {
            s.push_str("    float o_depth = gl_FragCoord.z;\n");
        }
    } else {
        s.push_str("    vec4 o_pos = vec4(0.0, 0.0, 0.0, 1.0);\n");
        if c.major >= 3 {
            for &(n, _, _) in &c.vs3_outputs {
                let _ = writeln!(s, "    vec4 o{n} = vec4(0.0);");
            }
        } else {
            s.push_str("    vec4 o_fog = vec4(0.0);\n    vec4 o_psize = vec4(1.0);\n");
            s.push_str("    vec4 o_d0 = vec4(1.0);\n    vec4 o_d1 = vec4(0.0);\n");
            for t in 0..8 {
                if c.uses_outputs.t[t] {
                    let _ = writeln!(s, "    vec4 o_t{t} = vec4(0.0);");
                }
            }
        }
    }
    s.push_str(&c.body);
    if c.pixel {
        if c.major == 1 {
            // ps_1_x : la couleur finale est r0.
            s.push_str("    gl_FragColor = r0;\n");
        } else {
            for i in 0..4 {
                if c.uses_outputs.c[i] {
                    let _ = writeln!(s, "    gl_FragData[{i}] = o_c{i};");
                }
            }
        }
        if c.uses_outputs.depth {
            s.push_str("    gl_FragDepth = o_depth;\n");
        }
    } else {
        if c.major >= 3 {
            for &(n, u, i) in &c.vs3_outputs {
                let line = match u {
                    USAGE_POSITION | USAGE_POSITIONT => format!("    o_pos = o{n};\n"),
                    USAGE_COLOR if i == 0 => format!("    gl_FrontColor = o{n};\n"),
                    USAGE_COLOR => format!("    gl_FrontSecondaryColor = o{n};\n"),
                    USAGE_TEXCOORD => format!("    gl_TexCoord[{}] = o{n};\n", i.min(7)),
                    USAGE_FOG => format!("    gl_FogFragCoord = o{n}.x;\n"),
                    USAGE_PSIZE => format!("    gl_PointSize = o{n}.x;\n"),
                    _ => String::new(),
                };
                s.push_str(&line);
            }
        } else {
            s.push_str("    gl_FrontColor = o_d0;\n    gl_FrontSecondaryColor = o_d1;\n");
            s.push_str("    gl_FogFragCoord = o_fog.x;\n");
            for t in 0..8 {
                if c.uses_outputs.t[t] {
                    let _ = writeln!(s, "    gl_TexCoord[{t}] = o_t{t};");
                }
            }
        }
        // Espace de découpe D3D (z dans [0, w]) → OpenGL ([-w, w]), rendu
        // retourné verticalement (la ligne 0 des cibles est en haut, comme
        // sous Direct3D) et décalage d'un demi-pixel propre à Direct3D 9.
        s.push_str("    gl_Position = o_pos;\n");
        s.push_str("    gl_Position.y = -o_pos.y;\n");
        s.push_str("    gl_Position.xy += wl_pos_fixup.xy * o_pos.w;\n");
        s.push_str("    gl_Position.z = o_pos.z * 2.0 - o_pos.w;\n");
    }
    s.push_str("}\n");
    Sm9Shader {
        pixel: c.pixel,
        major: c.major,
        minor: c.minor,
        glsl: s,
        inputs,
        samplers: c.samplers,
        defs: c.defs,
    }
}

/// Longueur en jetons d'un shader (jusqu'à `END` inclus), bornée.
pub(crate) unsafe fn token_len(ptr: *const u32, max: usize) -> usize {
    for i in 0..max {
        if *ptr.add(i) == 0x0000_ffff {
            return i + 1;
        }
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlls::gl_bindings as gl;

    /// Traduit puis, si un affichage X et OpenGL sont disponibles, fait
    /// compiler le GLSL par le vrai pilote (Mesa en CI locale).
    fn checked(t: &[u32]) -> Sm9Shader {
        let s = translate(t).unwrap();
        thread_local! {
            static CTX: Option<crate::dlls::dxgi::GlContext> =
                if std::env::var_os("DISPLAY").is_some() {
                    crate::dlls::dxgi::GlContext::create_for_window(0)
                } else {
                    None
                };
        }
        CTX.with(|c| {
            let (Some(_), Some(g)) = (c.as_ref(), gl::gl()) else {
                return;
            };
            unsafe {
                let ty = if s.pixel {
                    gl::GL_FRAGMENT_SHADER
                } else {
                    gl::GL_VERTEX_SHADER
                };
                let sh = (g.CreateShader)(ty);
                let src = std::ffi::CString::new(s.glsl.clone()).unwrap();
                let p = src.as_ptr();
                (g.ShaderSource)(sh, 1, &p, std::ptr::null());
                (g.CompileShader)(sh);
                let mut ok = 0;
                (g.GetShaderiv)(sh, gl::GL_COMPILE_STATUS, &mut ok);
                let mut log = vec![0i8; 2048];
                let mut n = 0;
                (g.GetShaderInfoLog)(sh, 2048, &mut n, log.as_mut_ptr());
                let msg = String::from_utf8_lossy(
                    &log[..n.max(0) as usize]
                        .iter()
                        .map(|&c| c as u8)
                        .collect::<Vec<_>>(),
                )
                .into_owned();
                (g.DeleteShader)(sh);
                assert!(ok != 0, "GLSL refusé par le pilote : {msg}\n{}", s.glsl);
            }
        });
        s
    }

    // Aides pour assembler des jetons à la main.
    fn ins(op: u32, len: u32) -> u32 {
        op | (len << 24)
    }
    fn dst(ty: u32, n: u32, mask: u32) -> u32 {
        0x8000_0000 | ((ty & 7) << 28) | ((ty & 0x18) << 8) | (mask << 16) | n
    }
    fn src(ty: u32, n: u32) -> u32 {
        0x8000_0000 | ((ty & 7) << 28) | ((ty & 0x18) << 8) | (0xe4 << 16) | n
    }
    fn src_mod(ty: u32, n: u32, swz: u32, m: u32) -> u32 {
        0x8000_0000 | ((ty & 7) << 28) | ((ty & 0x18) << 8) | (swz << 16) | (m << 24) | n
    }

    #[test]
    fn vs_2_0_transformation() {
        // vs_2_0 ; dcl_position v0 ; dcl_texcoord0 v1 ; m4x4 oPos, v0, c0 ; mov oT0, v1
        let t = [
            0xfffe_0200,
            ins(OP_DCL, 2),
            0x8000_0000 | USAGE_POSITION as u32,
            dst(R_INPUT, 0, 0xf),
            ins(OP_DCL, 2),
            0x8000_0000 | USAGE_TEXCOORD as u32,
            dst(R_INPUT, 1, 0xf),
            ins(OP_M4X4, 3),
            dst(R_RASTOUT, 0, 0xf),
            src(R_INPUT, 0),
            src(R_CONST, 0),
            ins(OP_MOV, 2),
            dst(R_OUTPUT, 0, 0x3),
            src(R_INPUT, 1),
            0x0000_ffff,
        ];
        let s = checked(&t);
        assert!(!s.pixel);
        assert!(s.glsl.contains("attribute vec4 a_position0;"));
        assert!(s.glsl.contains("dot(a_position0, vc[0])"));
        assert!(s.glsl.contains("dot(a_position0, vc[3])"));
        assert!(s.glsl.contains("o_t0.xy = (vec4(a_texcoord0)).xy;"));
        assert!(s.glsl.contains("gl_TexCoord[0] = o_t0;"));
        assert!(s.glsl.contains("gl_Position.z = o_pos.z * 2.0 - o_pos.w;"));
        assert_eq!(s.inputs.len(), 2);
    }

    #[test]
    fn ps_2_0_texture_et_constante() {
        // ps_2_0 ; def c1, 0.5,0.5,0.5,1 ; dcl t0 ; dcl_2d s0 ; texld r0, t0, s0 ; mul r0, r0, c1 ; mov oC0, r0
        let half = 0.5f32.to_bits();
        let one = 1.0f32.to_bits();
        let t = [
            0xffff_0200,
            ins(OP_DEF, 5),
            dst(R_CONST, 1, 0xf),
            half,
            half,
            half,
            one,
            ins(OP_DCL, 2),
            0x8000_0000,
            dst(R_ADDR_TEXTURE, 0, 0xf),
            ins(OP_DCL, 2),
            0x8000_0000 | (2 << 27),
            dst(R_SAMPLER, 0, 0xf),
            ins(OP_TEX, 3),
            dst(R_TEMP, 0, 0xf),
            src(R_ADDR_TEXTURE, 0),
            src(R_SAMPLER, 0),
            ins(OP_MUL, 3),
            dst(R_TEMP, 0, 0xf),
            src(R_TEMP, 0),
            src(R_CONST, 1),
            ins(OP_MOV, 2),
            dst(R_COLOROUT, 0, 0xf),
            src(R_TEMP, 0),
            0x0000_ffff,
        ];
        let s = checked(&t);
        assert!(s.pixel);
        assert!(s.glsl.contains("uniform sampler2D s0;"));
        assert!(s
            .glsl
            .contains("const vec4 cd1 = vec4(0.5, 0.5, 0.5, 1.0);"));
        assert!(s.glsl.contains("texture2D(s0, (t0).xy)"));
        assert!(s.glsl.contains("r0 * cd1"));
        assert!(s.glsl.contains("gl_FragData[0] = o_c0;"));
        assert_eq!(s.samplers, vec![(0, SamplerType::Tex2D)]);
    }

    #[test]
    fn ps_1_1_sans_longueur() {
        // ps_1_1 ; tex t0 ; mul r0, t0, v0  (SM1 : pas de champ longueur)
        let t = [
            0xffff_0101,
            OP_TEX,
            dst(R_ADDR_TEXTURE, 0, 0xf),
            OP_MUL,
            dst(R_TEMP, 0, 0xf),
            src(R_ADDR_TEXTURE, 0),
            src(R_INPUT, 0),
            0x0000_ffff,
        ];
        let s = checked(&t);
        assert!(s
            .glsl
            .contains("t0 = vec4(texture2D(s0, gl_TexCoord[0].xy));"));
        assert!(s.glsl.contains("r0 = vec4(t0 * gl_Color);"));
        assert!(s.glsl.contains("gl_FragColor = r0;"));
    }

    #[test]
    fn modificateurs_et_saturation() {
        // ps_2_0 ; mov_sat r0.xy, -c0.wzyx
        let t = [
            0xffff_0200,
            ins(OP_MOV, 2),
            dst(R_TEMP, 0, 0x3) | (1 << 20),
            src_mod(R_CONST, 0, 0x1b, 1),
            0x0000_ffff,
        ];
        let s = checked(&t);
        assert!(s
            .glsl
            .contains("r0.xy = (clamp(vec4((-pc[0].wzyx)), 0.0, 1.0)).xy;"));
    }

    #[test]
    fn boucle_et_condition() {
        // vs_3_0 ; defi i0, 4,0,1,0 ; dcl_position v0 ; dcl_position o0 ;
        // mov r0, v0 ; rep i0 ; add r0, r0, c0 ; endrep ; if b0 ; mov r0, c1 ; endif ; mov o0, r0
        let t = [
            0xfffe_0300,
            ins(OP_DEFI, 5),
            dst(R_CONSTINT, 0, 0xf),
            4,
            0,
            1,
            0,
            ins(OP_DCL, 2),
            0x8000_0000,
            dst(R_INPUT, 0, 0xf),
            ins(OP_DCL, 2),
            0x8000_0000,
            dst(R_OUTPUT, 0, 0xf),
            ins(OP_MOV, 2),
            dst(R_TEMP, 0, 0xf),
            src(R_INPUT, 0),
            ins(OP_REP, 1),
            src(R_CONSTINT, 0),
            ins(OP_ADD, 3),
            dst(R_TEMP, 0, 0xf),
            src(R_TEMP, 0),
            src(R_CONST, 0),
            ins(OP_ENDREP, 0),
            ins(OP_IF, 1),
            src(R_CONSTBOOL, 0),
            ins(OP_MOV, 2),
            dst(R_TEMP, 0, 0xf),
            src(R_CONST, 1),
            ins(OP_ENDIF, 0),
            ins(OP_MOV, 2),
            dst(R_OUTPUT, 0, 0xf),
            src(R_TEMP, 0),
            0x0000_ffff,
        ];
        let s = checked(&t);
        assert!(s.glsl.contains("for (int k0 = 0; k0 < int((vec4(ci0)).x)"));
        assert!(s.glsl.contains("if (vcb[0]) {"));
        assert!(s.glsl.contains("o_pos = o0;"));
    }

    #[test]
    fn adressage_relatif_vs2() {
        // vs_2_0 ; mova a0.x, c0.x ; mov r0, c[a0.x + 5]
        let t = [
            0xfffe_0200,
            ins(OP_MOVA, 2),
            dst(R_ADDR_TEXTURE, 0, 0x1),
            src_mod(R_CONST, 0, 0x00, 0),
            ins(OP_MOV, 3),
            dst(R_TEMP, 0, 0xf),
            src(R_CONST, 5) | (1 << 13),
            0x8000_0000 | ((R_ADDR_TEXTURE & 7) << 28),
            0x0000_ffff,
        ];
        let s = checked(&t);
        assert!(s.glsl.contains("ivec4 a0 = ivec4(0);"));
        assert!(s
            .glsl
            .contains("a0.x = ivec4(floor(vec4(vc[0].xxxx) + 0.5)).x;"));
        assert!(s.glsl.contains("vc[a0.x + 5]"));
    }

    #[test]
    fn refus_propres() {
        assert!(translate(&[]).is_err());
        assert!(translate(&[0x1234_0000]).is_err());
        assert!(translate(&[0xfffe_0400, 0xffff]).is_err());
        // Instruction prédiquée
        assert!(translate(&[0xffff_0300, ins(OP_MOV, 2) | (1 << 28), 0, 0, 0xffff]).is_err());
        // Bytecode tronqué
        assert!(translate(&[0xffff_0200, ins(OP_MOV, 2), dst(R_TEMP, 0, 0xf)]).is_err());
        // Sous-routine
        assert!(translate(&[0xfffe_0200, ins(OP_CALL, 1), 0, 0xffff]).is_err());
    }

    #[test]
    fn commentaires_ignores() {
        let t = [
            0xffff_0200,
            OP_COMMENT | (2 << 16),
            0x4142_4344,
            0x4546_4748,
            ins(OP_MOV, 2),
            dst(R_COLOROUT, 0, 0xf),
            src(R_INPUT, 0),
            0x0000_ffff,
        ];
        let s = checked(&t);
        assert!(s.glsl.contains("o_c0 = vec4(gl_Color);"));
    }
}
