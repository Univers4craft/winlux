//! Petit assembleur SPIR-V pour les shaders du backend Vulkan.
//!
//! Les shaders D3D compilés (DXBC/DXIL) ne sont pas encore traduits en
//! SPIR-V : comme le chemin OpenGL (`d3d_shader::passthrough_*`), le backend
//! Vulkan utilise des shaders « passe-plat » (position transmise telle
//! quelle, couleur de sommet ou blanc) générés ici, mot par mot.

const MAGIC: u32 = 0x0723_0203;
const VERSION_1_0: u32 = 0x0001_0000;

// Opcodes utilisés.
const OP_CAPABILITY: u16 = 17;
const OP_MEMORY_MODEL: u16 = 14;
const OP_ENTRY_POINT: u16 = 15;
const OP_EXECUTION_MODE: u16 = 16;
const OP_DECORATE: u16 = 71;
const OP_TYPE_VOID: u16 = 19;
const OP_TYPE_FUNCTION: u16 = 33;
const OP_TYPE_FLOAT: u16 = 22;
const OP_TYPE_VECTOR: u16 = 23;
const OP_TYPE_POINTER: u16 = 32;
const OP_CONSTANT: u16 = 43;
const OP_CONSTANT_COMPOSITE: u16 = 44;
const OP_VARIABLE: u16 = 59;
const OP_FUNCTION: u16 = 54;
const OP_LABEL: u16 = 248;
const OP_LOAD: u16 = 61;
const OP_STORE: u16 = 62;
const OP_RETURN: u16 = 253;
const OP_FUNCTION_END: u16 = 56;

const CAP_SHADER: u32 = 1;
const ADDRESSING_LOGICAL: u32 = 0;
const MEMORY_GLSL450: u32 = 1;
const MODEL_VERTEX: u32 = 0;
const MODEL_FRAGMENT: u32 = 4;
const MODE_ORIGIN_UPPER_LEFT: u32 = 7;
const DECO_BUILTIN: u32 = 11;
const DECO_LOCATION: u32 = 30;
const BUILTIN_POSITION: u32 = 0;
const SC_INPUT: u32 = 1;
const SC_OUTPUT: u32 = 3;
const FN_CONTROL_NONE: u32 = 0;

struct Module {
    words: Vec<u32>,
    next_id: u32,
}

impl Module {
    fn new() -> Self {
        Module {
            words: vec![MAGIC, VERSION_1_0, 0, 0, 0],
            next_id: 1,
        }
    }
    fn id(&mut self) -> u32 {
        let i = self.next_id;
        self.next_id += 1;
        i
    }
    fn op(&mut self, opcode: u16, operands: &[u32]) {
        self.words
            .push(((operands.len() as u32 + 1) << 16) | opcode as u32);
        self.words.extend_from_slice(operands);
    }
    fn string_words(s: &str) -> Vec<u32> {
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        bytes
            .chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
    fn finish(mut self) -> Vec<u32> {
        self.words[3] = self.next_id; // borne des identifiants
        self.words
    }
}

/// Shader de sommets : `gl_Position = in0;` et, si `with_color`,
/// `out0 = in1;`.
pub(crate) fn passthrough_vertex(with_color: bool) -> Vec<u32> {
    let mut m = Module::new();
    let main = m.id();
    let void = m.id();
    let fn_ty = m.id();
    let float = m.id();
    let v4 = m.id();
    let ptr_in = m.id();
    let ptr_out = m.id();
    let pos_in = m.id();
    let pos_out = m.id();
    let col_in = m.id();
    let col_out = m.id();
    let label = m.id();
    let tmp_pos = m.id();
    let tmp_col = m.id();

    m.op(OP_CAPABILITY, &[CAP_SHADER]);
    m.op(OP_MEMORY_MODEL, &[ADDRESSING_LOGICAL, MEMORY_GLSL450]);
    let mut ep = vec![MODEL_VERTEX, main];
    ep.extend(Module::string_words("main"));
    ep.extend([pos_in, pos_out]);
    if with_color {
        ep.extend([col_in, col_out]);
    }
    m.op(OP_ENTRY_POINT, &ep);
    m.op(OP_DECORATE, &[pos_in, DECO_LOCATION, 0]);
    m.op(OP_DECORATE, &[pos_out, DECO_BUILTIN, BUILTIN_POSITION]);
    if with_color {
        m.op(OP_DECORATE, &[col_in, DECO_LOCATION, 1]);
        m.op(OP_DECORATE, &[col_out, DECO_LOCATION, 0]);
    }
    m.op(OP_TYPE_VOID, &[void]);
    m.op(OP_TYPE_FUNCTION, &[fn_ty, void]);
    m.op(OP_TYPE_FLOAT, &[float, 32]);
    m.op(OP_TYPE_VECTOR, &[v4, float, 4]);
    m.op(OP_TYPE_POINTER, &[ptr_in, SC_INPUT, v4]);
    m.op(OP_TYPE_POINTER, &[ptr_out, SC_OUTPUT, v4]);
    m.op(OP_VARIABLE, &[ptr_in, pos_in, SC_INPUT]);
    m.op(OP_VARIABLE, &[ptr_out, pos_out, SC_OUTPUT]);
    if with_color {
        m.op(OP_VARIABLE, &[ptr_in, col_in, SC_INPUT]);
        m.op(OP_VARIABLE, &[ptr_out, col_out, SC_OUTPUT]);
    }
    m.op(OP_FUNCTION, &[void, main, FN_CONTROL_NONE, fn_ty]);
    m.op(OP_LABEL, &[label]);
    m.op(OP_LOAD, &[v4, tmp_pos, pos_in]);
    m.op(OP_STORE, &[pos_out, tmp_pos]);
    if with_color {
        m.op(OP_LOAD, &[v4, tmp_col, col_in]);
        m.op(OP_STORE, &[col_out, tmp_col]);
    }
    m.op(OP_RETURN, &[]);
    m.op(OP_FUNCTION_END, &[]);
    m.finish()
}

/// Shader de fragments : couleur interpolée (`with_color`) ou blanc opaque.
pub(crate) fn passthrough_fragment(with_color: bool) -> Vec<u32> {
    let mut m = Module::new();
    let main = m.id();
    let void = m.id();
    let fn_ty = m.id();
    let float = m.id();
    let v4 = m.id();
    let ptr_in = m.id();
    let ptr_out = m.id();
    let col_in = m.id();
    let out = m.id();
    let one = m.id();
    let white = m.id();
    let label = m.id();
    let tmp = m.id();

    m.op(OP_CAPABILITY, &[CAP_SHADER]);
    m.op(OP_MEMORY_MODEL, &[ADDRESSING_LOGICAL, MEMORY_GLSL450]);
    let mut ep = vec![MODEL_FRAGMENT, main];
    ep.extend(Module::string_words("main"));
    if with_color {
        ep.push(col_in);
    }
    ep.push(out);
    m.op(OP_ENTRY_POINT, &ep);
    m.op(OP_EXECUTION_MODE, &[main, MODE_ORIGIN_UPPER_LEFT]);
    if with_color {
        m.op(OP_DECORATE, &[col_in, DECO_LOCATION, 0]);
    }
    m.op(OP_DECORATE, &[out, DECO_LOCATION, 0]);
    m.op(OP_TYPE_VOID, &[void]);
    m.op(OP_TYPE_FUNCTION, &[fn_ty, void]);
    m.op(OP_TYPE_FLOAT, &[float, 32]);
    m.op(OP_TYPE_VECTOR, &[v4, float, 4]);
    m.op(OP_TYPE_POINTER, &[ptr_in, SC_INPUT, v4]);
    m.op(OP_TYPE_POINTER, &[ptr_out, SC_OUTPUT, v4]);
    m.op(OP_CONSTANT, &[float, one, 1.0f32.to_bits()]);
    m.op(OP_CONSTANT_COMPOSITE, &[v4, white, one, one, one, one]);
    if with_color {
        m.op(OP_VARIABLE, &[ptr_in, col_in, SC_INPUT]);
    }
    m.op(OP_VARIABLE, &[ptr_out, out, SC_OUTPUT]);
    m.op(OP_FUNCTION, &[void, main, FN_CONTROL_NONE, fn_ty]);
    m.op(OP_LABEL, &[label]);
    if with_color {
        m.op(OP_LOAD, &[v4, tmp, col_in]);
        m.op(OP_STORE, &[out, tmp]);
    } else {
        m.op(OP_STORE, &[out, white]);
    }
    m.op(OP_RETURN, &[]);
    m.op(OP_FUNCTION_END, &[]);
    m.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parcourt les instructions : chaque mot d'en-tête doit annoncer une
    /// longueur cohérente jusqu'à la fin exacte du module.
    fn walk(words: &[u32]) -> Vec<u16> {
        assert_eq!(words[0], MAGIC);
        let mut ops = Vec::new();
        let mut i = 5;
        while i < words.len() {
            let len = (words[i] >> 16) as usize;
            assert!(len >= 1, "instruction de longueur nulle au mot {i}");
            ops.push((words[i] & 0xffff) as u16);
            i += len;
        }
        assert_eq!(i, words.len());
        ops
    }

    #[test]
    fn modules_bien_formes() {
        for color in [false, true] {
            let vs = walk(&passthrough_vertex(color));
            assert_eq!(vs[0], OP_CAPABILITY);
            assert_eq!(*vs.last().unwrap(), OP_FUNCTION_END);
            let fs = walk(&passthrough_fragment(color));
            assert!(fs.contains(&OP_EXECUTION_MODE));
        }
    }

    #[test]
    fn borne_des_identifiants() {
        let vs = passthrough_vertex(true);
        assert!(vs[3] > 10);
    }

    #[test]
    fn chaines_completees_par_nul() {
        assert_eq!(Module::string_words("main"), vec![0x6e69_616d, 0]);
    }
}
