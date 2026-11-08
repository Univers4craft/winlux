//! Écriture d'assemblies .NET minimales (ECMA-335, partition II) pour les
//! programmes témoins du CLR intégré : un module, ses types, champs,
//! méthodes avec leur code IL, et les références vers d'autres assemblies.
//!
//! Écrit d'après la spécification publique ; sert aux tests, sans compilateur
//! C# : chaque témoin décrit son code IL octet par octet.
//!
//! Boîte à outils des témoins des étapes suivantes (génériques, délégués…) :
//! tout n'y sert pas encore.
#![allow(dead_code)]

use std::collections::HashMap;

/// Tables écrites, dans l'ordre de leur numéro (ECMA-335 §II.22).
const MODULE: usize = 0x00;
const TYPE_REF: usize = 0x01;
const TYPE_DEF: usize = 0x02;
const FIELD: usize = 0x04;
const METHOD_DEF: usize = 0x06;
const MEMBER_REF: usize = 0x0A;
const STANDALONE_SIG: usize = 0x11;
const ASSEMBLY: usize = 0x20;
const ASSEMBLY_REF: usize = 0x23;
const TYPE_SPEC: usize = 0x1B;
const GENERIC_PARAM: usize = 0x2A;
const METHOD_SPEC: usize = 0x2B;

pub const PUBLIC: u32 = 0x0001;
pub const ABSTRACT: u32 = 0x0080;
pub const BEFORE_FIELD_INIT: u32 = 0x0010_0000;

pub const M_PUBLIC: u16 = 0x0006;
pub const M_STATIC: u16 = 0x0010;
pub const M_VIRTUAL: u16 = 0x0040;
pub const M_HIDEBYSIG: u16 = 0x0080;
pub const M_NEWSLOT: u16 = 0x0100;
pub const M_SPECIALNAME: u16 = 0x0800;
pub const M_RTSPECIALNAME: u16 = 0x1000;
pub const M_CTOR: u16 = M_PUBLIC | M_HIDEBYSIG | M_SPECIALNAME | M_RTSPECIALNAME;
pub const M_CCTOR: u16 = 0x0011 | M_HIDEBYSIG | M_SPECIALNAME | M_RTSPECIALNAME;

pub const F_PUBLIC: u16 = 0x0006;
pub const F_STATIC: u16 = 0x0010;

/// Types élémentaires des signatures (§II.23.1.16).
pub const T_VOID: u8 = 0x01;
pub const T_BOOL: u8 = 0x02;
pub const T_I4: u8 = 0x08;
pub const T_STRING: u8 = 0x0E;
pub const T_OBJECT: u8 = 0x1C;

/// Corps d'une méthode : code IL et types des variables locales.
pub struct Body {
    pub code: Vec<u8>,
    pub max_stack: u16,
    pub locals: Vec<Vec<u8>>,
}

impl Body {
    pub fn new(code: Vec<u8>) -> Body {
        Body {
            code,
            max_stack: 8,
            locals: Vec::new(),
        }
    }
}

struct Method {
    flags: u16,
    name: u32,
    sig: u32,
    body: Option<Body>,
}

/// Assembly en construction. Les champs et méthodes appartiennent au dernier
/// type déclaré par [`Builder::type_def`].
pub struct Builder {
    name: String,
    strings: Vec<u8>,
    string_index: HashMap<String, u32>,
    blobs: Vec<u8>,
    user_strings: Vec<u8>,
    type_refs: Vec<(u32, u32, u32)>,
    type_defs: Vec<(u32, u32, u32, u32, u32, u32)>,
    fields: Vec<(u16, u32, u32)>,
    methods: Vec<Method>,
    member_refs: Vec<(u32, u32, u32)>,
    sigs: Vec<u32>,
    assembly_refs: Vec<u32>,
    type_specs: Vec<u32>,
    generic_params: Vec<(u16, u32, u32)>,
    method_specs: Vec<(u32, u32)>,
}

/// Portée d'une référence de type.
pub enum Scope {
    Assembly(u32),
    Nested(u32),
}

impl Builder {
    pub fn new(name: &str) -> Builder {
        let mut b = Builder {
            name: name.to_string(),
            strings: vec![0],
            string_index: HashMap::new(),
            blobs: vec![0],
            user_strings: vec![0],
            type_refs: Vec::new(),
            type_defs: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            member_refs: Vec::new(),
            sigs: Vec::new(),
            assembly_refs: Vec::new(),
            type_specs: Vec::new(),
            generic_params: Vec::new(),
            method_specs: Vec::new(),
        };
        b.type_def(0, "", "<Module>", 0);
        b
    }

    fn string(&mut self, s: &str) -> u32 {
        if s.is_empty() {
            return 0;
        }
        if let Some(&i) = self.string_index.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.extend_from_slice(s.as_bytes());
        self.strings.push(0);
        self.string_index.insert(s.to_string(), i);
        i
    }

    fn blob(&mut self, data: &[u8]) -> u32 {
        let i = self.blobs.len() as u32;
        compress(&mut self.blobs, data.len() as u32);
        self.blobs.extend_from_slice(data);
        i
    }

    /// Chaîne littérale pour `ldstr` (jeton 0x70…).
    pub fn user_string(&mut self, s: &str) -> u32 {
        let i = self.user_strings.len() as u32;
        let utf16: Vec<u16> = s.encode_utf16().collect();
        compress(&mut self.user_strings, utf16.len() as u32 * 2 + 1);
        for c in &utf16 {
            self.user_strings.extend_from_slice(&c.to_le_bytes());
        }
        self.user_strings.push(0);
        0x7000_0000 | i
    }

    pub fn assembly_ref(&mut self, name: &str) -> u32 {
        let n = self.string(name);
        self.assembly_refs.push(n);
        self.assembly_refs.len() as u32
    }

    /// Référence de type (jeton 0x01…).
    pub fn type_ref(&mut self, scope: Scope, namespace: &str, name: &str) -> u32 {
        let scope = match scope {
            Scope::Assembly(row) => (row << 2) | 2,
            Scope::Nested(token) => ((token & 0xFF_FFFF) << 2) | 3,
        };
        let (n, ns) = (self.string(name), self.string(namespace));
        self.type_refs.push((scope, n, ns));
        0x0100_0000 | self.type_refs.len() as u32
    }

    /// Déclare un type (jeton 0x02…) ; `extends` est un jeton TypeDef/TypeRef ou 0.
    pub fn type_def(&mut self, flags: u32, namespace: &str, name: &str, extends: u32) -> u32 {
        let extends = match extends >> 24 {
            0x02 => (extends & 0xFF_FFFF) << 2,
            0x01 => ((extends & 0xFF_FFFF) << 2) | 1,
            _ => 0,
        };
        let (n, ns) = (self.string(name), self.string(namespace));
        let fields = self.fields.len() as u32 + 1;
        let methods = self.methods.len() as u32 + 1;
        self.type_defs
            .push((flags, n, ns, extends, fields, methods));
        0x0200_0000 | self.type_defs.len() as u32
    }

    /// Champ du dernier type déclaré (jeton 0x04…) ; `ty` est un type de signature.
    pub fn field(&mut self, flags: u16, name: &str, ty: &[u8]) -> u32 {
        let mut sig = vec![0x06];
        sig.extend_from_slice(ty);
        let (n, s) = (self.string(name), self.blob(&sig));
        self.fields.push((flags, n, s));
        0x0400_0000 | self.fields.len() as u32
    }

    /// Méthode du dernier type déclaré (jeton 0x06…).
    pub fn method(&mut self, flags: u16, name: &str, sig: &[u8], body: Option<Body>) -> u32 {
        let (n, s) = (self.string(name), self.blob(sig));
        self.methods.push(Method {
            flags,
            name: n,
            sig: s,
            body,
        });
        0x0600_0000 | self.methods.len() as u32
    }

    /// Référence de membre (jeton 0x0A…) sur un type TypeRef ou TypeDef.
    pub fn member_ref(&mut self, parent: u32, name: &str, sig: &[u8]) -> u32 {
        let class = match parent >> 24 {
            0x02 => (parent & 0xFF_FFFF) << 3,
            0x01 => ((parent & 0xFF_FFFF) << 3) | 1,
            0x06 => ((parent & 0xFF_FFFF) << 3) | 3,
            0x1B => ((parent & 0xFF_FFFF) << 3) | 4,
            _ => 0,
        };
        let (n, s) = (self.string(name), self.blob(sig));
        self.member_refs.push((class, n, s));
        0x0A00_0000 | self.member_refs.len() as u32
    }

    /// Type construit (jeton 0x1B…) d'après sa signature (`generic_inst`…).
    pub fn type_spec(&mut self, sig: &[u8]) -> u32 {
        let b = self.blob(sig);
        self.type_specs.push(b);
        0x1B00_0000 | self.type_specs.len() as u32
    }

    /// Paramètre générique `number` d'un type (0x02…) ou d'une méthode (0x06…).
    pub fn generic_param(&mut self, owner: u32, number: u16, name: &str) {
        let coded = ((owner & 0xFF_FFFF) << 1) | u32::from(owner >> 24 == 0x06);
        let n = self.string(name);
        self.generic_params.push((number, coded, n));
    }

    /// Instanciation d'une méthode générique (jeton 0x2B…).
    pub fn method_spec(&mut self, method: u32, args: &[&[u8]]) -> u32 {
        let coded = ((method & 0xFF_FFFF) << 1) | u32::from(method >> 24 == 0x0A);
        let mut sig = vec![0x0A];
        compress(&mut sig, args.len() as u32);
        for a in args {
            sig.extend_from_slice(a);
        }
        let b = self.blob(&sig);
        self.method_specs.push((coded, b));
        0x2B00_0000 | self.method_specs.len() as u32
    }

    /// Assemble le fichier PE ; `entry` est le jeton du point d'entrée (0 pour une DLL).
    pub fn build(mut self, entry: u32) -> Vec<u8> {
        const SECTION_RVA: u32 = 0x2000;
        const FILE_ALIGN: usize = 0x200;
        let module_name = self.string(&format!("{}.dll", self.name));
        let asm_name = self.name.clone();
        let asm_name = self.string(&asm_name);

        // Section : en-tête CLI, corps des méthodes, métadonnées.
        let mut text = vec![0u8; 72];
        let mut rvas = Vec::new();
        let methods = std::mem::take(&mut self.methods);
        for m in &methods {
            let Some(body) = &m.body else {
                rvas.push(0);
                continue;
            };
            let locals = if body.locals.is_empty() {
                0
            } else {
                let mut sig = vec![0x07];
                compress(&mut sig, body.locals.len() as u32);
                for l in &body.locals {
                    sig.extend_from_slice(l);
                }
                let blob = self.blob(&sig);
                self.sigs.push(blob);
                0x1100_0000 | self.sigs.len() as u32
            };
            while !text.len().is_multiple_of(4) {
                text.push(0);
            }
            rvas.push(SECTION_RVA + text.len() as u32);
            let flags: u16 = 0x3003 | if locals != 0 { 0x10 } else { 0 };
            text.extend_from_slice(&flags.to_le_bytes());
            text.extend_from_slice(&body.max_stack.to_le_bytes());
            text.extend_from_slice(&(body.code.len() as u32).to_le_bytes());
            text.extend_from_slice(&locals.to_le_bytes());
            text.extend_from_slice(&body.code);
        }
        while !text.len().is_multiple_of(4) {
            text.push(0);
        }
        let metadata_rva = SECTION_RVA + text.len() as u32;
        let metadata = self.metadata(&methods, &rvas, module_name, asm_name);
        text.extend_from_slice(&metadata);

        // En-tête CLI (§II.25.3.3).
        let mut cli = Vec::new();
        cli.extend_from_slice(&72u32.to_le_bytes());
        cli.extend_from_slice(&2u16.to_le_bytes());
        cli.extend_from_slice(&5u16.to_le_bytes());
        cli.extend_from_slice(&metadata_rva.to_le_bytes());
        cli.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        cli.extend_from_slice(&1u32.to_le_bytes()); // ILONLY
        cli.extend_from_slice(&entry.to_le_bytes());
        text[..cli.len()].copy_from_slice(&cli);

        let raw_size = text.len().div_ceil(FILE_ALIGN) * FILE_ALIGN;
        let mut pe = vec![0u8; FILE_ALIGN];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        let mut h = Vec::new();
        h.extend_from_slice(b"PE\0\0");
        h.extend_from_slice(&0x14cu16.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes());
        h.extend_from_slice(&[0; 12]);
        h.extend_from_slice(&0xE0u16.to_le_bytes());
        let characteristics: u16 = if entry == 0 { 0x2102 } else { 0x0102 };
        h.extend_from_slice(&characteristics.to_le_bytes());
        // En-tête optionnel PE32.
        let mut opt = vec![0u8; 0xE0];
        opt[0..2].copy_from_slice(&0x10bu16.to_le_bytes());
        opt[4..8].copy_from_slice(&(raw_size as u32).to_le_bytes());
        opt[28..32].copy_from_slice(&0x0040_0000u32.to_le_bytes());
        opt[32..36].copy_from_slice(&(SECTION_RVA).to_le_bytes());
        opt[36..40].copy_from_slice(&(FILE_ALIGN as u32).to_le_bytes());
        opt[40..42].copy_from_slice(&4u16.to_le_bytes());
        opt[48..50].copy_from_slice(&4u16.to_le_bytes());
        let image_size = SECTION_RVA + (text.len() as u32).div_ceil(0x2000) * 0x2000;
        opt[56..60].copy_from_slice(&image_size.to_le_bytes());
        opt[60..64].copy_from_slice(&(FILE_ALIGN as u32).to_le_bytes());
        opt[68..70].copy_from_slice(&3u16.to_le_bytes());
        opt[92..96].copy_from_slice(&16u32.to_le_bytes());
        let clr_dir = 96 + 14 * 8;
        opt[clr_dir..clr_dir + 4].copy_from_slice(&SECTION_RVA.to_le_bytes());
        opt[clr_dir + 4..clr_dir + 8].copy_from_slice(&72u32.to_le_bytes());
        h.extend_from_slice(&opt);
        let mut section = vec![0u8; 40];
        section[..5].copy_from_slice(b".text");
        section[8..12].copy_from_slice(&(text.len() as u32).to_le_bytes());
        section[12..16].copy_from_slice(&SECTION_RVA.to_le_bytes());
        section[16..20].copy_from_slice(&(raw_size as u32).to_le_bytes());
        section[20..24].copy_from_slice(&(FILE_ALIGN as u32).to_le_bytes());
        section[36..40].copy_from_slice(&0x6000_0020u32.to_le_bytes());
        h.extend_from_slice(&section);
        pe[0x80..0x80 + h.len()].copy_from_slice(&h);
        text.resize(raw_size, 0);
        pe.extend_from_slice(&text);
        pe
    }

    fn metadata(&mut self, methods: &[Method], rvas: &[u32], module: u32, asm: u32) -> Vec<u8> {
        let mut rows: Vec<(usize, Vec<Vec<u8>>)> = Vec::new();
        let w16 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&(x as u16).to_le_bytes());
        let w32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());

        let mut r = Vec::new();
        w16(&mut r, 0);
        w16(&mut r, module);
        w16(&mut r, 1);
        w16(&mut r, 0);
        w16(&mut r, 0);
        rows.push((MODULE, vec![r]));

        let tr = self.type_refs.iter().map(|&(scope, n, ns)| {
            let mut r = Vec::new();
            w16(&mut r, scope);
            w16(&mut r, n);
            w16(&mut r, ns);
            r
        });
        rows.push((TYPE_REF, tr.collect()));

        let td = self.type_defs.iter().map(|&(flags, n, ns, ext, f, m)| {
            let mut r = Vec::new();
            w32(&mut r, flags);
            w16(&mut r, n);
            w16(&mut r, ns);
            w16(&mut r, ext);
            w16(&mut r, f);
            w16(&mut r, m);
            r
        });
        rows.push((TYPE_DEF, td.collect()));

        let fd = self.fields.iter().map(|&(flags, n, s)| {
            let mut r = Vec::new();
            w16(&mut r, flags as u32);
            w16(&mut r, n);
            w16(&mut r, s);
            r
        });
        rows.push((FIELD, fd.collect()));

        let md = methods.iter().zip(rvas).map(|(m, &rva)| {
            let mut r = Vec::new();
            w32(&mut r, rva);
            w16(&mut r, if m.body.is_none() { 0x1000 } else { 0 });
            w16(&mut r, m.flags as u32);
            w16(&mut r, m.name);
            w16(&mut r, m.sig);
            w16(&mut r, 1);
            r
        });
        rows.push((METHOD_DEF, md.collect()));

        let mr = self.member_refs.iter().map(|&(class, n, s)| {
            let mut r = Vec::new();
            w16(&mut r, class);
            w16(&mut r, n);
            w16(&mut r, s);
            r
        });
        rows.push((MEMBER_REF, mr.collect()));

        let ss = self.sigs.iter().map(|&s| {
            let mut r = Vec::new();
            w16(&mut r, s);
            r
        });
        rows.push((STANDALONE_SIG, ss.collect()));

        let ts = self.type_specs.iter().map(|&b| {
            let mut r = Vec::new();
            w16(&mut r, b);
            r
        });
        rows.push((TYPE_SPEC, ts.collect()));

        let mut a = Vec::new();
        w32(&mut a, 0x8004);
        for v in [1, 0, 0, 0] {
            w16(&mut a, v);
        }
        w32(&mut a, 0);
        w16(&mut a, 0);
        w16(&mut a, asm);
        w16(&mut a, 0);
        rows.push((ASSEMBLY, vec![a]));

        let ar = self.assembly_refs.iter().map(|&n| {
            let mut r = Vec::new();
            for v in [4, 0, 0, 0] {
                w16(&mut r, v);
            }
            w32(&mut r, 0);
            w16(&mut r, 0);
            w16(&mut r, n);
            w16(&mut r, 0);
            w16(&mut r, 0);
            r
        });
        rows.push((ASSEMBLY_REF, ar.collect()));

        // GenericParam est trié par propriétaire (§II.22).
        let mut gp = self.generic_params.clone();
        gp.sort_by_key(|&(number, owner, _)| (owner, number));
        let gp = gp.iter().map(|&(number, owner, n)| {
            let mut r = Vec::new();
            w16(&mut r, number as u32);
            w16(&mut r, 0);
            w16(&mut r, owner);
            w16(&mut r, n);
            r
        });
        rows.push((GENERIC_PARAM, gp.collect()));

        let ms = self.method_specs.iter().map(|&(m, b)| {
            let mut r = Vec::new();
            w16(&mut r, m);
            w16(&mut r, b);
            r
        });
        rows.push((METHOD_SPEC, ms.collect()));

        rows.retain(|(_, r)| !r.is_empty());
        let mut tables = Vec::new();
        w32(&mut tables, 0);
        tables.extend_from_slice(&[2, 0, 0, 1]);
        let valid: u64 = rows.iter().map(|(t, _)| 1u64 << t).sum();
        tables.extend_from_slice(&valid.to_le_bytes());
        tables.extend_from_slice(&0u64.to_le_bytes());
        for (_, r) in &rows {
            w32(&mut tables, r.len() as u32);
        }
        for (_, r) in &rows {
            for row in r {
                tables.extend_from_slice(row);
            }
        }
        // Les index de tas et de tables sont écrits sur 2 octets : un témoin
        // reste bien en dessous de 65 536 entrées.
        assert!(self.strings.len() < 0x10000 && self.blobs.len() < 0x10000);

        let streams: [(&str, Vec<u8>); 5] = [
            ("#~", tables),
            ("#Strings", std::mem::take(&mut self.strings)),
            ("#US", std::mem::take(&mut self.user_strings)),
            ("#GUID", vec![0x57; 16]),
            ("#Blob", std::mem::take(&mut self.blobs)),
        ];
        let version = b"v4.0.30319\0\0";
        let mut header_len = 16 + version.len() + 4;
        for (name, _) in &streams {
            header_len += 8 + (name.len() + 4) / 4 * 4;
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"BSJB");
        w16(&mut out, 1);
        w16(&mut out, 1);
        w32(&mut out, 0);
        w32(&mut out, version.len() as u32);
        out.extend_from_slice(version);
        w16(&mut out, 0);
        w16(&mut out, streams.len() as u32);
        let mut offset = header_len;
        let mut bodies = Vec::new();
        for (name, data) in &streams {
            let mut data = data.clone();
            while !data.len().is_multiple_of(4) {
                data.push(0);
            }
            w32(&mut out, offset as u32);
            w32(&mut out, data.len() as u32);
            out.extend_from_slice(name.as_bytes());
            let pad = (name.len() + 4) / 4 * 4 - name.len();
            out.extend(std::iter::repeat_n(0, pad));
            offset += data.len();
            bodies.push(data);
        }
        debug_assert_eq!(out.len(), header_len);
        for b in bodies {
            out.extend_from_slice(&b);
        }
        out
    }
}

/// Entier compressé des signatures (§II.23.2).
pub fn compress(out: &mut Vec<u8>, v: u32) {
    if v < 0x80 {
        out.push(v as u8);
    } else if v < 0x4000 {
        out.extend_from_slice(&((v as u16) | 0x8000).to_be_bytes());
    } else {
        out.extend_from_slice(&(v | 0xC000_0000).to_be_bytes());
    }
}

/// Type d'une classe dans une signature (`CLASS` + TypeDefOrRefEncoded).
pub fn class(token: u32) -> Vec<u8> {
    let tag = if token >> 24 == 0x01 { 1 } else { 0 };
    let mut v = vec![0x12];
    compress(&mut v, ((token & 0xFF_FFFF) << 2) | tag);
    v
}

/// Signature de méthode : `has_this`, type de retour, paramètres.
pub fn method_sig(has_this: bool, ret: &[u8], params: &[&[u8]]) -> Vec<u8> {
    let mut v = vec![if has_this { 0x20 } else { 0x00 }];
    compress(&mut v, params.len() as u32);
    v.extend_from_slice(ret);
    for p in params {
        v.extend_from_slice(p);
    }
    v
}

/// Petit assembleur IL : opcodes utiles aux témoins.
#[derive(Default)]
pub struct Il(pub Vec<u8>);

impl Il {
    pub fn op(mut self, b: u8) -> Il {
        self.0.push(b);
        self
    }
    pub fn tok(mut self, b: u8, token: u32) -> Il {
        self.0.push(b);
        self.0.extend_from_slice(&token.to_le_bytes());
        self
    }
    pub fn i4(mut self, v: i32) -> Il {
        self.0.push(0x20);
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn ldarg0(self) -> Il {
        self.op(0x02)
    }
    pub fn ret(self) -> Il {
        self.op(0x2A)
    }
    pub fn call(self, t: u32) -> Il {
        self.tok(0x28, t)
    }
    pub fn callvirt(self, t: u32) -> Il {
        self.tok(0x6F, t)
    }
    pub fn newobj(self, t: u32) -> Il {
        self.tok(0x73, t)
    }
    pub fn ldfld(self, t: u32) -> Il {
        self.tok(0x7B, t)
    }
    pub fn stfld(self, t: u32) -> Il {
        self.tok(0x7D, t)
    }
    pub fn ldsfld(self, t: u32) -> Il {
        self.tok(0x7E, t)
    }
    pub fn stsfld(self, t: u32) -> Il {
        self.tok(0x80, t)
    }
    pub fn ldstr(self, t: u32) -> Il {
        self.tok(0x72, t)
    }
    pub fn body(self) -> Body {
        Body::new(self.0)
    }
}

/// Signature `GENERICINST CLASS|VALUETYPE type<args>`.
pub fn generic_inst(value_type: bool, def: u32, args: &[&[u8]]) -> Vec<u8> {
    let mut v = vec![0x15];
    let mut inner = class(def);
    if value_type {
        inner[0] = 0x11;
    }
    v.extend_from_slice(&inner);
    compress(&mut v, args.len() as u32);
    for a in args {
        v.extend_from_slice(a);
    }
    v
}

/// Type valeur (`VALUETYPE` + TypeDefOrRefEncoded).
pub fn value_type(token: u32) -> Vec<u8> {
    let mut v = class(token);
    v[0] = 0x11;
    v
}

/// `!n` (paramètre du type) et `!!n` (paramètre de la méthode).
pub fn var(n: u8) -> Vec<u8> {
    vec![0x13, n]
}
pub fn mvar(n: u8) -> Vec<u8> {
    vec![0x1E, n]
}

/// Signature de méthode générique (`GENERIC`, nombre de paramètres de type).
pub fn generic_method_sig(generics: u32, has_this: bool, ret: &[u8], params: &[&[u8]]) -> Vec<u8> {
    let mut v = vec![0x10 | if has_this { 0x20 } else { 0x00 }];
    compress(&mut v, generics);
    compress(&mut v, params.len() as u32);
    v.extend_from_slice(ret);
    for p in params {
        v.extend_from_slice(p);
    }
    v
}
