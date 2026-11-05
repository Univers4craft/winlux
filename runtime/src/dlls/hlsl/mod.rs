//! Compilateur HLSL maison (modèles 4 et 5, vertex, geometry et pixel shaders) :
//! préprocesseur, analyse, puis génération de GLSL 4.30 aux conventions de
//! la traduction DXBC (`dxbc.rs`). `D3DCompile` range le résultat dans un
//! conteneur DXBC (chunks `SHEX`, `ISGN`, `OSGN`, `WLGL` pour le GLSL et
//! `WLSP` pour les paires texture/échantillonneur), que la couche D3D11
//! reconnaît.

mod ast;
mod gen;
mod lex;
mod parse;
mod pp;

use crate::dlls::dxbc::{Shader, SigElement, Stage};

pub(crate) use pp::Includer;

/// D3DCOMPILE_PACK_MATRIX_ROW_MAJOR.
pub(crate) const FLAG_ROW_MAJOR: u32 = 1 << 3;

pub(crate) struct Compiled {
    pub stage: Stage,
    pub glsl: String,
    pub inputs: Vec<SigElement>,
    pub outputs: Vec<SigElement>,
    pub pairs: Vec<(u32, u32)>,
}

/// Étage d'après la cible (« vs_5_0 », « ps_4_0_level_9_3 »…).
pub(crate) fn stage_of(target: &str) -> Result<Stage, String> {
    match target.get(..3) {
        Some("vs_") => Ok(Stage::Vertex),
        Some("ps_") => Ok(Stage::Pixel),
        Some("gs_") => Ok(Stage::Geometry),
        _ => Err(format!("cible « {target} » non prise en charge")),
    }
}

/// Compile un source HLSL. Les erreurs ont la forme « (ligne) : message ».
pub(crate) fn compile(
    src: &str,
    defines: &[(String, String)],
    include: &mut Includer<'_>,
    entry: &str,
    target: &str,
    flags: u32,
) -> Result<Compiled, String> {
    let stage = stage_of(target)?;
    let tokens = pp::preprocess(src, defines, include)?;
    let prog = parse::parse(tokens, flags & FLAG_ROW_MAJOR != 0)?;
    let out = gen::Gen::new(&prog, stage)?.run(entry)?;
    Ok(Compiled {
        stage: out.stage,
        glsl: out.glsl,
        inputs: out.inputs,
        outputs: out.outputs,
        pairs: out.pairs,
    })
}

/// Préprocesseur seul (D3DPreprocess) : texte des jetons.
pub(crate) fn preprocess_text(
    src: &str,
    defines: &[(String, String)],
    include: &mut Includer<'_>,
) -> Result<String, String> {
    use lex::Tok;
    let tokens = pp::preprocess(src, defines, include)?;
    let mut out = String::new();
    let mut line = 0;
    for t in tokens {
        if t.line != line && !out.is_empty() {
            out.push('\n');
        }
        line = t.line;
        match t.t {
            Tok::Ident(s) => out.push_str(&s),
            Tok::Int(v, u) => out.push_str(&format!("{v}{}", if u { "u" } else { "" })),
            Tok::Float(f) => out.push_str(&format!("{f:?}")),
            Tok::Str(s) => out.push_str(&format!("\"{s}\"")),
            Tok::Punct(p) => out.push_str(p),
            Tok::PackMatrix(r) => out.push_str(&format!(
                "\n#pragma pack_matrix({})\n",
                if r { "row_major" } else { "column_major" }
            )),
        }
        out.push(' ');
    }
    out.push('\n');
    Ok(out)
}

fn signature(elems: &[SigElement]) -> Vec<u8> {
    let mut head = Vec::new();
    let mut names = Vec::new();
    let table = 8 + elems.len() * 24;
    head.extend_from_slice(&(elems.len() as u32).to_le_bytes());
    head.extend_from_slice(&8u32.to_le_bytes());
    for e in elems {
        let off = (table + names.len()) as u32;
        names.extend_from_slice(e.name.as_bytes());
        names.push(0);
        for v in [off, e.index, e.system_value, e.component_type, e.register] {
            head.extend_from_slice(&v.to_le_bytes());
        }
        head.extend_from_slice(&[e.mask, e.mask, 0, 0]);
    }
    while names.len() % 4 != 0 {
        names.push(0);
    }
    head.extend_from_slice(&names);
    head
}

/// Conteneur DXBC portant le GLSL compilé.
pub(crate) fn container(c: &Compiled) -> Vec<u8> {
    let ptype: u32 = match c.stage {
        Stage::Pixel => 0,
        Stage::Vertex => 1,
        Stage::Geometry => 2,
        Stage::Compute => 5,
    };
    let mut shex = Vec::new();
    shex.extend_from_slice(&((ptype << 16) | 0x50).to_le_bytes());
    shex.extend_from_slice(&2u32.to_le_bytes());
    let mut pairs = Vec::new();
    for (t, s) in &c.pairs {
        pairs.extend_from_slice(&t.to_le_bytes());
        pairs.extend_from_slice(&s.to_le_bytes());
    }
    let parts: [(&[u8; 4], Vec<u8>); 5] = [
        (b"SHEX", shex),
        (b"ISGN", signature(&c.inputs)),
        (b"OSGN", signature(&c.outputs)),
        (b"WLGL", c.glsl.as_bytes().to_vec()),
        (b"WLSP", pairs),
    ];
    let header = 32 + 4 * parts.len();
    let mut offsets = Vec::new();
    let mut body = Vec::new();
    for (tag, data) in &parts {
        offsets.push((header + body.len()) as u32);
        body.extend_from_slice(*tag);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    let total = header + body.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"DXBC");
    out.extend_from_slice(&[0u8; 16]);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(parts.len() as u32).to_le_bytes());
    for o in offsets {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&body);
    out
}

/// Shader prêt à l'emploi si le conteneur vient de ce compilateur.
pub(crate) fn from_container(blob: &[u8]) -> Option<Shader> {
    use crate::dlls::dxbc::{chunks, parse_signature};
    let ch = chunks(blob);
    let glsl = ch.iter().find(|c| &c.0 == b"WLGL")?.1;
    let mut s = Shader {
        stage: Stage::Pixel,
        glsl: String::from_utf8_lossy(glsl).into_owned(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        sampler_pairs: Vec::new(),
    };
    for (tag, data) in &ch {
        match tag {
            b"SHEX" => {
                let v = u32::from_le_bytes(data.get(..4)?.try_into().ok()?);
                s.stage = match v >> 16 {
                    1 => Stage::Vertex,
                    2 => Stage::Geometry,
                    5 => Stage::Compute,
                    _ => Stage::Pixel,
                };
            }
            b"ISGN" => s.inputs = parse_signature(data, tag),
            b"OSGN" => s.outputs = parse_signature(data, tag),
            b"WLSP" => {
                s.sampler_pairs = data
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| {
                        (
                            u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                            u32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                        )
                    })
                    .collect();
            }
            _ => {}
        }
    }
    Some(s)
}

#[cfg(test)]
mod tests;
