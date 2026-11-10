//! Analyse complète d'un descripteur de rapports HID et fonctions `HidP_*`
//! de `hid.dll` (#1523).
//!
//! Sources publiques : spécification USB-IF « Device Class Definition for
//! HID 1.11 » (format des éléments du descripteur, tables d'usages) et
//! en-tête `hidpi.h` de mingw-w64 (structures HIDP_CAPS, HIDP_BUTTON_CAPS,
//! HIDP_VALUE_CAPS, HIDP_LINK_COLLECTION_NODE, HIDP_DATA et codes
//! HIDP_STATUS_*). Les logiciels de matériel (éclairage, souris, casques)
//! et hidapi décrivent l'appareil par ces fonctions avant de lui parler.
//!
//! Sous Windows, chaque collection de tête (« top-level collection ») d'un
//! appareil est un périphérique à part, avec ses propres capacités. Le
//! descripteur est donc analysé en entier une fois, puis découpé par
//! collection de tête (`Tlc`).
#![allow(non_snake_case)]

use std::collections::BTreeMap;

pub const HIDP_STATUS_SUCCESS: u32 = 0x0011_0000;
pub const HIDP_STATUS_NULL: u32 = 0x8011_0001;
pub const HIDP_STATUS_INVALID_PREPARSED_DATA: u32 = 0xC011_0001;
pub const HIDP_STATUS_INVALID_REPORT_TYPE: u32 = 0xC011_0002;
pub const HIDP_STATUS_INVALID_REPORT_LENGTH: u32 = 0xC011_0003;
pub const HIDP_STATUS_USAGE_NOT_FOUND: u32 = 0xC011_0004;
pub const HIDP_STATUS_VALUE_OUT_OF_RANGE: u32 = 0xC011_0005;
pub const HIDP_STATUS_BAD_LOG_PHY_VALUES: u32 = 0xC011_0006;
pub const HIDP_STATUS_BUFFER_TOO_SMALL: u32 = 0xC011_0007;
pub const HIDP_STATUS_INCOMPATIBLE_REPORT_ID: u32 = 0xC011_000A;
pub const HIDP_STATUS_NOT_VALUE_ARRAY: u32 = 0xC011_000B;
pub const HIDP_STATUS_DATA_INDEX_NOT_FOUND: u32 = 0xC011_000D;
pub const HIDP_STATUS_BUTTON_NOT_PRESSED: u32 = 0xC011_000F;
pub const HIDP_STATUS_REPORT_DOES_NOT_EXIST: u32 = 0xC011_0010;
pub const HIDP_STATUS_NOT_IMPLEMENTED: u32 = 0xC011_0020;

/// Plage d'usages (bornes incluses) ; `min == max` pour un usage seul.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UsageRange {
    pub page: u16,
    pub min: u16,
    pub max: u16,
}

/// Un élément principal Input/Output/Feature, tel que déclaré.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Field {
    /// 0 Input, 1 Output, 2 Feature (HIDP_REPORT_TYPE).
    pub kind: u8,
    pub report_id: u8,
    /// Position du premier bit dans le rapport, octet d'identifiant compris.
    pub bit_offset: u32,
    pub bit_size: u16,
    pub count: u16,
    /// Bits de l'élément principal (constante, variable, relatif, null…).
    pub flags: u16,
    pub usages: Vec<UsageRange>,
    pub logical_min: i32,
    pub logical_max: i32,
    pub physical_min: i32,
    pub physical_max: i32,
    pub unit: u32,
    pub unit_exp: u32,
    pub string: (u16, u16),
    pub designator: (u16, u16),
    /// Collection de rattachement (indice dans `Descriptor::collections`).
    pub collection: usize,
}

impl Field {
    pub fn constant(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn variable(&self) -> bool {
        self.flags & 2 != 0
    }
    pub fn absolute(&self) -> bool {
        self.flags & 4 == 0
    }
    pub fn has_null(&self) -> bool {
        self.flags & 0x40 != 0
    }
    /// Bouton pour Windows : tableau (sélecteurs) ou variable d'un bit.
    pub fn is_button(&self) -> bool {
        !self.variable() || self.bit_size == 1
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Collection {
    pub usage_page: u16,
    pub usage: u16,
    pub kind: u8,
    pub parent: Option<usize>,
    /// Collection de tête à laquelle elle appartient (0 pour la première).
    pub tlc: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Descriptor {
    pub collections: Vec<Collection>,
    pub fields: Vec<Field>,
    /// Indices (dans `collections`) des collections de tête.
    pub tlcs: Vec<usize>,
    pub report_ids: bool,
}

/// État global de l'analyseur (empilé par Push/Pop).
#[derive(Clone, Copy, Default)]
struct Globals {
    page: u32,
    logical_min: i32,
    logical_max: i32,
    physical_min: i32,
    physical_max: i32,
    unit_exp: u32,
    unit: u32,
    size: u32,
    id: u32,
    count: u32,
}

fn signed(value: u32, size: usize) -> i32 {
    match size {
        1 => value as u8 as i8 as i32,
        2 => value as u16 as i16 as i32,
        _ => value as i32,
    }
}

pub fn parse(d: &[u8]) -> Descriptor {
    let mut out = Descriptor::default();
    let mut g = Globals::default();
    let mut stack: Vec<Globals> = Vec::new();
    // Locaux : usages (liste ou plages), chaînes, désignateurs.
    let mut usages: Vec<UsageRange> = Vec::new();
    let mut usage_min: Option<u32> = None;
    let mut strings = (0u16, 0u16);
    let mut designators = (0u16, 0u16);
    let mut open: Vec<usize> = Vec::new();
    // Bits déjà placés par (type, identifiant).
    let mut offsets: BTreeMap<(u8, u8), u32> = BTreeMap::new();
    let page_of = |raw: u32, size: usize, page: u32| -> (u16, u16) {
        if size == 4 {
            ((raw >> 16) as u16, raw as u16)
        } else {
            (page as u16, raw as u16)
        }
    };
    let mut i = 0;
    while i < d.len() {
        let prefix = d[i];
        if prefix == 0xFE {
            let n = d.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + n;
            continue;
        }
        let size = match prefix & 3 {
            3 => 4,
            s => s as usize,
        };
        let mut value = 0u32;
        for k in 0..size {
            value |= (d.get(i + 1 + k).copied().unwrap_or(0) as u32) << (8 * k);
        }
        let (kind, tag) = ((prefix >> 2) & 3, prefix >> 4);
        let mut clear_locals = false;
        match (kind, tag) {
            (0, 8) | (0, 9) | (0, 11) => {
                let t = match tag {
                    8 => 0u8,
                    9 => 1,
                    _ => 2,
                };
                let id = g.id as u8;
                let at = offsets.entry((t, id)).or_insert(0);
                let bits = g.size * g.count;
                let mut list = usages.clone();
                if list.is_empty() {
                    if let Some(min) = usage_min {
                        let (p, u) = page_of(min, 2, g.page);
                        list.push(UsageRange {
                            page: p,
                            min: u,
                            max: u,
                        });
                    }
                }
                out.fields.push(Field {
                    kind: t,
                    report_id: id,
                    bit_offset: 8 * u32::from(out.report_ids || g.id != 0) + *at,
                    bit_size: g.size as u16,
                    count: g.count as u16,
                    flags: value as u16,
                    usages: list,
                    logical_min: g.logical_min,
                    logical_max: g.logical_max,
                    physical_min: g.physical_min,
                    physical_max: g.physical_max,
                    unit: g.unit,
                    unit_exp: g.unit_exp,
                    string: strings,
                    designator: designators,
                    collection: open.last().copied().unwrap_or(0),
                });
                *at += bits;
                clear_locals = true;
            }
            (0, 10) => {
                let (page, usage) = usages
                    .first()
                    .map(|u| (u.page, u.min))
                    .unwrap_or((g.page as u16, 0));
                let parent = open.last().copied();
                let index = out.collections.len();
                let tlc = match parent {
                    Some(p) => out.collections[p].tlc,
                    None => {
                        out.tlcs.push(index);
                        out.tlcs.len() - 1
                    }
                };
                out.collections.push(Collection {
                    usage_page: page,
                    usage,
                    kind: value as u8,
                    parent,
                    tlc,
                });
                open.push(index);
                clear_locals = true;
            }
            (0, 12) => {
                open.pop();
                clear_locals = true;
            }
            (1, 0) => g.page = value,
            (1, 1) => g.logical_min = signed(value, size),
            (1, 2) => {
                g.logical_max = signed(value, size);
                // Maximum non signé quand le minimum est positif (usage courant
                // `26 FF 00` : 255 et non -1).
                if g.logical_min >= 0 && g.logical_max < 0 {
                    g.logical_max = value as i32;
                }
            }
            (1, 3) => g.physical_min = signed(value, size),
            (1, 4) => {
                g.physical_max = signed(value, size);
                if g.physical_min >= 0 && g.physical_max < 0 {
                    g.physical_max = value as i32;
                }
            }
            (1, 5) => g.unit_exp = value,
            (1, 6) => g.unit = value,
            (1, 7) => g.size = value,
            (1, 8) => {
                g.id = value;
                out.report_ids = true;
            }
            (1, 9) => g.count = value,
            (1, 10) => stack.push(g),
            (1, 11) => {
                if let Some(s) = stack.pop() {
                    g = s;
                }
            }
            (2, 0) => {
                let (p, u) = page_of(value, size, g.page);
                usages.push(UsageRange {
                    page: p,
                    min: u,
                    max: u,
                });
            }
            (2, 1) => usage_min = Some(value | if size == 4 { 0 } else { g.page << 16 }),
            (2, 2) => {
                if let Some(min) = usage_min.take() {
                    let (p, lo) = ((min >> 16) as u16, min as u16);
                    let hi = value as u16;
                    usages.push(UsageRange {
                        page: if size == 4 { (value >> 16) as u16 } else { p },
                        min: lo,
                        max: hi.max(lo),
                    });
                }
            }
            (2, 4) => strings = (value as u16, value as u16),
            (2, 5) => strings.0 = value as u16,
            (2, 6) => strings.1 = value as u16,
            (2, 3) => designators = (value as u16, value as u16),
            (2, 7) => designators.0 = value as u16,
            (2, 8) => designators.1 = value as u16,
            _ => {}
        }
        if clear_locals {
            usages.clear();
            usage_min = None;
            strings = (0, 0);
            designators = (0, 0);
        }
        i += 1 + size;
    }
    out
}

// ============================================================
//  Données préanalysées d'une collection de tête
// ============================================================

/// Un « cap » Windows : une entrée de HidP_GetButtonCaps/GetValueCaps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cap {
    pub field: usize,
    pub kind: u8,
    pub button: bool,
    pub report_id: u8,
    pub page: u16,
    pub range: (u16, u16),
    pub is_range: bool,
    /// Premier bit de ce cap et nombre d'éléments qu'il couvre.
    pub bit_offset: u32,
    pub count: u16,
    pub data_index: (u16, u16),
    /// Collection de liaison (indice local à la collection de tête).
    pub link: u16,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkNode {
    pub usage: u16,
    pub page: u16,
    pub parent: u16,
    pub children: u16,
    pub next_sibling: u16,
    pub first_child: u16,
    pub kind: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tlc {
    pub usage_page: u16,
    pub usage: u16,
    /// Longueurs des rapports Input, Output, Feature (octet d'identifiant
    /// compris), 0 sans rapport de ce type.
    pub lengths: [u16; 3],
    pub fields: Vec<Field>,
    pub caps: Vec<Cap>,
    pub nodes: Vec<LinkNode>,
    /// Identifiants des rapports d'entrée de cette collection.
    pub input_ids: Vec<u8>,
    pub report_ids: bool,
}

impl Tlc {
    pub fn data_indices(&self, kind: u8) -> u16 {
        self.caps
            .iter()
            .filter(|c| c.kind == kind)
            .map(|c| c.data_index.1 + 1)
            .max()
            .unwrap_or(0)
    }
    pub fn count_caps(&self, kind: u8, button: bool) -> u16 {
        self.caps
            .iter()
            .filter(|c| c.kind == kind && c.button == button)
            .count() as u16
    }
}

/// Découpe le descripteur en collections de tête, avec leurs caps.
pub fn split(desc: &Descriptor) -> Vec<Tlc> {
    let mut out = Vec::new();
    for (t, &root) in desc.tlcs.iter().enumerate() {
        // Collections de cette tête, réindexées dans l'ordre d'apparition.
        let members: Vec<usize> = (0..desc.collections.len())
            .filter(|&c| desc.collections[c].tlc == t)
            .collect();
        let local = |c: usize| members.iter().position(|&m| m == c).unwrap_or(0) as u16;
        let mut nodes: Vec<LinkNode> = members
            .iter()
            .map(|&c| {
                let col = &desc.collections[c];
                LinkNode {
                    usage: col.usage,
                    page: col.usage_page,
                    parent: col.parent.map(local).unwrap_or(0),
                    kind: col.kind,
                    ..Default::default()
                }
            })
            .collect();
        // Enfants : Windows chaîne les frères du dernier au premier.
        for i in (1..nodes.len()).rev() {
            let p = nodes[i].parent as usize;
            let previous_first = nodes[p].first_child;
            nodes[i].next_sibling = previous_first;
            nodes[p].first_child = i as u16;
            nodes[p].children += 1;
        }
        let fields: Vec<Field> = desc
            .fields
            .iter()
            .filter(|f| desc.collections.get(f.collection).map(|c| c.tlc) == Some(t))
            .cloned()
            .collect();
        let mut caps = Vec::new();
        let mut next_index = [0u16; 3];
        for (fi, f) in fields.iter().enumerate() {
            if f.constant() || f.count == 0 {
                continue;
            }
            let k = f.kind as usize;
            let link = local(f.collection);
            let button = f.is_button();
            let mut push =
                |caps: &mut Vec<Cap>, r: UsageRange, is_range: bool, off: u32, n: u16| {
                    let span = if is_range { r.max - r.min + 1 } else { 1 };
                    let first = next_index[k];
                    next_index[k] += span;
                    caps.push(Cap {
                        field: fi,
                        kind: f.kind,
                        button,
                        report_id: f.report_id,
                        page: r.page,
                        range: (r.min, r.max),
                        is_range,
                        bit_offset: off,
                        count: n,
                        data_index: (first, first + span - 1),
                        link,
                    });
                };
            let usages = if f.usages.is_empty() {
                vec![UsageRange::default()]
            } else {
                f.usages.clone()
            };
            if !f.variable() {
                // Tableau : un cap par plage de sélecteurs, tous sur les mêmes bits.
                for r in usages {
                    push(&mut caps, r, r.min != r.max, f.bit_offset, f.count);
                }
                continue;
            }
            // Variable : chaque élément du rapport reçoit l'usage suivant de la
            // liste (une plage se déroule), le dernier usage prend le reste.
            let mut slot = 0u16;
            let n = usages.len();
            for (ui, r) in usages.iter().enumerate() {
                if slot >= f.count {
                    break;
                }
                let span = r.max - r.min + 1;
                let last = ui + 1 == n;
                let take = if last {
                    f.count - slot
                } else {
                    span.min(f.count - slot)
                };
                let off = f.bit_offset + u32::from(slot) * u32::from(f.bit_size);
                if r.min != r.max {
                    let rr = UsageRange {
                        page: r.page,
                        min: r.min,
                        max: r.min + take.min(span) - 1,
                    };
                    push(&mut caps, rr, true, off, take);
                } else {
                    push(&mut caps, *r, false, off, take);
                }
                slot += take;
            }
        }
        // Longueurs : plus grand rapport de chaque type dans cette tête.
        let mut lengths = [0u16; 3];
        let mut input_ids = Vec::new();
        for kind in 0..3u8 {
            let mut per_id: BTreeMap<u8, u32> = BTreeMap::new();
            for f in desc.fields.iter().filter(|f| f.kind == kind) {
                let end = f.bit_offset + u32::from(f.bit_size) * u32::from(f.count);
                let e = per_id.entry(f.report_id).or_insert(0);
                *e = (*e).max(end);
            }
            for f in fields.iter().filter(|f| f.kind == kind) {
                let end = per_id.get(&f.report_id).copied().unwrap_or(0);
                // Sans identifiant, Windows ajoute quand même l'octet 0 en tête.
                let bytes = if desc.report_ids {
                    end.div_ceil(8)
                } else {
                    end.div_ceil(8) + 1
                };
                lengths[kind as usize] = lengths[kind as usize].max(bytes as u16);
                if kind == 0 && !input_ids.contains(&f.report_id) {
                    input_ids.push(f.report_id);
                }
            }
        }
        let col = &desc.collections[root];
        out.push(Tlc {
            usage_page: col.usage_page,
            usage: col.usage,
            lengths,
            fields,
            caps,
            nodes,
            input_ids,
            report_ids: desc.report_ids,
        });
    }
    if out.is_empty() {
        out.push(Tlc::default());
    }
    out
}

// ============================================================
//  Lecture et écriture de bits dans un rapport
// ============================================================

/// Décalage en bits dans le tampon Windows (octet 0 = identifiant). Les
/// rapports sans identifiant ont quand même leur octet 0 côté Windows.
fn win_offset(tlc: &Tlc, bit_offset: u32) -> u32 {
    if tlc.report_ids {
        bit_offset
    } else {
        bit_offset + 8
    }
}

fn get_bits(report: &[u8], at: u32, size: u16) -> u32 {
    let mut v = 0u32;
    for b in 0..u32::from(size.min(32)) {
        let bit = at + b;
        let byte = report.get((bit / 8) as usize).copied().unwrap_or(0);
        if byte >> (bit % 8) & 1 != 0 {
            v |= 1 << b;
        }
    }
    v
}

fn set_bits(report: &mut [u8], at: u32, size: u16, value: u32) {
    for b in 0..u32::from(size.min(32)) {
        let bit = at + b;
        if let Some(byte) = report.get_mut((bit / 8) as usize) {
            if value >> b & 1 != 0 {
                *byte |= 1 << (bit % 8);
            } else {
                *byte &= !(1 << (bit % 8));
            }
        }
    }
}

fn sign_extend(v: u32, size: u16) -> i32 {
    if size == 0 || size >= 32 {
        return v as i32;
    }
    let shift = 32 - u32::from(size);
    ((v << shift) as i32) >> shift
}

// ============================================================
//  Accès aux données préanalysées (pointeur rendu au programme)
// ============================================================

#[repr(C)]
pub struct Preparsed {
    magic: u64,
    pub tlc: Tlc,
}
const PREPARSED_MAGIC: u64 = 0x5749_4E4C_5848_4944; // « WINLXHID »

pub fn preparsed_into_raw(tlc: Tlc) -> usize {
    Box::into_raw(Box::new(Preparsed {
        magic: PREPARSED_MAGIC,
        tlc,
    })) as usize
}

pub unsafe fn preparsed_free(data: usize) -> bool {
    let p = data as *mut Preparsed;
    if p.is_null() || (*p).magic != PREPARSED_MAGIC {
        return false;
    }
    (*p).magic = 0;
    drop(Box::from_raw(p));
    true
}

unsafe fn tlc<'a>(data: usize) -> Option<&'a Tlc> {
    let p = data as *const Preparsed;
    if p.is_null() || (*p).magic != PREPARSED_MAGIC {
        return None;
    }
    Some(&(*p).tlc)
}

unsafe fn report<'a>(report: *mut u8, len: u32) -> Option<&'a mut [u8]> {
    if report.is_null() || len == 0 {
        return None;
    }
    Some(std::slice::from_raw_parts_mut(report, len as usize))
}

fn put16(out: *mut u8, at: usize, v: u16) {
    unsafe { (out.add(at) as *mut u16).write_unaligned(v) }
}
fn put32(out: *mut u8, at: usize, v: u32) {
    unsafe { (out.add(at) as *mut u32).write_unaligned(v) }
}

// ============================================================
//  hidpi.dll : capacités
// ============================================================

/// HIDP_CAPS (64 octets).
pub unsafe extern "win64" fn HidP_GetCaps(data: usize, out: *mut u8) -> u32 {
    let Some(t) = tlc(data) else {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    };
    if out.is_null() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    std::ptr::write_bytes(out, 0, 64);
    put16(out, 0, t.usage);
    put16(out, 2, t.usage_page);
    put16(out, 4, t.lengths[0]);
    put16(out, 6, t.lengths[1]);
    put16(out, 8, t.lengths[2]);
    put16(out, 44, t.nodes.len() as u16);
    for k in 0..3u8 {
        let at = 46 + 6 * k as usize;
        put16(out, at, t.count_caps(k, true));
        put16(out, at + 2, t.count_caps(k, false));
        put16(out, at + 4, t.data_indices(k));
    }
    HIDP_STATUS_SUCCESS
}

/// Écrit un HIDP_BUTTON_CAPS ou HIDP_VALUE_CAPS (72 octets).
unsafe fn write_cap(t: &Tlc, c: &Cap, out: *mut u8) {
    let f = &t.fields[c.field];
    std::ptr::write_bytes(out, 0, 72);
    put16(out, 0, c.page);
    *out.add(2) = c.report_id;
    *out.add(3) = 0; // IsAlias
    put16(out, 4, f.flags);
    put16(out, 6, c.link);
    let node = t.nodes.get(c.link as usize).cloned().unwrap_or_default();
    put16(out, 8, node.usage);
    put16(out, 10, node.page);
    *out.add(12) = u8::from(c.is_range);
    *out.add(13) = u8::from(f.string.0 != f.string.1);
    *out.add(14) = u8::from(f.designator.0 != f.designator.1);
    *out.add(15) = u8::from(f.absolute());
    if c.button {
        put16(out, 16, c.count); // ReportCount
    } else {
        *out.add(16) = u8::from(f.has_null());
        put16(out, 18, f.bit_size);
        put16(out, 20, c.count);
        put32(out, 32, f.unit_exp);
        put32(out, 36, f.unit);
        put32(out, 40, f.logical_min as u32);
        put32(out, 44, f.logical_max as u32);
        put32(out, 48, f.physical_min as u32);
        put32(out, 52, f.physical_max as u32);
    }
    // Union Range / NotRange.
    put16(out, 56, c.range.0);
    put16(out, 58, if c.is_range { c.range.1 } else { 0 });
    put16(out, 60, f.string.0);
    put16(out, 62, if c.is_range { f.string.1 } else { 0 });
    put16(out, 64, f.designator.0);
    put16(out, 66, if c.is_range { f.designator.1 } else { 0 });
    put16(out, 68, c.data_index.0);
    put16(out, 70, if c.is_range { c.data_index.1 } else { 0 });
}

fn cap_matches(c: &Cap, page: u16, link: u16, usage: u16) -> bool {
    (page == 0 || c.page == page)
        && (link == 0 || c.link == link)
        && (usage == 0 || (c.range.0..=c.range.1).contains(&usage))
}

unsafe fn get_caps(
    kind: u32,
    button: bool,
    filter: (u16, u16, u16),
    out: *mut u8,
    len: *mut u16,
    data: usize,
) -> u32 {
    let Some(t) = tlc(data) else {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    };
    if kind > 2 {
        return HIDP_STATUS_INVALID_REPORT_TYPE;
    }
    if len.is_null() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    let found: Vec<&Cap> = t
        .caps
        .iter()
        .filter(|c| c.kind == kind as u8 && c.button == button)
        .filter(|c| cap_matches(c, filter.0, filter.1, filter.2))
        .collect();
    let room = *len as usize;
    let mut written = 0;
    if !out.is_null() {
        for c in found.iter().take(room) {
            write_cap(t, c, out.add(72 * written));
            written += 1;
        }
    }
    *len = written as u16;
    if found.is_empty() {
        HIDP_STATUS_USAGE_NOT_FOUND
    } else {
        HIDP_STATUS_SUCCESS
    }
}

pub unsafe extern "win64" fn HidP_GetButtonCaps(
    kind: u32,
    out: *mut u8,
    len: *mut u16,
    data: usize,
) -> u32 {
    get_caps(kind, true, (0, 0, 0), out, len, data)
}
pub unsafe extern "win64" fn HidP_GetValueCaps(
    kind: u32,
    out: *mut u8,
    len: *mut u16,
    data: usize,
) -> u32 {
    get_caps(kind, false, (0, 0, 0), out, len, data)
}
pub unsafe extern "win64" fn HidP_GetSpecificButtonCaps(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    out: *mut u8,
    len: *mut u16,
    data: usize,
) -> u32 {
    get_caps(
        kind,
        true,
        (page as u16, link as u16, usage as u16),
        out,
        len,
        data,
    )
}
pub unsafe extern "win64" fn HidP_GetSpecificValueCaps(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    out: *mut u8,
    len: *mut u16,
    data: usize,
) -> u32 {
    get_caps(
        kind,
        false,
        (page as u16, link as u16, usage as u16),
        out,
        len,
        data,
    )
}

/// HIDP_LINK_COLLECTION_NODE : 12 octets de champs, un ULONG de bits
/// (CollectionType, IsAlias), puis UserContext (pointeur) : 24 octets en
/// 64 bits, 20 en 32 bits.
pub unsafe extern "win64" fn HidP_GetLinkCollectionNodes(
    out: *mut u8,
    len: *mut u32,
    data: usize,
) -> u32 {
    let Some(t) = tlc(data) else {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    };
    if len.is_null() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    let need = t.nodes.len() as u32;
    if *len < need || out.is_null() {
        *len = need;
        return HIDP_STATUS_BUFFER_TOO_SMALL;
    }
    let size = 16 + crate::wow32::pointer_size();
    for (i, n) in t.nodes.iter().enumerate() {
        let p = out.add(i * size);
        std::ptr::write_bytes(p, 0, size);
        put16(p, 0, n.usage);
        put16(p, 2, n.page);
        put16(p, 4, n.parent);
        put16(p, 6, n.children);
        put16(p, 8, n.next_sibling);
        put16(p, 10, n.first_child);
        put32(p, 12, u32::from(n.kind));
    }
    *len = need;
    HIDP_STATUS_SUCCESS
}

pub unsafe extern "win64" fn HidP_MaxUsageListLength(kind: u32, page: u32, data: usize) -> u32 {
    let Some(t) = tlc(data) else {
        return 0;
    };
    t.caps
        .iter()
        .filter(|c| c.kind == kind as u8 && c.button && (page as u16 == 0 || c.page == page as u16))
        .map(|c| {
            if t.fields[c.field].variable() {
                u32::from(c.count)
            } else {
                u32::from(c.count).min(u32::from(c.range.1 - c.range.0 + 1))
            }
        })
        .sum()
}

pub unsafe extern "win64" fn HidP_MaxDataListLength(kind: u32, data: usize) -> u32 {
    match tlc(data) {
        Some(t) if kind <= 2 => u32::from(t.data_indices(kind as u8)),
        _ => 0,
    }
}

// ============================================================
//  hidpi.dll : boutons
// ============================================================

/// Usages des boutons enfoncés dans un rapport : (page, usage, indice de donnée).
fn pressed(
    t: &Tlc,
    kind: u8,
    rep: &[u8],
    page: u16,
    link: u16,
) -> Result<Vec<(u16, u16, u16)>, u32> {
    let id = rep[0];
    let mut any_id = false;
    let mut out = Vec::new();
    for c in t.caps.iter().filter(|c| c.kind == kind && c.button) {
        if t.report_ids && c.report_id != id {
            continue;
        }
        any_id = true;
        if (page != 0 && c.page != page) || (link != 0 && c.link != link) {
            continue;
        }
        let f = &t.fields[c.field];
        if f.variable() {
            for k in 0..c.count {
                let at = win_offset(t, c.bit_offset + u32::from(k));
                if get_bits(rep, at, 1) != 0 {
                    let u = if c.is_range { c.range.0 + k } else { c.range.0 };
                    let di = if c.is_range {
                        c.data_index.0 + k
                    } else {
                        c.data_index.0
                    };
                    out.push((c.page, u, di));
                }
            }
        } else {
            // Tableau : chaque case contient un sélecteur. La valeur hors des
            // bornes logiques (souvent 0 quand elles commencent à 1) veut
            // dire « aucun » ; sinon le sélecteur v donne l'usage
            // min + (v - logical_min).
            for k in 0..c.count {
                let at = win_offset(t, c.bit_offset + u32::from(k) * u32::from(f.bit_size));
                let raw = get_bits(rep, at, f.bit_size);
                let v = if f.logical_min < 0 {
                    i64::from(sign_extend(raw, f.bit_size))
                } else {
                    i64::from(raw)
                };
                if v < i64::from(f.logical_min) || v > i64::from(f.logical_max) {
                    continue;
                }
                if raw == 0 && f.logical_min == 0 && c.range.0 == 0 {
                    continue; // usage 0 : « aucun événement » (page Clavier)
                }
                let u = i64::from(c.range.0) + v - i64::from(f.logical_min);
                if u > i64::from(c.range.1) {
                    continue;
                }
                let di = c.data_index.0 + (u - i64::from(c.range.0)) as u16;
                if !out.iter().any(|x| x.1 == u as u16 && x.0 == c.page) {
                    out.push((c.page, u as u16, di));
                }
            }
        }
    }
    if !any_id {
        return Err(HIDP_STATUS_INCOMPATIBLE_REPORT_ID);
    }
    Ok(out)
}

unsafe fn check_report<'a>(
    data: usize,
    kind: u32,
    rep: *mut u8,
    len: u32,
) -> Result<(&'a Tlc, &'a mut [u8]), u32> {
    let t = tlc(data).ok_or(HIDP_STATUS_INVALID_PREPARSED_DATA)?;
    if kind > 2 {
        return Err(HIDP_STATUS_INVALID_REPORT_TYPE);
    }
    let r = report(rep, len).ok_or(HIDP_STATUS_INVALID_REPORT_LENGTH)?;
    if t.lengths[kind as usize] == 0 {
        return Err(HIDP_STATUS_REPORT_DOES_NOT_EXIST);
    }
    if r.len() < t.lengths[kind as usize] as usize {
        return Err(HIDP_STATUS_INVALID_REPORT_LENGTH);
    }
    Ok((t, r))
}

pub unsafe extern "win64" fn HidP_GetUsages(
    kind: u32,
    page: u32,
    link: u32,
    list: *mut u16,
    len: *mut u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if len.is_null() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    let found = match pressed(t, kind as u8, r, page as u16, link as u16) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let room = *len as usize;
    *len = found.len() as u32;
    if found.len() > room || (list.is_null() && !found.is_empty()) {
        return HIDP_STATUS_BUFFER_TOO_SMALL;
    }
    for (i, (_, u, _)) in found.iter().enumerate() {
        *list.add(i) = *u;
    }
    HIDP_STATUS_SUCCESS
}

/// USAGE_AND_PAGE : Usage puis UsagePage.
pub unsafe extern "win64" fn HidP_GetUsagesEx(
    kind: u32,
    link: u32,
    list: *mut u16,
    len: *mut u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if len.is_null() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    let found = match pressed(t, kind as u8, r, 0, link as u16) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let room = *len as usize;
    *len = found.len() as u32;
    if found.len() > room || (list.is_null() && !found.is_empty()) {
        return HIDP_STATUS_BUFFER_TOO_SMALL;
    }
    for (i, (p, u, _)) in found.iter().enumerate() {
        *list.add(2 * i) = *u;
        *list.add(2 * i + 1) = *p;
    }
    HIDP_STATUS_SUCCESS
}

/// Allume (`on`) ou éteint des boutons dans un rapport.
fn set_buttons(
    t: &Tlc,
    kind: u8,
    r: &mut [u8],
    page: u16,
    link: u16,
    usages: &[u16],
    on: bool,
) -> u32 {
    for &u in usages {
        if u == 0 {
            continue;
        }
        let Some(c) = t.caps.iter().find(|c| {
            c.kind == kind
                && c.button
                && c.page == page
                && (link == 0 || c.link == link)
                && (c.range.0..=c.range.1).contains(&u)
        }) else {
            return HIDP_STATUS_USAGE_NOT_FOUND;
        };
        if t.report_ids {
            if r[0] != 0 && r[0] != c.report_id {
                return HIDP_STATUS_INCOMPATIBLE_REPORT_ID;
            }
            r[0] = c.report_id;
        }
        let f = &t.fields[c.field];
        if f.variable() {
            let k = u32::from(u - c.range.0);
            set_bits(r, win_offset(t, c.bit_offset + k), 1, u32::from(on));
            continue;
        }
        let selector = (i64::from(u - c.range.0) + i64::from(f.logical_min)) as u32;
        let slots: Vec<u32> = (0..u32::from(c.count))
            .map(|k| win_offset(t, c.bit_offset + k * u32::from(f.bit_size)))
            .collect();
        if on {
            if slots
                .iter()
                .any(|&at| get_bits(r, at, f.bit_size) == selector)
            {
                continue;
            }
            match slots.iter().find(|&&at| get_bits(r, at, f.bit_size) == 0) {
                Some(&at) => set_bits(r, at, f.bit_size, selector),
                None => return HIDP_STATUS_BUFFER_TOO_SMALL,
            }
        } else {
            match slots
                .iter()
                .find(|&&at| get_bits(r, at, f.bit_size) == selector)
            {
                Some(&at) => set_bits(r, at, f.bit_size, 0),
                None => return HIDP_STATUS_BUTTON_NOT_PRESSED,
            }
        }
    }
    HIDP_STATUS_SUCCESS
}

pub unsafe extern "win64" fn HidP_SetUsages(
    kind: u32,
    page: u32,
    link: u32,
    list: *mut u16,
    len: *mut u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if list.is_null() || len.is_null() {
        return HIDP_STATUS_SUCCESS;
    }
    let usages = std::slice::from_raw_parts(list, *len as usize);
    set_buttons(t, kind as u8, r, page as u16, link as u16, usages, true)
}

pub unsafe extern "win64" fn HidP_UnsetUsages(
    kind: u32,
    page: u32,
    link: u32,
    list: *mut u16,
    len: *mut u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if list.is_null() || len.is_null() {
        return HIDP_STATUS_SUCCESS;
    }
    let usages = std::slice::from_raw_parts(list, *len as usize);
    set_buttons(t, kind as u8, r, page as u16, link as u16, usages, false)
}

/// Différence entre deux listes d'usages (touches relâchées, enfoncées).
pub unsafe extern "win64" fn HidP_UsageListDifference(
    previous: *const u16,
    current: *const u16,
    broken: *mut u16,
    made: *mut u16,
    len: u32,
) -> u32 {
    if previous.is_null() || current.is_null() || broken.is_null() || made.is_null() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    let n = len as usize;
    let list = |p: *const u16| -> Vec<u16> {
        std::slice::from_raw_parts(p, n)
            .iter()
            .copied()
            .take_while(|&u| u != 0)
            .collect()
    };
    let (prev, cur) = (list(previous), list(current));
    let gone: Vec<u16> = prev.iter().copied().filter(|u| !cur.contains(u)).collect();
    let new: Vec<u16> = cur.iter().copied().filter(|u| !prev.contains(u)).collect();
    for i in 0..n {
        *broken.add(i) = gone.get(i).copied().unwrap_or(0);
        *made.add(i) = new.get(i).copied().unwrap_or(0);
    }
    HIDP_STATUS_SUCCESS
}

// ============================================================
//  hidpi.dll : valeurs
// ============================================================

fn find_value(
    t: &Tlc,
    kind: u8,
    page: u16,
    link: u16,
    usage: u16,
    id: u8,
) -> Result<(&Cap, u32), u32> {
    let mut wrong_id = false;
    for c in t.caps.iter().filter(|c| c.kind == kind && !c.button) {
        if c.page != page || (link != 0 && c.link != link) {
            continue;
        }
        if !(c.range.0..=c.range.1).contains(&usage) {
            continue;
        }
        if t.report_ids && c.report_id != id {
            wrong_id = true;
            continue;
        }
        let f = &t.fields[c.field];
        // Dans une plage, chaque usage occupe un élément ; sinon, le cap
        // couvre `count` éléments (tableau de valeurs).
        let k = u32::from(usage - c.range.0);
        let at = c.bit_offset + k * u32::from(f.bit_size);
        return Ok((c, at));
    }
    Err(if wrong_id {
        HIDP_STATUS_INCOMPATIBLE_REPORT_ID
    } else {
        HIDP_STATUS_USAGE_NOT_FOUND
    })
}

pub unsafe extern "win64" fn HidP_GetUsageValue(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    value: *mut u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    match find_value(t, kind as u8, page as u16, link as u16, usage as u16, r[0]) {
        Ok((c, at)) => {
            let f = &t.fields[c.field];
            if !value.is_null() {
                *value = get_bits(r, win_offset(t, at), f.bit_size);
            }
            HIDP_STATUS_SUCCESS
        }
        Err(e) => e,
    }
}

pub unsafe extern "win64" fn HidP_SetUsageValue(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    value: u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let want = if t.report_ids && r[0] == 0 {
        // Identifiant pas encore posé : celui du premier cap trouvé.
        t.caps
            .iter()
            .find(|c| {
                c.kind == kind as u8
                    && !c.button
                    && c.page == page as u16
                    && (c.range.0..=c.range.1).contains(&(usage as u16))
            })
            .map(|c| c.report_id)
            .unwrap_or(0)
    } else {
        r[0]
    };
    match find_value(t, kind as u8, page as u16, link as u16, usage as u16, want) {
        Ok((c, at)) => {
            if t.report_ids {
                r[0] = c.report_id;
            }
            let f = &t.fields[c.field];
            set_bits(r, win_offset(t, at), f.bit_size, value);
            HIDP_STATUS_SUCCESS
        }
        Err(e) => e,
    }
}

/// Bornes physiques (les logiques quand le descripteur n'en donne pas,
/// comme le prévoit la spécification HID).
fn physical(f: &Field) -> (i32, i32) {
    if f.physical_min == 0 && f.physical_max == 0 {
        (f.logical_min, f.logical_max)
    } else {
        (f.physical_min, f.physical_max)
    }
}

pub unsafe extern "win64" fn HidP_GetScaledUsageValue(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    value: *mut i32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let (c, at) = match find_value(t, kind as u8, page as u16, link as u16, usage as u16, r[0]) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let f = &t.fields[c.field];
    if f.logical_min >= f.logical_max {
        return HIDP_STATUS_BAD_LOG_PHY_VALUES;
    }
    let raw = get_bits(r, win_offset(t, at), f.bit_size);
    let v = if f.logical_min < 0 {
        sign_extend(raw, f.bit_size)
    } else {
        raw as i32
    };
    if v < f.logical_min || v > f.logical_max {
        if !value.is_null() {
            *value = 0;
        }
        return if f.has_null() {
            HIDP_STATUS_NULL
        } else {
            HIDP_STATUS_VALUE_OUT_OF_RANGE
        };
    }
    let (pmin, pmax) = physical(f);
    let scaled = i64::from(pmin)
        + (i64::from(v) - i64::from(f.logical_min)) * (i64::from(pmax) - i64::from(pmin))
            / (i64::from(f.logical_max) - i64::from(f.logical_min));
    if !value.is_null() {
        *value = scaled as i32;
    }
    HIDP_STATUS_SUCCESS
}

pub unsafe extern "win64" fn HidP_SetScaledUsageValue(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    value: i32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let id = if t.report_ids && r[0] == 0 {
        t.caps
            .iter()
            .find(|c| {
                c.kind == kind as u8
                    && !c.button
                    && c.page == page as u16
                    && (c.range.0..=c.range.1).contains(&(usage as u16))
            })
            .map(|c| c.report_id)
            .unwrap_or(0)
    } else {
        r[0]
    };
    let (c, at) = match find_value(t, kind as u8, page as u16, link as u16, usage as u16, id) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let f = &t.fields[c.field];
    let (pmin, pmax) = physical(f);
    if f.logical_min >= f.logical_max || pmin >= pmax {
        return HIDP_STATUS_BAD_LOG_PHY_VALUES;
    }
    if value < pmin || value > pmax {
        return HIDP_STATUS_VALUE_OUT_OF_RANGE;
    }
    let logical = i64::from(f.logical_min)
        + (i64::from(value) - i64::from(pmin))
            * (i64::from(f.logical_max) - i64::from(f.logical_min))
            / (i64::from(pmax) - i64::from(pmin));
    if t.report_ids {
        r[0] = c.report_id;
    }
    set_bits(r, win_offset(t, at), f.bit_size, logical as u32);
    HIDP_STATUS_SUCCESS
}

/// Tableau de valeurs : `count` éléments de `bit_size` bits, copiés tels quels.
unsafe fn value_array(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    buf: *mut u8,
    buf_len: u16,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
    write: bool,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let (c, at) = match find_value(t, kind as u8, page as u16, link as u16, usage as u16, r[0]) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if c.is_range || c.count <= 1 {
        return HIDP_STATUS_NOT_VALUE_ARRAY;
    }
    let f = &t.fields[c.field];
    let bits = u32::from(f.bit_size) * u32::from(c.count);
    if buf.is_null() || u32::from(buf_len) * 8 < bits {
        return HIDP_STATUS_BUFFER_TOO_SMALL;
    }
    let out = std::slice::from_raw_parts_mut(buf, buf_len as usize);
    let base = win_offset(t, at);
    for b in 0..bits {
        if write {
            let v = u32::from(out[(b / 8) as usize] >> (b % 8) & 1);
            set_bits(r, base + b, 1, v);
        } else {
            let v = get_bits(r, base + b, 1) as u8;
            let byte = &mut out[(b / 8) as usize];
            *byte = (*byte & !(1 << (b % 8))) | (v << (b % 8));
        }
    }
    HIDP_STATUS_SUCCESS
}

pub unsafe extern "win64" fn HidP_GetUsageValueArray(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    buf: *mut u8,
    buf_len: u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    value_array(
        kind,
        page,
        link,
        usage,
        buf,
        buf_len as u16,
        data,
        rep,
        rep_len,
        false,
    )
}
pub unsafe extern "win64" fn HidP_SetUsageValueArray(
    kind: u32,
    page: u32,
    link: u32,
    usage: u32,
    buf: *mut u8,
    buf_len: u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    value_array(
        kind,
        page,
        link,
        usage,
        buf,
        buf_len as u16,
        data,
        rep,
        rep_len,
        true,
    )
}

/// Rapport vierge pour un identifiant : zéros (valeurs nulles quand un état
/// nul est déclaré) et identifiant en tête.
pub unsafe extern "win64" fn HidP_InitializeReportForID(
    kind: u32,
    id: u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    let id = id as u8;
    let exists = t
        .fields
        .iter()
        .any(|f| f.kind == kind as u8 && (!t.report_ids || f.report_id == id));
    if !exists {
        return HIDP_STATUS_REPORT_DOES_NOT_EXIST;
    }
    r.fill(0);
    r[0] = id;
    for f in t
        .fields
        .iter()
        .filter(|f| f.kind == kind as u8 && f.report_id == id)
    {
        if f.variable() && f.has_null() && f.bit_size > 1 {
            // Une valeur hors des bornes logiques signale « pas de donnée ».
            let null = if f.logical_min > 0 {
                0
            } else {
                (f.logical_max as u32).wrapping_add(1)
            };
            for k in 0..u32::from(f.count) {
                set_bits(
                    r,
                    win_offset(t, f.bit_offset + k * u32::from(f.bit_size)),
                    f.bit_size,
                    null,
                );
            }
        }
    }
    HIDP_STATUS_SUCCESS
}

// ============================================================
//  hidpi.dll : liste de données (HIDP_DATA, 8 octets)
// ============================================================

pub unsafe extern "win64" fn HidP_GetData(
    kind: u32,
    list: *mut u8,
    len: *mut u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if len.is_null() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    let mut items: Vec<(u16, u32)> = match pressed(t, kind as u8, r, 0, 0) {
        Ok(b) => b.into_iter().map(|(_, _, di)| (di, 1)).collect(),
        Err(e) => return e,
    };
    for c in t.caps.iter().filter(|c| c.kind == kind as u8 && !c.button) {
        if t.report_ids && c.report_id != r[0] {
            continue;
        }
        let f = &t.fields[c.field];
        let n = if c.is_range {
            c.range.1 - c.range.0 + 1
        } else {
            1
        };
        for k in 0..n {
            let at = win_offset(t, c.bit_offset + u32::from(k) * u32::from(f.bit_size));
            items.push((c.data_index.0 + k, get_bits(r, at, f.bit_size)));
        }
    }
    items.sort_by_key(|x| x.0);
    let room = *len as usize;
    *len = items.len() as u32;
    if items.len() > room || (list.is_null() && !items.is_empty()) {
        return HIDP_STATUS_BUFFER_TOO_SMALL;
    }
    for (i, (di, v)) in items.iter().enumerate() {
        let p = list.add(8 * i);
        put16(p, 0, *di);
        put16(p, 2, 0);
        put32(p, 4, *v);
    }
    HIDP_STATUS_SUCCESS
}

pub unsafe extern "win64" fn HidP_SetData(
    kind: u32,
    list: *mut u8,
    len: *mut u32,
    data: usize,
    rep: *mut u8,
    rep_len: u32,
) -> u32 {
    let (t, r) = match check_report(data, kind, rep, rep_len) {
        Ok(x) => x,
        Err(e) => return e,
    };
    if len.is_null() || list.is_null() {
        return HIDP_STATUS_SUCCESS;
    }
    for i in 0..*len as usize {
        let p = list.add(8 * i);
        let di = (p as *const u16).read_unaligned();
        let v = (p.add(4) as *const u32).read_unaligned();
        let Some(c) = t
            .caps
            .iter()
            .find(|c| c.kind == kind as u8 && (c.data_index.0..=c.data_index.1).contains(&di))
        else {
            *len = i as u32;
            return HIDP_STATUS_DATA_INDEX_NOT_FOUND;
        };
        let usage = c.range.0 + (di - c.data_index.0);
        let status = if c.button {
            if t.report_ids && r[0] == 0 {
                r[0] = c.report_id;
            }
            set_buttons(t, kind as u8, r, c.page, c.link, &[usage], (v & 0xFF) != 0)
        } else {
            if t.report_ids {
                r[0] = c.report_id;
            }
            let f = &t.fields[c.field];
            let at = c.bit_offset + u32::from(usage - c.range.0) * u32::from(f.bit_size);
            set_bits(r, win_offset(t, at), f.bit_size, v);
            HIDP_STATUS_SUCCESS
        };
        if status != HIDP_STATUS_SUCCESS && status != HIDP_STATUS_BUTTON_NOT_PRESSED {
            *len = i as u32;
            return status;
        }
    }
    HIDP_STATUS_SUCCESS
}

/// HIDP_EXTENDED_ATTRIBUTES : aucun attribut global étendu.
pub unsafe extern "win64" fn HidP_GetExtendedAttributes(
    _kind: u32,
    _data_index: u32,
    data: usize,
    attrs: *mut u8,
    len: *mut u32,
) -> u32 {
    if tlc(data).is_none() {
        return HIDP_STATUS_INVALID_PREPARSED_DATA;
    }
    if !attrs.is_null() && !len.is_null() && *len >= 4 {
        std::ptr::write_bytes(attrs, 0, 4);
    }
    HIDP_STATUS_SUCCESS
}

pub extern "win64" fn HidP_TranslateUsagesToI8042ScanCodes() -> u32 {
    HIDP_STATUS_NOT_IMPLEMENTED
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "HidP_GetCaps" => fn HidP_GetCaps,
        "HidP_GetButtonCaps" => fn HidP_GetButtonCaps,
        "HidP_GetValueCaps" => fn HidP_GetValueCaps,
        "HidP_GetSpecificButtonCaps" => fn HidP_GetSpecificButtonCaps,
        "HidP_GetSpecificValueCaps" => fn HidP_GetSpecificValueCaps,
        "HidP_GetLinkCollectionNodes" => fn HidP_GetLinkCollectionNodes,
        "HidP_MaxUsageListLength" => fn HidP_MaxUsageListLength,
        "HidP_MaxDataListLength" => fn HidP_MaxDataListLength,
        "HidP_GetUsages" => fn HidP_GetUsages,
        "HidP_GetUsagesEx" => fn HidP_GetUsagesEx,
        "HidP_SetUsages" => fn HidP_SetUsages,
        "HidP_UnsetUsages" => fn HidP_UnsetUsages,
        "HidP_UsageListDifference" => fn HidP_UsageListDifference,
        "HidP_GetUsageValue" => fn HidP_GetUsageValue,
        "HidP_SetUsageValue" => fn HidP_SetUsageValue,
        "HidP_GetScaledUsageValue" => fn HidP_GetScaledUsageValue,
        "HidP_SetScaledUsageValue" => fn HidP_SetScaledUsageValue,
        "HidP_GetUsageValueArray" => fn HidP_GetUsageValueArray,
        "HidP_SetUsageValueArray" => fn HidP_SetUsageValueArray,
        "HidP_InitializeReportForID" => fn HidP_InitializeReportForID,
        "HidP_GetData" => fn HidP_GetData,
        "HidP_SetData" => fn HidP_SetData,
        "HidP_GetExtendedAttributes" => fn HidP_GetExtendedAttributes,
        "HidP_TranslateUsagesToI8042ScanCodes" => fn HidP_TranslateUsagesToI8042ScanCodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clavier de démarrage (spécification HID 1.11, annexe B.1) :
    /// 8 modificateurs, un octet réservé, 5 voyants en sortie + 3 bits de
    /// bourrage, 6 touches en tableau (0..101).
    const KEYBOARD: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, // Generic Desktop, Keyboard, Application
        0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81,
        0x02, // modificateurs
        0x95, 0x01, 0x75, 0x08, 0x81, 0x01, // réservé (constante)
        0x95, 0x05, 0x75, 0x01, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, // voyants
        0x95, 0x01, 0x75, 0x03, 0x91, 0x01, // bourrage
        0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81,
        0x00, // touches
        0xC0,
    ];

    /// Clavier RVB typique : collection clavier (rapport 1) et collection
    /// fabricant 0xFF00 avec un rapport Feature 7 de 64 octets (couleurs) et
    /// un rapport de sortie 8 de 32 octets.
    const RGB: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, // clavier, rapport 1
        0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81,
        0x02, 0xC0, //
        0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, // fabricant, Application
        0x85, 0x07, 0x09, 0x02, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x96, 0x40, 0x00, 0xB1,
        0x02, // Feature 7 : 64 octets
        0x85, 0x08, 0x09, 0x03, 0x95, 0x20, 0x91, 0x02, // Output 8 : 32 octets
        0xC0,
    ];

    fn preparsed(d: &[u8], n: usize) -> usize {
        preparsed_into_raw(split(&parse(d)).swap_remove(n))
    }

    #[test]
    fn capacites_du_clavier_de_demarrage() {
        let p = preparsed(KEYBOARD, 0);
        let mut caps = [0u8; 64];
        unsafe {
            assert_eq!(HidP_GetCaps(p, caps.as_mut_ptr()), HIDP_STATUS_SUCCESS);
        }
        let w = |i: usize| u16::from_le_bytes([caps[2 * i], caps[2 * i + 1]]);
        assert_eq!((w(0), w(1)), (6, 1)); // Keyboard, Generic Desktop
        assert_eq!(w(2), 9); // 8 octets + octet d'identifiant
        assert_eq!(w(3), 2); // voyants : 1 octet + identifiant
        assert_eq!(w(4), 0);
        assert_eq!(w(22), 1); // une collection
        assert_eq!(w(23), 2); // boutons d'entrée : modificateurs et touches
        assert_eq!(w(24), 0);
        assert_eq!(w(26), 1); // boutons de sortie : voyants
        unsafe { preparsed_free(p) };
    }

    #[test]
    fn touches_enfoncees_et_difference() {
        let p = preparsed(KEYBOARD, 0);
        // Maj gauche (0xE1) + « a » (4) + « b » (5).
        let mut rep = [0u8, 0x02, 0, 4, 5, 0, 0, 0, 0];
        let mut list = [0u16; 16];
        let mut n = 16u32;
        unsafe {
            let s = HidP_GetUsages(0, 7, 0, list.as_mut_ptr(), &mut n, p, rep.as_mut_ptr(), 9);
            assert_eq!(s, HIDP_STATUS_SUCCESS);
        }
        assert_eq!(&list[..n as usize], &[0xE1, 4, 5]);
        // Liste trop courte : la taille nécessaire est rendue.
        let mut small = 1u32;
        unsafe {
            let s = HidP_GetUsages(
                0,
                7,
                0,
                list.as_mut_ptr(),
                &mut small,
                p,
                rep.as_mut_ptr(),
                9,
            );
            assert_eq!((s, small), (HIDP_STATUS_BUFFER_TOO_SMALL, 3));
            assert_eq!(HidP_MaxUsageListLength(0, 7, p), 8 + 6);
        }
        let prev = [4u16, 6, 0];
        let cur = [4u16, 5, 0];
        let (mut gone, mut made) = ([0u16; 3], [0u16; 3]);
        unsafe {
            HidP_UsageListDifference(
                prev.as_ptr(),
                cur.as_ptr(),
                gone.as_mut_ptr(),
                made.as_mut_ptr(),
                3,
            );
        }
        assert_eq!((gone[0], made[0]), (6, 5));
        unsafe { preparsed_free(p) };
    }

    #[test]
    fn allumer_les_voyants_et_les_touches() {
        let p = preparsed(KEYBOARD, 0);
        let mut out = [0u8; 2];
        let mut leds = [1u16, 3];
        let mut n = 2u32;
        unsafe {
            let s = HidP_SetUsages(1, 8, 0, leds.as_mut_ptr(), &mut n, p, out.as_mut_ptr(), 2);
            assert_eq!(s, HIDP_STATUS_SUCCESS);
        }
        assert_eq!(out, [0, 0b101]);
        let mut rep = [0u8; 9];
        let mut keys = [4u16, 0xE0];
        let mut n = 2u32;
        unsafe {
            let s = HidP_SetUsages(0, 7, 0, keys.as_mut_ptr(), &mut n, p, rep.as_mut_ptr(), 9);
            assert_eq!(s, HIDP_STATUS_SUCCESS);
            let mut one = [4u16];
            let mut n1 = 1u32;
            assert_eq!(
                HidP_UnsetUsages(0, 7, 0, one.as_mut_ptr(), &mut n1, p, rep.as_mut_ptr(), 9),
                HIDP_STATUS_SUCCESS
            );
        }
        assert_eq!(rep, [0, 1, 0, 0, 0, 0, 0, 0, 0]);
        unsafe { preparsed_free(p) };
    }

    #[test]
    fn deux_collections_de_tete_separees() {
        let d = parse(RGB);
        assert_eq!(d.tlcs.len(), 2);
        let tlcs = split(&d);
        assert_eq!((tlcs[0].usage_page, tlcs[0].usage), (1, 6));
        assert_eq!(tlcs[0].lengths, [2, 0, 0]);
        assert_eq!(tlcs[0].input_ids, vec![1]);
        assert_eq!((tlcs[1].usage_page, tlcs[1].usage), (0xFF00, 1));
        assert_eq!(tlcs[1].lengths, [0, 33, 65]);
        assert!(tlcs[1].input_ids.is_empty());
        let p = preparsed_into_raw(tlcs[1].clone());
        // Une valeur Feature de 64 éléments : tableau de valeurs.
        let mut caps = [0u8; 72 * 4];
        let mut n = 4u16;
        unsafe {
            assert_eq!(
                HidP_GetValueCaps(2, caps.as_mut_ptr(), &mut n, p),
                HIDP_STATUS_SUCCESS
            );
        }
        assert_eq!(n, 1);
        assert_eq!(caps[2], 7); // ReportID
        assert_eq!(u16::from_le_bytes([caps[18], caps[19]]), 8); // BitSize
        assert_eq!(u16::from_le_bytes([caps[20], caps[21]]), 64); // ReportCount
        assert_eq!(u16::from_le_bytes([caps[56], caps[57]]), 2); // Usage
                                                                 // Remplir la trame de couleurs d'un coup, sans aplatir les octets.
        let mut rep = [0u8; 65];
        let mut colors: Vec<u8> = (0..64).collect();
        unsafe {
            assert_eq!(
                HidP_InitializeReportForID(2, 7, p, rep.as_mut_ptr(), 65),
                HIDP_STATUS_SUCCESS
            );
            let s = HidP_SetUsageValueArray(
                2,
                0xFF00,
                0,
                2,
                colors.as_mut_ptr(),
                64,
                p,
                rep.as_mut_ptr(),
                65,
            );
            assert_eq!(s, HIDP_STATUS_SUCCESS);
        }
        assert_eq!(rep[0], 7);
        assert_eq!(&rep[1..], &colors[..]);
        unsafe { preparsed_free(p) };
    }

    #[test]
    fn valeurs_mises_a_l_echelle_et_liste_de_donnees() {
        // Axe X de -127..127, physique 0..1000, et deux boutons.
        let d: &[u8] = &[
            0x05, 0x01, 0x09, 0x04, 0xA1, 0x01, 0x09, 0x30, 0x15, 0x81, 0x25, 0x7F, 0x35, 0x00,
            0x46, 0xE8, 0x03, 0x75, 0x08, 0x95, 0x01, 0x81, 0x02, 0x05, 0x09, 0x19, 0x01, 0x29,
            0x02, 0x15, 0x00, 0x25, 0x01, 0x35, 0x00, 0x45, 0x00, 0x75, 0x01, 0x95, 0x02, 0x81,
            0x02, 0x75, 0x06, 0x95, 0x01, 0x81, 0x01, 0xC0,
        ];
        let p = preparsed(d, 0);
        let mut rep = [0u8, 0x81, 0b10];
        let mut v = 0i32;
        let mut raw = 0u32;
        unsafe {
            assert_eq!(
                HidP_GetScaledUsageValue(0, 1, 0, 0x30, &mut v, p, rep.as_mut_ptr(), 3),
                HIDP_STATUS_SUCCESS
            );
            assert_eq!(
                HidP_GetUsageValue(0, 1, 0, 0x30, &mut raw, p, rep.as_mut_ptr(), 3),
                HIDP_STATUS_SUCCESS
            );
        }
        assert_eq!((v, raw), (0, 0x81));
        let mut data = [0u8; 8 * 4];
        let mut n = 4u32;
        unsafe {
            assert_eq!(
                HidP_GetData(0, data.as_mut_ptr(), &mut n, p, rep.as_mut_ptr(), 3),
                HIDP_STATUS_SUCCESS
            );
            assert_eq!(HidP_MaxDataListLength(0, p), 3);
        }
        assert_eq!(n, 2); // la valeur X et le bouton 2
        assert_eq!(u16::from_le_bytes([data[8], data[9]]), 2); // indice du bouton 2
        unsafe {
            assert_eq!(
                HidP_SetScaledUsageValue(0, 1, 0, 0x30, 1000, p, rep.as_mut_ptr(), 3),
                HIDP_STATUS_SUCCESS
            );
        }
        assert_eq!(rep[1], 0x7F);
        unsafe { preparsed_free(p) };
    }
}
