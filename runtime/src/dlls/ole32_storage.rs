//! Stockage composé (#1235) : IStorage, IStream, IEnumSTATSTG et ILockBytes.
//!
//! Le document est tenu en mémoire sous forme d'arbre ; il est lu et écrit
//! au format publié [MS-CFB] (Compound File Binary File Format, spécification
//! ouverte de Microsoft), dans un fichier ou dans un ILockBytes (HGLOBAL).
//! Mode direct : le support est réécrit à chaque Commit et à la dernière
//! libération. Mode transactionnel : seulement au Commit de la racine,
//! Revert relit le support.

use super::ole32::Guid;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const S_OK: i32 = 0;
const S_FALSE: i32 = 1;
const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
const E_NOTIMPL: i32 = 0x8000_4001u32 as i32;
const STG_E_INVALIDFUNCTION: i32 = 0x8003_0001u32 as i32;
const STG_E_FILENOTFOUND: i32 = 0x8003_0002u32 as i32;
const STG_E_ACCESSDENIED: i32 = 0x8003_0005u32 as i32;
const STG_E_INSUFFICIENTMEMORY: i32 = 0x8003_0008u32 as i32;
const STG_E_INVALIDPOINTER: i32 = 0x8003_0009u32 as i32;
const STG_E_WRITEFAULT: i32 = 0x8003_001Du32 as i32;
const STG_E_READFAULT: i32 = 0x8003_001Eu32 as i32;
const STG_E_FILEALREADYEXISTS: i32 = 0x8003_0050u32 as i32;
const STG_E_INVALIDPARAMETER: i32 = 0x8003_0057u32 as i32;
const STG_E_INVALIDNAME: i32 = 0x8003_00FCu32 as i32;
const STG_E_REVERTED: i32 = 0x8003_0102u32 as i32;
const STG_E_DOCFILECORRUPT: i32 = 0x8003_0109u32 as i32;
const STG_S_CONVERTED: i32 = 0x0003_0200;

const STGM_WRITE: u32 = 1;
const STGM_READWRITE: u32 = 2;
const STGM_CREATE: u32 = 0x1000;
const STGM_CONVERT: u32 = 0x2_0000;
const STGM_TRANSACTED: u32 = 0x1_0000;
const STGM_DELETEONRELEASE: u32 = 0x0400_0000;

const STGTY_STORAGE: u32 = 1;
const STGTY_STREAM: u32 = 2;
const STGTY_LOCKBYTES: u32 = 3;
const STATFLAG_NONAME: u32 = 1;
const STGMOVE_MOVE: u32 = 0;

const STGFMT_STORAGE: u32 = 0;
const STGFMT_ANY: u32 = 4;
const STGFMT_DOCFILE: u32 = 5;

const fn ole_guid(data1: u32) -> Guid {
    Guid {
        data1,
        data2: 0,
        data3: 0,
        data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46],
    }
}
const IID_IUNKNOWN: Guid = ole_guid(0);
const IID_ILOCKBYTES: Guid = ole_guid(0xA);
const IID_ISTORAGE: Guid = ole_guid(0xB);
const IID_ISTREAM: Guid = ole_guid(0xC);
const IID_IENUMSTATSTG: Guid = ole_guid(0xD);
const IID_ISEQUENTIALSTREAM: Guid = Guid {
    data1: 0x0c73_3a30,
    data2: 0x2a1c,
    data3: 0x11ce,
    data4: [0xad, 0xe5, 0x00, 0xaa, 0x00, 0x44, 0x77, 0x3d],
};

// ── Arbre du document ───────────────────────────────────────────────

#[derive(Clone, Debug)]
enum Kind {
    Storage(Vec<Entry>),
    Stream(Vec<u8>),
}

#[derive(Clone, Debug)]
struct Entry {
    name: String,
    clsid: Guid,
    bits: u32,
    ctime: u64,
    mtime: u64,
    kind: Kind,
}

impl Entry {
    fn storage(name: &str) -> Entry {
        Entry {
            name: name.into(),
            clsid: Guid::default(),
            bits: 0,
            ctime: 0,
            mtime: 0,
            kind: Kind::Storage(Vec::new()),
        }
    }
    fn stream(name: &str, data: Vec<u8>) -> Entry {
        Entry {
            name: name.into(),
            clsid: Guid::default(),
            bits: 0,
            ctime: 0,
            mtime: 0,
            kind: Kind::Stream(data),
        }
    }
    fn children(&self) -> &[Entry] {
        match &self.kind {
            Kind::Storage(c) => c,
            Kind::Stream(_) => &[],
        }
    }
    fn children_mut(&mut self) -> Option<&mut Vec<Entry>> {
        match &mut self.kind {
            Kind::Storage(c) => Some(c),
            Kind::Stream(_) => None,
        }
    }
    fn size(&self) -> u64 {
        match &self.kind {
            Kind::Stream(d) => d.len() as u64,
            Kind::Storage(_) => 0,
        }
    }
}

/// Comparaison des noms d'éléments : sans tenir compte de la casse.
fn same_name(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

/// Ordre des frères dans l'arbre rouge-noir de [MS-CFB] : le nom le plus
/// court d'abord, puis comparaison en majuscules.
fn cfb_order(a: &str, b: &str) -> std::cmp::Ordering {
    let ua: Vec<u16> = a.to_uppercase().encode_utf16().collect();
    let ub: Vec<u16> = b.to_uppercase().encode_utf16().collect();
    a.encode_utf16()
        .count()
        .cmp(&b.encode_utf16().count())
        .then(ua.cmp(&ub))
}

fn find<'a>(e: &'a Entry, path: &[String]) -> Option<&'a Entry> {
    let mut cur = e;
    for p in path {
        cur = cur.children().iter().find(|c| same_name(&c.name, p))?;
    }
    Some(cur)
}

fn find_mut<'a>(e: &'a mut Entry, path: &[String]) -> Option<&'a mut Entry> {
    let mut cur = e;
    for p in path {
        cur = cur
            .children_mut()?
            .iter_mut()
            .find(|c| same_name(&c.name, p))?;
    }
    Some(cur)
}

// ── Format [MS-CFB] ─────────────────────────────────────────────────

const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const FREESECT: u32 = 0xFFFF_FFFF;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const DIFSECT: u32 = 0xFFFF_FFFC;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_CUTOFF: usize = 4096;
const MINI: usize = 64;
const SECTOR: usize = 512;

fn rd16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn rd64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn guid_at(b: &[u8], o: usize) -> Guid {
    Guid {
        data1: rd32(b, o),
        data2: rd16(b, o + 4),
        data3: rd16(b, o + 6),
        data4: b[o + 8..o + 16].try_into().unwrap(),
    }
}
fn put_guid(b: &mut [u8], o: usize, g: &Guid) {
    b[o..o + 4].copy_from_slice(&g.data1.to_le_bytes());
    b[o + 4..o + 6].copy_from_slice(&g.data2.to_le_bytes());
    b[o + 6..o + 8].copy_from_slice(&g.data3.to_le_bytes());
    b[o + 8..o + 16].copy_from_slice(&g.data4);
}

fn is_cfb(bytes: &[u8]) -> bool {
    bytes.len() >= 512 && bytes[..8] == SIGNATURE
}

/// Lit un fichier composé. Données non fiables : chaînes bornées, cycles
/// refusés.
fn parse_cfb(bytes: &[u8]) -> Result<Entry, i32> {
    if !is_cfb(bytes) {
        return Err(STG_E_FILEALREADYEXISTS);
    }
    let corrupt = Err(STG_E_DOCFILECORRUPT);
    let major = rd16(bytes, 26);
    let shift = rd16(bytes, 30);
    let ss = match (major, shift) {
        (3, 9) => 512usize,
        (4, 12) => 4096,
        _ => return corrupt,
    };
    if rd16(bytes, 32) != 6 {
        return corrupt;
    }
    let sector = |n: u32| -> Option<Vec<u8>> {
        let off = (n as usize).checked_add(1)?.checked_mul(ss)?;
        if off >= bytes.len() {
            return None;
        }
        let mut v = bytes[off..bytes.len().min(off + ss)].to_vec();
        v.resize(ss, 0);
        Some(v)
    };
    let max_sectors = bytes.len() / ss + 1;
    let num_fat = rd32(bytes, 44) as usize;
    if num_fat > max_sectors {
        return corrupt;
    }
    // DIFAT : 109 entrées dans l'en-tête, puis une chaîne de secteurs.
    let mut fat_sectors: Vec<u32> = (0..109).map(|i| rd32(bytes, 76 + i * 4)).collect();
    let mut next = rd32(bytes, 68);
    let mut guard = 0;
    while next != ENDOFCHAIN && next != FREESECT && fat_sectors.len() < num_fat + 109 {
        guard += 1;
        if guard > max_sectors {
            return corrupt;
        }
        let Some(s) = sector(next) else {
            return corrupt;
        };
        let n = ss / 4 - 1;
        fat_sectors.extend((0..n).map(|i| rd32(&s, i * 4)));
        next = rd32(&s, n * 4);
    }
    let fat_sectors: Vec<u32> = fat_sectors
        .into_iter()
        .filter(|&s| s != FREESECT)
        .take(num_fat)
        .collect();
    let mut fat: Vec<u32> = Vec::with_capacity(fat_sectors.len() * ss / 4);
    for s in fat_sectors {
        let Some(d) = sector(s) else { return corrupt };
        fat.extend((0..ss / 4).map(|i| rd32(&d, i * 4)));
    }
    let chain = |start: u32, table: &[u32]| -> Result<Vec<u32>, i32> {
        let mut out = Vec::new();
        let mut cur = start;
        while cur != ENDOFCHAIN {
            if cur as usize >= table.len() || out.len() > table.len() {
                return Err(STG_E_DOCFILECORRUPT);
            }
            out.push(cur);
            cur = table[cur as usize];
        }
        Ok(out)
    };
    let read_chain = |start: u32| -> Result<Vec<u8>, i32> {
        let mut out = Vec::new();
        for s in chain(start, &fat)? {
            out.extend(sector(s).ok_or(STG_E_DOCFILECORRUPT)?);
        }
        Ok(out)
    };
    let dir = read_chain(rd32(bytes, 48))?;
    let n_dir = dir.len() / 128;
    if n_dir == 0 {
        return corrupt;
    }
    let minifat_bytes = match rd32(bytes, 60) {
        ENDOFCHAIN | FREESECT => Vec::new(),
        s => read_chain(s)?,
    };
    let minifat: Vec<u32> = (0..minifat_bytes.len() / 4)
        .map(|i| rd32(&minifat_bytes, i * 4))
        .collect();
    let cutoff = rd32(bytes, 56) as u64;
    let de = |i: usize| &dir[i * 128..i * 128 + 128];
    let size_of = |d: &[u8]| -> u64 {
        let s = rd64(d, 120);
        if major == 3 {
            s & 0xFFFF_FFFF
        } else {
            s
        }
    };
    let root = de(0);
    if root[66] != 5 {
        return corrupt;
    }
    let mut ministream = match rd32(root, 116) {
        ENDOFCHAIN | FREESECT => Vec::new(),
        s => read_chain(s)?,
    };
    ministream.truncate(size_of(root) as usize);

    let stream_data = |d: &[u8]| -> Result<Vec<u8>, i32> {
        let size = size_of(d);
        if size == 0 {
            return Ok(Vec::new());
        }
        if size as usize > bytes.len() * 2 + ministream.len() {
            return Err(STG_E_DOCFILECORRUPT);
        }
        let start = rd32(d, 116);
        let mut out = if size < cutoff {
            let mut v = Vec::new();
            for m in chain(start, &minifat)? {
                let o = m as usize * MINI;
                if o + MINI > ministream.len() {
                    return Err(STG_E_DOCFILECORRUPT);
                }
                v.extend_from_slice(&ministream[o..o + MINI]);
            }
            v
        } else {
            read_chain(start)?
        };
        if (out.len() as u64) < size {
            return Err(STG_E_DOCFILECORRUPT);
        }
        out.truncate(size as usize);
        Ok(out)
    };

    fn name_of(d: &[u8]) -> String {
        let len = (rd16(d, 64) as usize).min(64);
        let units: Vec<u16> = (0..len / 2).map(|i| rd16(d, i * 2)).collect();
        let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
        String::from_utf16_lossy(&units[..end])
    }

    // Parcours de l'arbre des frères (infixe), puis des enfants.
    let mut seen = vec![false; n_dir];
    // Parcours infixe itératif : un arbre dégénéré (liste) reste lisible.
    fn siblings(
        id: u32,
        n_dir: usize,
        dir: &[u8],
        seen: &mut [bool],
        out: &mut Vec<usize>,
    ) -> Result<(), i32> {
        let mut stack: Vec<usize> = Vec::new();
        let mut cur = id;
        loop {
            while cur != NOSTREAM {
                let i = cur as usize;
                if i >= n_dir || seen[i] {
                    return Err(STG_E_DOCFILECORRUPT);
                }
                seen[i] = true;
                stack.push(i);
                cur = rd32(&dir[i * 128..], 68);
            }
            let Some(i) = stack.pop() else { return Ok(()) };
            out.push(i);
            cur = rd32(&dir[i * 128..], 72);
        }
    }
    seen[0] = true;
    let mut todo: Vec<(usize, Vec<usize>)> = Vec::new(); // (id, chemin d'indices)
    let mut root_entry = Entry::storage("Root Entry");
    root_entry.clsid = guid_at(root, 80);
    root_entry.bits = rd32(root, 96);
    root_entry.mtime = rd64(root, 108);
    todo.push((0, Vec::new()));
    while let Some((id, path)) = todo.pop() {
        let mut ids = Vec::new();
        siblings(rd32(de(id), 76), n_dir, &dir, &mut seen, &mut ids)?;
        let mut node = &mut root_entry;
        for &k in &path {
            node = &mut node.children_mut().ok_or(STG_E_DOCFILECORRUPT)?[k];
        }
        let base = node.children().len();
        let mut subs = Vec::new();
        for c in ids {
            let d = de(c);
            let mut e = match d[66] {
                1 => {
                    subs.push(c);
                    Entry::storage(&name_of(d))
                }
                2 => Entry::stream(&name_of(d), stream_data(d)?),
                _ => continue,
            };
            e.clsid = guid_at(d, 80);
            e.bits = rd32(d, 96);
            e.ctime = rd64(d, 100);
            e.mtime = rd64(d, 108);
            node.children_mut().ok_or(STG_E_DOCFILECORRUPT)?.push(e);
        }
        let kids = node.children();
        for (k, e) in kids.iter().enumerate().skip(base) {
            if matches!(e.kind, Kind::Storage(_)) {
                let pos = subs.remove(0);
                let mut p = path.clone();
                p.push(k);
                todo.push((pos, p));
            }
        }
    }
    Ok(root_entry)
}

struct DirOut {
    name: String,
    typ: u8,
    clsid: Guid,
    bits: u32,
    ctime: u64,
    mtime: u64,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

/// Écrit un fichier composé version 3 (secteurs de 512 octets), nœuds tous
/// noirs (arbre binaire équilibré, admis par [MS-CFB]).
fn write_cfb(root: &Entry) -> Vec<u8> {
    let mut dirs: Vec<DirOut> = Vec::new();
    let mut datas: Vec<Option<&Vec<u8>>> = Vec::new();
    fn add<'a>(
        e: &'a Entry,
        typ: u8,
        dirs: &mut Vec<DirOut>,
        datas: &mut Vec<Option<&'a Vec<u8>>>,
    ) -> u32 {
        let id = dirs.len() as u32;
        dirs.push(DirOut {
            name: e.name.clone(),
            typ,
            clsid: e.clsid,
            bits: e.bits,
            ctime: if typ == 5 { 0 } else { e.ctime },
            mtime: e.mtime,
            left: NOSTREAM,
            right: NOSTREAM,
            child: NOSTREAM,
            start: ENDOFCHAIN,
            size: 0,
        });
        match &e.kind {
            Kind::Stream(d) => {
                datas.push(Some(d));
                dirs[id as usize].size = d.len() as u64;
                dirs[id as usize].clsid = Guid::default();
            }
            Kind::Storage(children) => {
                datas.push(None);
                let mut sorted: Vec<&Entry> = children.iter().collect();
                sorted.sort_by(|a, b| cfb_order(&a.name, &b.name));
                let ids: Vec<u32> = sorted
                    .iter()
                    .map(|c| {
                        let t = if matches!(c.kind, Kind::Storage(_)) {
                            1
                        } else {
                            2
                        };
                        add(c, t, dirs, datas)
                    })
                    .collect();
                fn bst(ids: &[u32], dirs: &mut [DirOut]) -> u32 {
                    if ids.is_empty() {
                        return NOSTREAM;
                    }
                    let mid = ids.len() / 2;
                    let l = bst(&ids[..mid], dirs);
                    let r = bst(&ids[mid + 1..], dirs);
                    dirs[ids[mid] as usize].left = l;
                    dirs[ids[mid] as usize].right = r;
                    ids[mid]
                }
                dirs[id as usize].child = bst(&ids, dirs);
            }
        }
        id
    }
    add(root, 5, &mut dirs, &mut datas);

    // Mini-flux et grands flux.
    let mut ministream: Vec<u8> = Vec::new();
    let mut minifat: Vec<u32> = Vec::new();
    let mut sectors: Vec<u8> = Vec::new(); // secteurs à partir du n° 0
    let mut fat: Vec<u32> = Vec::new();
    let chain_into = |fat: &mut Vec<u32>, first: usize, count: usize| {
        for k in 0..count {
            fat.push(if k + 1 == count {
                ENDOFCHAIN
            } else {
                (first + k + 1) as u32
            });
        }
    };
    for (i, d) in datas.iter().enumerate() {
        let Some(d) = d else { continue };
        if d.is_empty() {
            continue;
        }
        if d.len() < MINI_CUTOFF {
            let first = minifat.len();
            let count = d.len().div_ceil(MINI);
            chain_into(&mut minifat, first, count);
            ministream.extend_from_slice(d);
            ministream.resize(ministream.len().next_multiple_of(MINI), 0);
            dirs[i].start = first as u32;
        } else {
            let first = fat.len();
            let count = d.len().div_ceil(SECTOR);
            chain_into(&mut fat, first, count);
            sectors.extend_from_slice(d);
            sectors.resize(sectors.len().next_multiple_of(SECTOR), 0);
            dirs[i].start = first as u32;
        }
    }
    let put_run = |bytes: &[u8], fat: &mut Vec<u32>, sectors: &mut Vec<u8>| -> u32 {
        if bytes.is_empty() {
            return ENDOFCHAIN;
        }
        let first = fat.len();
        let count = bytes.len().div_ceil(SECTOR);
        chain_into(fat, first, count);
        sectors.extend_from_slice(bytes);
        sectors.resize(sectors.len().next_multiple_of(SECTOR), 0);
        first as u32
    };
    dirs[0].size = ministream.len() as u64;
    dirs[0].start = put_run(&ministream, &mut fat, &mut sectors);
    let minifat_bytes: Vec<u8> = minifat.iter().flat_map(|v| v.to_le_bytes()).collect();
    let first_minifat = put_run(&minifat_bytes, &mut fat, &mut sectors);
    let n_minifat = minifat_bytes.len().div_ceil(SECTOR) as u32;

    let mut dir_bytes = Vec::with_capacity(dirs.len().next_multiple_of(4) * 128);
    for d in &dirs {
        let mut b = [0u8; 128];
        let units: Vec<u16> = d.name.encode_utf16().take(31).collect();
        for (k, u) in units.iter().enumerate() {
            b[k * 2..k * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        b[64..66].copy_from_slice(&(((units.len() + 1) * 2) as u16).to_le_bytes());
        b[66] = d.typ;
        b[67] = 1; // noir
        b[68..72].copy_from_slice(&d.left.to_le_bytes());
        b[72..76].copy_from_slice(&d.right.to_le_bytes());
        b[76..80].copy_from_slice(&d.child.to_le_bytes());
        put_guid(&mut b, 80, &d.clsid);
        b[96..100].copy_from_slice(&d.bits.to_le_bytes());
        b[100..108].copy_from_slice(&d.ctime.to_le_bytes());
        b[108..116].copy_from_slice(&d.mtime.to_le_bytes());
        let start = if d.typ == 1 { 0 } else { d.start };
        b[116..120].copy_from_slice(&start.to_le_bytes());
        b[120..128].copy_from_slice(&d.size.to_le_bytes());
        dir_bytes.extend_from_slice(&b);
    }
    while dir_bytes.len() % SECTOR != 0 {
        let mut b = [0u8; 128];
        b[68..80].copy_from_slice(&[0xFF; 12]);
        dir_bytes.extend_from_slice(&b);
    }
    let first_dir = put_run(&dir_bytes, &mut fat, &mut sectors);

    // Secteurs de FAT et de DIFAT : leur nombre dépend du total.
    let data_sectors = fat.len();
    let per = SECTOR / 4;
    let (mut n_fat, mut n_difat) = (0usize, 0usize);
    loop {
        let total = data_sectors + n_fat + n_difat;
        let need_fat = total.div_ceil(per);
        let need_difat = need_fat.saturating_sub(109).div_ceil(per - 1);
        if need_fat == n_fat && need_difat == n_difat {
            break;
        }
        n_fat = need_fat;
        n_difat = need_difat;
    }
    let fat_ids: Vec<u32> = (data_sectors..data_sectors + n_fat)
        .map(|s| s as u32)
        .collect();
    let difat_ids: Vec<u32> = (data_sectors + n_fat..data_sectors + n_fat + n_difat)
        .map(|s| s as u32)
        .collect();
    fat.extend(std::iter::repeat_n(FATSECT, n_fat));
    fat.extend(std::iter::repeat_n(DIFSECT, n_difat));
    fat.resize(n_fat * per, FREESECT);
    for v in &fat {
        sectors.extend_from_slice(&v.to_le_bytes());
    }
    for (k, _) in difat_ids.iter().enumerate() {
        let mut b = vec![0xFFu8; SECTOR];
        for j in 0..per - 1 {
            if let Some(f) = fat_ids.get(109 + k * (per - 1) + j) {
                b[j * 4..j * 4 + 4].copy_from_slice(&f.to_le_bytes());
            }
        }
        let next = difat_ids.get(k + 1).copied().unwrap_or(ENDOFCHAIN);
        b[SECTOR - 4..].copy_from_slice(&next.to_le_bytes());
        sectors.extend_from_slice(&b);
    }

    let mut h = vec![0u8; 512];
    h[..8].copy_from_slice(&SIGNATURE);
    h[24..26].copy_from_slice(&0x3Eu16.to_le_bytes());
    h[26..28].copy_from_slice(&3u16.to_le_bytes());
    h[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    h[30..32].copy_from_slice(&9u16.to_le_bytes());
    h[32..34].copy_from_slice(&6u16.to_le_bytes());
    h[44..48].copy_from_slice(&(n_fat as u32).to_le_bytes());
    h[48..52].copy_from_slice(&first_dir.to_le_bytes());
    h[56..60].copy_from_slice(&(MINI_CUTOFF as u32).to_le_bytes());
    let fm = if n_minifat == 0 {
        ENDOFCHAIN
    } else {
        first_minifat
    };
    h[60..64].copy_from_slice(&fm.to_le_bytes());
    h[64..68].copy_from_slice(&n_minifat.to_le_bytes());
    h[68..72].copy_from_slice(
        &difat_ids
            .first()
            .copied()
            .unwrap_or(ENDOFCHAIN)
            .to_le_bytes(),
    );
    h[72..76].copy_from_slice(&(n_difat as u32).to_le_bytes());
    for k in 0..109 {
        let v = fat_ids.get(k).copied().unwrap_or(FREESECT);
        h[76 + k * 4..80 + k * 4].copy_from_slice(&v.to_le_bytes());
    }
    h.extend_from_slice(&sectors);
    h
}

// ── Appels COM génériques ───────────────────────────────────────────

unsafe fn slot(obj: usize, n: usize) -> usize {
    *(*(obj as *const *const usize)).add(n)
}
unsafe fn com_add_ref(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 1));
    f(obj);
}
unsafe fn com_release(obj: usize) {
    let f: unsafe extern "win64" fn(usize) -> u32 = std::mem::transmute(slot(obj, 2));
    f(obj);
}

/// Contenu entier d'un ILockBytes (le nôtre ou celui de l'appelant).
unsafe fn lockbytes_read_all(lb: usize) -> Result<Vec<u8>, i32> {
    let mut st = [0u64; 10];
    let stat: unsafe extern "win64" fn(usize, *mut u64, u32) -> i32 =
        std::mem::transmute(slot(lb, 9));
    let hr = stat(lb, st.as_mut_ptr(), STATFLAG_NONAME);
    if hr < 0 {
        return Err(hr);
    }
    let size = st[2] as usize;
    if size > (1usize << 31) {
        return Err(STG_E_INSUFFICIENTMEMORY);
    }
    let mut out = vec![0u8; size];
    let mut got = 0u32;
    let read_at: unsafe extern "win64" fn(usize, u64, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(lb, 3));
    let hr = read_at(lb, 0, out.as_mut_ptr(), size as u32, &mut got);
    if hr < 0 {
        return Err(hr);
    }
    out.truncate(got as usize);
    Ok(out)
}

unsafe fn lockbytes_write_all(lb: usize, data: &[u8]) -> i32 {
    let set_size: unsafe extern "win64" fn(usize, u64) -> i32 = std::mem::transmute(slot(lb, 6));
    let hr = set_size(lb, data.len() as u64);
    if hr < 0 {
        return hr;
    }
    let write_at: unsafe extern "win64" fn(usize, u64, *const u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(lb, 4));
    let mut done = 0u32;
    let hr = write_at(lb, 0, data.as_ptr(), data.len() as u32, &mut done);
    if hr < 0 {
        return hr;
    }
    let flush: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(slot(lb, 5));
    flush(lb)
}

// ── Document partagé ────────────────────────────────────────────────

enum Backing {
    File { path: PathBuf, name: String },
    Bytes(usize),
}

struct Doc {
    root: Entry,
    backing: Backing,
    writable: bool,
    transacted: bool,
    dirty: bool,
    delete_on_release: bool,
}

impl Doc {
    fn save(&mut self) -> i32 {
        if !self.writable {
            return S_OK;
        }
        let bytes = write_cfb(&self.root);
        let hr = match &self.backing {
            Backing::File { path, .. } => match std::fs::write(path, &bytes) {
                Ok(()) => S_OK,
                Err(_) => STG_E_WRITEFAULT,
            },
            Backing::Bytes(lb) => unsafe { lockbytes_write_all(*lb, &bytes) },
        };
        if hr >= 0 {
            self.dirty = false;
        }
        hr
    }
    fn load(&mut self) -> i32 {
        let bytes = match &self.backing {
            Backing::File { path, .. } => match std::fs::read(path) {
                Ok(b) => b,
                Err(_) => return STG_E_READFAULT,
            },
            Backing::Bytes(lb) => match unsafe { lockbytes_read_all(*lb) } {
                Ok(b) => b,
                Err(hr) => return hr,
            },
        };
        match parse_cfb(&bytes) {
            Ok(root) => {
                self.root = root;
                self.dirty = false;
                S_OK
            }
            Err(hr) => hr,
        }
    }
}

struct DocCell(Mutex<Doc>);

impl Drop for DocCell {
    fn drop(&mut self) {
        let d = self.0.get_mut().unwrap_or_else(|e| e.into_inner());
        if d.dirty && !d.transacted {
            d.save();
        }
        match &d.backing {
            Backing::File { path, .. } => {
                if d.delete_on_release {
                    let _ = std::fs::remove_file(path);
                }
            }
            Backing::Bytes(lb) => unsafe { com_release(*lb) },
        }
    }
}

type DocRef = Arc<DocCell>;

fn doc(d: &DocRef) -> MutexGuard<'_, Doc> {
    d.0.lock().unwrap_or_else(|e| e.into_inner())
}

// ── Objets COM ──────────────────────────────────────────────────────

struct Vtbl<const N: usize>([*const (); N]);
unsafe impl<const N: usize> Sync for Vtbl<N> {}

/// IStorage sur un nœud du document.
#[repr(C)]
struct StgObj {
    vtbl: *const usize,
    refs: AtomicU32,
    doc: DocRef,
    path: Vec<String>,
}

/// IStream sur un flux du document.
#[repr(C)]
struct StmObj {
    vtbl: *const usize,
    refs: AtomicU32,
    doc: DocRef,
    path: Vec<String>,
    pos: Mutex<u64>,
}

/// IEnumSTATSTG : instantané des éléments d'un stockage.
#[repr(C)]
struct EnumObj {
    vtbl: *const usize,
    refs: AtomicU32,
    items: Vec<(String, u32, u64, Guid, u32, u64, u64)>,
    pos: Mutex<usize>,
}

/// ILockBytes sur un HGLOBAL.
#[repr(C)]
struct LockBytesObj {
    vtbl: *const usize,
    refs: AtomicU32,
    global: usize,
    memory: Arc<Mutex<super::kernel32::GlobalMemory>>,
    delete_on_release: bool,
}

unsafe fn new_storage(doc: DocRef, path: Vec<String>) -> usize {
    Box::into_raw(Box::new(StgObj {
        vtbl: STORAGE_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        doc,
        path,
    })) as usize
}

unsafe fn new_stream(doc: DocRef, path: Vec<String>, pos: u64) -> usize {
    Box::into_raw(Box::new(StmObj {
        vtbl: STREAM_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        doc,
        path,
        pos: Mutex::new(pos),
    })) as usize
}

unsafe fn guid_eq(p: *const Guid, g: &Guid) -> bool {
    !p.is_null() && std::ptr::read_unaligned(p) == *g
}

unsafe fn wide(p: *const u16) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let len = (0..).take_while(|&i| *p.add(i) != 0).count();
    Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)))
}

/// Nom d'élément : 1 à 31 caractères, sans « / \ : ! ».
unsafe fn element_name(p: *const u16) -> Result<String, i32> {
    let Some(s) = wide(p) else {
        return Err(STG_E_INVALIDPOINTER);
    };
    let n = s.encode_utf16().count();
    if n == 0 || n > 31 || s.contains(['/', '\\', ':', '!']) {
        return Err(STG_E_INVALIDNAME);
    }
    Ok(s)
}

unsafe fn co_wide(s: &str) -> *mut u16 {
    let w: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
    let p = super::ole32::co_task_mem_alloc(w.len() * 2) as *mut u16;
    if !p.is_null() {
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
    }
    p
}

#[allow(clippy::too_many_arguments)]
unsafe fn fill_stat(
    out: *mut u8,
    name: Option<&str>,
    typ: u32,
    size: u64,
    mtime: u64,
    ctime: u64,
    mode: u32,
    clsid: &Guid,
    bits: u32,
) {
    std::ptr::write_bytes(out, 0, 80);
    let p = name.map(|n| co_wide(n)).unwrap_or(std::ptr::null_mut());
    std::ptr::write_unaligned(out as *mut *mut u16, p);
    std::ptr::write_unaligned(out.add(8) as *mut u32, typ);
    std::ptr::write_unaligned(out.add(16) as *mut u64, size);
    std::ptr::write_unaligned(out.add(24) as *mut u64, mtime);
    std::ptr::write_unaligned(out.add(32) as *mut u64, ctime);
    std::ptr::write_unaligned(out.add(48) as *mut u32, mode);
    std::ptr::write_unaligned(out.add(56) as *mut Guid, *clsid);
    std::ptr::write_unaligned(out.add(72) as *mut u32, bits);
}

fn now_filetime() -> u64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_nanos() / 100) as u64 + 116_444_736_000_000_000
}

// ── IStorage ────────────────────────────────────────────────────────

static STORAGE_VTBL: Vtbl<18> = Vtbl([
    stg_query_interface as *const (),
    stg_add_ref as *const (),
    stg_release as *const (),
    stg_create_stream as *const (),
    stg_open_stream as *const (),
    stg_create_storage as *const (),
    stg_open_sub_storage as *const (),
    stg_copy_to as *const (),
    stg_move_element_to as *const (),
    stg_commit as *const (),
    stg_revert as *const (),
    stg_enum_elements as *const (),
    stg_destroy_element as *const (),
    stg_rename_element as *const (),
    stg_set_element_times as *const (),
    stg_set_class as *const (),
    stg_set_state_bits as *const (),
    stg_stat as *const (),
]);

unsafe fn stg(this: usize) -> &'static StgObj {
    &*(this as *const StgObj)
}

fn is_ours(obj: usize) -> bool {
    obj != 0 && unsafe { *(obj as *const *const usize) } == STORAGE_VTBL.0.as_ptr().cast()
}

unsafe extern "win64" fn stg_query_interface(
    this: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    if guid_eq(iid, &IID_IUNKNOWN) || guid_eq(iid, &IID_ISTORAGE) {
        stg_add_ref(this);
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn stg_add_ref(this: usize) -> u32 {
    stg(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn stg_release(this: usize) -> u32 {
    let n = stg(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut StgObj));
    }
    n
}

fn write_mode(mode: u32) -> bool {
    mode & (STGM_WRITE | STGM_READWRITE) != 0
}

/// Crée un élément (flux ou stockage) dans le stockage `this`.
unsafe fn create_element(
    this: usize,
    name: *const u16,
    mode: u32,
    storage: bool,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    let name = match element_name(name) {
        Ok(n) => n,
        Err(hr) => return hr,
    };
    let o = stg(this);
    let mut d = doc(&o.doc);
    if !d.writable {
        return STG_E_ACCESSDENIED;
    }
    let Some(node) = find_mut(&mut d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    let kids = node.children_mut().unwrap();
    if let Some(i) = kids.iter().position(|c| same_name(&c.name, &name)) {
        if mode & STGM_CREATE == 0 {
            return STG_E_FILEALREADYEXISTS;
        }
        kids.remove(i);
    }
    let mut e = if storage {
        Entry::storage(&name)
    } else {
        Entry::stream(&name, Vec::new())
    };
    let now = now_filetime();
    e.ctime = now;
    e.mtime = now;
    kids.push(e);
    node.mtime = now;
    d.dirty = true;
    drop(d);
    let mut path = o.path.clone();
    path.push(name);
    *out = if storage {
        new_storage(o.doc.clone(), path)
    } else {
        new_stream(o.doc.clone(), path, 0)
    };
    S_OK
}

unsafe fn open_element(
    this: usize,
    name: *const u16,
    mode: u32,
    storage: bool,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    let name = match element_name(name) {
        Ok(n) => n,
        Err(hr) => return hr,
    };
    let o = stg(this);
    let d = doc(&o.doc);
    if write_mode(mode) && !d.writable {
        return STG_E_ACCESSDENIED;
    }
    let Some(node) = find(&d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    let Some(e) = node.children().iter().find(|c| same_name(&c.name, &name)) else {
        return STG_E_FILENOTFOUND;
    };
    if matches!(e.kind, Kind::Storage(_)) != storage {
        return STG_E_FILENOTFOUND;
    }
    let real = e.name.clone();
    drop(d);
    let mut path = o.path.clone();
    path.push(real);
    *out = if storage {
        new_storage(o.doc.clone(), path)
    } else {
        new_stream(o.doc.clone(), path, 0)
    };
    S_OK
}

unsafe extern "win64" fn stg_create_stream(
    this: usize,
    name: *const u16,
    mode: u32,
    _r1: u32,
    _r2: u32,
    out: *mut usize,
) -> i32 {
    create_element(this, name, mode, false, out)
}
unsafe extern "win64" fn stg_open_stream(
    this: usize,
    name: *const u16,
    _r1: usize,
    mode: u32,
    _r2: u32,
    out: *mut usize,
) -> i32 {
    open_element(this, name, mode, false, out)
}
unsafe extern "win64" fn stg_create_storage(
    this: usize,
    name: *const u16,
    mode: u32,
    _r1: u32,
    _r2: u32,
    out: *mut usize,
) -> i32 {
    create_element(this, name, mode, true, out)
}
unsafe extern "win64" fn stg_open_sub_storage(
    this: usize,
    name: *const u16,
    _priority: usize,
    mode: u32,
    _exclude: usize,
    _r: u32,
    out: *mut usize,
) -> i32 {
    open_element(this, name, mode, true, out)
}

/// Copie un élément dans un IStorage quelconque, par son interface.
unsafe fn copy_entry_into(
    e: &Entry,
    dest: usize,
    name: &str,
    skip_storages: bool,
    skip_streams: bool,
) -> i32 {
    let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mode = STGM_CREATE | STGM_READWRITE | 0x10;
    match &e.kind {
        Kind::Stream(data) => {
            if skip_streams {
                return S_OK;
            }
            let create: unsafe extern "win64" fn(
                usize,
                *const u16,
                u32,
                u32,
                u32,
                *mut usize,
            ) -> i32 = std::mem::transmute(slot(dest, 3));
            let mut stm = 0usize;
            let hr = create(dest, w.as_ptr(), mode, 0, 0, &mut stm);
            if hr < 0 {
                return hr;
            }
            let write: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
                std::mem::transmute(slot(stm, 4));
            let mut done = 0u32;
            let hr = write(stm, data.as_ptr(), data.len() as u32, &mut done);
            com_release(stm);
            hr.min(S_OK)
        }
        Kind::Storage(children) => {
            if skip_storages {
                return S_OK;
            }
            let create: unsafe extern "win64" fn(
                usize,
                *const u16,
                u32,
                u32,
                u32,
                *mut usize,
            ) -> i32 = std::mem::transmute(slot(dest, 5));
            let mut sub = 0usize;
            let hr = create(dest, w.as_ptr(), mode, 0, 0, &mut sub);
            if hr < 0 {
                return hr;
            }
            let hr = copy_children_into(e, children, sub, &[], false, false);
            com_release(sub);
            hr
        }
    }
}

unsafe fn copy_children_into(
    parent: &Entry,
    children: &[Entry],
    dest: usize,
    exclude: &[String],
    skip_storages: bool,
    skip_streams: bool,
) -> i32 {
    let set_class: unsafe extern "win64" fn(usize, *const Guid) -> i32 =
        std::mem::transmute(slot(dest, 15));
    set_class(dest, &parent.clsid);
    for c in children {
        if exclude.iter().any(|x| same_name(x, &c.name)) {
            continue;
        }
        let hr = copy_entry_into(c, dest, &c.name, skip_storages, skip_streams);
        if hr < 0 {
            return hr;
        }
    }
    S_OK
}

unsafe fn snb_names(snb: *const *const u16) -> Vec<String> {
    let mut out = Vec::new();
    if snb.is_null() {
        return out;
    }
    let mut i = 0;
    while !(*snb.add(i)).is_null() && i < 4096 {
        out.extend(wide(*snb.add(i)));
        i += 1;
    }
    out
}

unsafe extern "win64" fn stg_copy_to(
    this: usize,
    ciid: u32,
    iids: *const Guid,
    snb: *const *const u16,
    dest: usize,
) -> i32 {
    if dest == 0 {
        return STG_E_INVALIDPOINTER;
    }
    let (mut skip_storages, mut skip_streams) = (false, false);
    if !iids.is_null() {
        for i in 0..ciid as usize {
            let g = std::ptr::read_unaligned(iids.add(i));
            skip_storages |= g == IID_ISTORAGE;
            skip_streams |= g == IID_ISTREAM;
        }
    }
    let exclude = snb_names(snb);
    let o = stg(this);
    let snapshot = {
        let d = doc(&o.doc);
        match find(&d.root, &o.path) {
            Some(e) => e.clone(),
            None => return STG_E_REVERTED,
        }
    };
    copy_children_into(
        &snapshot,
        snapshot.children(),
        dest,
        &exclude,
        skip_storages,
        skip_streams,
    )
}

unsafe extern "win64" fn stg_move_element_to(
    this: usize,
    name: *const u16,
    dest: usize,
    new_name: *const u16,
    flags: u32,
) -> i32 {
    if dest == 0 {
        return STG_E_INVALIDPOINTER;
    }
    let name = match element_name(name) {
        Ok(n) => n,
        Err(hr) => return hr,
    };
    let new_name = match element_name(new_name) {
        Ok(n) => n,
        Err(hr) => return hr,
    };
    let o = stg(this);
    let e = {
        let d = doc(&o.doc);
        let Some(node) = find(&d.root, &o.path) else {
            return STG_E_REVERTED;
        };
        match node.children().iter().find(|c| same_name(&c.name, &name)) {
            Some(e) => e.clone(),
            None => return STG_E_FILENOTFOUND,
        }
    };
    let hr = copy_entry_into(&e, dest, &new_name, false, false);
    if hr < 0 {
        return hr;
    }
    if flags == STGMOVE_MOVE {
        let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        return stg_destroy_element(this, w.as_ptr());
    }
    S_OK
}

unsafe extern "win64" fn stg_commit(this: usize, _flags: u32) -> i32 {
    let o = stg(this);
    let mut d = doc(&o.doc);
    if d.transacted && !o.path.is_empty() {
        return S_OK;
    }
    if d.dirty || d.transacted {
        return d.save();
    }
    S_OK
}

unsafe extern "win64" fn stg_revert(this: usize) -> i32 {
    let o = stg(this);
    let mut d = doc(&o.doc);
    if !d.transacted || !o.path.is_empty() {
        return S_OK;
    }
    d.load()
}

unsafe extern "win64" fn stg_enum_elements(
    this: usize,
    _r1: u32,
    _r2: usize,
    _r3: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    let o = stg(this);
    let items = {
        let d = doc(&o.doc);
        let Some(node) = find(&d.root, &o.path) else {
            return STG_E_REVERTED;
        };
        node.children()
            .iter()
            .map(|c| {
                let t = if matches!(c.kind, Kind::Storage(_)) {
                    STGTY_STORAGE
                } else {
                    STGTY_STREAM
                };
                (
                    c.name.clone(),
                    t,
                    c.size(),
                    c.clsid,
                    c.bits,
                    c.mtime,
                    c.ctime,
                )
            })
            .collect()
    };
    *out = new_enum(items, 0);
    S_OK
}

unsafe extern "win64" fn stg_destroy_element(this: usize, name: *const u16) -> i32 {
    let name = match element_name(name) {
        Ok(n) => n,
        Err(hr) => return hr,
    };
    let o = stg(this);
    let mut d = doc(&o.doc);
    if !d.writable {
        return STG_E_ACCESSDENIED;
    }
    let Some(node) = find_mut(&mut d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    let kids = node.children_mut().unwrap();
    let Some(i) = kids.iter().position(|c| same_name(&c.name, &name)) else {
        return STG_E_FILENOTFOUND;
    };
    kids.remove(i);
    d.dirty = true;
    S_OK
}

unsafe extern "win64" fn stg_rename_element(this: usize, old: *const u16, new: *const u16) -> i32 {
    let (old, new) = match (element_name(old), element_name(new)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(hr), _) | (_, Err(hr)) => return hr,
    };
    let o = stg(this);
    let mut d = doc(&o.doc);
    if !d.writable {
        return STG_E_ACCESSDENIED;
    }
    let Some(node) = find_mut(&mut d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    let kids = node.children_mut().unwrap();
    if !same_name(&old, &new) && kids.iter().any(|c| same_name(&c.name, &new)) {
        return STG_E_FILEALREADYEXISTS;
    }
    let Some(e) = kids.iter_mut().find(|c| same_name(&c.name, &old)) else {
        return STG_E_FILENOTFOUND;
    };
    e.name = new;
    d.dirty = true;
    S_OK
}

unsafe extern "win64" fn stg_set_element_times(
    this: usize,
    name: *const u16,
    ctime: *const u64,
    _atime: *const u64,
    mtime: *const u64,
) -> i32 {
    let o = stg(this);
    let mut path = o.path.clone();
    if !name.is_null() {
        match element_name(name) {
            Ok(n) => path.push(n),
            Err(hr) => return hr,
        }
    }
    let mut d = doc(&o.doc);
    if !d.writable {
        return STG_E_ACCESSDENIED;
    }
    let Some(e) = find_mut(&mut d.root, &path) else {
        return STG_E_FILENOTFOUND;
    };
    if !ctime.is_null() {
        e.ctime = std::ptr::read_unaligned(ctime);
    }
    if !mtime.is_null() {
        e.mtime = std::ptr::read_unaligned(mtime);
    }
    d.dirty = true;
    S_OK
}

unsafe extern "win64" fn stg_set_class(this: usize, clsid: *const Guid) -> i32 {
    if clsid.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let o = stg(this);
    let mut d = doc(&o.doc);
    if !d.writable {
        return STG_E_ACCESSDENIED;
    }
    let Some(e) = find_mut(&mut d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    e.clsid = std::ptr::read_unaligned(clsid);
    d.dirty = true;
    S_OK
}

unsafe extern "win64" fn stg_set_state_bits(this: usize, bits: u32, mask: u32) -> i32 {
    let o = stg(this);
    let mut d = doc(&o.doc);
    if !d.writable {
        return STG_E_ACCESSDENIED;
    }
    let Some(e) = find_mut(&mut d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    e.bits = (e.bits & !mask) | (bits & mask);
    d.dirty = true;
    S_OK
}

unsafe extern "win64" fn stg_stat(this: usize, out: *mut u8, flag: u32) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let o = stg(this);
    let d = doc(&o.doc);
    let Some(e) = find(&d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    let name = if o.path.is_empty() {
        match &d.backing {
            Backing::File { name, .. } => name.clone(),
            Backing::Bytes(_) => String::new(),
        }
    } else {
        e.name.clone()
    };
    let mut mode = if d.writable { STGM_READWRITE } else { 0 } | 0x10;
    if d.transacted {
        mode |= STGM_TRANSACTED;
    }
    let shown = (flag & STATFLAG_NONAME == 0).then_some(name.as_str());
    fill_stat(
        out,
        shown,
        STGTY_STORAGE,
        0,
        e.mtime,
        e.ctime,
        mode,
        &e.clsid,
        e.bits,
    );
    S_OK
}

// ── IStream ─────────────────────────────────────────────────────────

static STREAM_VTBL: Vtbl<14> = Vtbl([
    stm_query_interface as *const (),
    stm_add_ref as *const (),
    stm_release as *const (),
    stm_read as *const (),
    stm_write as *const (),
    stm_seek as *const (),
    stm_set_size as *const (),
    stm_copy_to as *const (),
    stm_commit as *const (),
    stm_revert as *const (),
    stm_lock_region as *const (),
    stm_lock_region as *const (), // UnlockRegion
    stm_stat as *const (),
    stm_clone as *const (),
]);

unsafe fn stm(this: usize) -> &'static StmObj {
    &*(this as *const StmObj)
}

unsafe extern "win64" fn stm_query_interface(
    this: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    if guid_eq(iid, &IID_IUNKNOWN)
        || guid_eq(iid, &IID_ISTREAM)
        || guid_eq(iid, &IID_ISEQUENTIALSTREAM)
    {
        stm_add_ref(this);
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn stm_add_ref(this: usize) -> u32 {
    stm(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn stm_release(this: usize) -> u32 {
    let n = stm(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut StmObj));
    }
    n
}

unsafe extern "win64" fn stm_read(this: usize, buf: *mut u8, cb: u32, read: *mut u32) -> i32 {
    let o = stm(this);
    let d = doc(&o.doc);
    let Some(Entry {
        kind: Kind::Stream(data),
        ..
    }) = find(&d.root, &o.path)
    else {
        return STG_E_REVERTED;
    };
    let mut pos = o.pos.lock().unwrap_or_else(|e| e.into_inner());
    let start = (*pos as usize).min(data.len());
    let n = (cb as usize).min(data.len() - start);
    if n > 0 {
        if buf.is_null() {
            return STG_E_INVALIDPOINTER;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr().add(start), buf, n);
    }
    *pos = (start + n) as u64;
    if !read.is_null() {
        *read = n as u32;
    }
    S_OK
}

unsafe extern "win64" fn stm_write(this: usize, buf: *const u8, cb: u32, written: *mut u32) -> i32 {
    if !written.is_null() {
        *written = 0;
    }
    let o = stm(this);
    let mut d = doc(&o.doc);
    if !d.writable {
        return STG_E_ACCESSDENIED;
    }
    if cb > 0 && buf.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let Some(e) = find_mut(&mut d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    let Kind::Stream(data) = &mut e.kind else {
        return STG_E_REVERTED;
    };
    let mut pos = o.pos.lock().unwrap_or_else(|e| e.into_inner());
    let start = *pos as usize;
    let end = start + cb as usize;
    if end > data.len() {
        data.resize(end, 0);
    }
    if cb > 0 {
        std::ptr::copy_nonoverlapping(buf, data.as_mut_ptr().add(start), cb as usize);
    }
    *pos = end as u64;
    e.mtime = now_filetime();
    d.dirty = true;
    if !written.is_null() {
        *written = cb;
    }
    S_OK
}

unsafe extern "win64" fn stm_seek(this: usize, mv: i64, origin: u32, newpos: *mut u64) -> i32 {
    let o = stm(this);
    let len = {
        let d = doc(&o.doc);
        match find(&d.root, &o.path) {
            Some(e) => e.size(),
            None => return STG_E_REVERTED,
        }
    };
    let mut pos = o.pos.lock().unwrap_or_else(|e| e.into_inner());
    let base: i128 = match origin {
        0 => 0,
        1 => *pos as i128,
        2 => len as i128,
        _ => return STG_E_INVALIDFUNCTION,
    };
    let target = base + mv as i128;
    if target < 0 {
        return STG_E_INVALIDFUNCTION;
    }
    *pos = target as u64;
    if !newpos.is_null() {
        *newpos = *pos;
    }
    S_OK
}

unsafe extern "win64" fn stm_set_size(this: usize, size: u64) -> i32 {
    let o = stm(this);
    let mut d = doc(&o.doc);
    if !d.writable {
        return STG_E_ACCESSDENIED;
    }
    if size > (1u64 << 31) {
        return STG_E_INSUFFICIENTMEMORY;
    }
    let Some(Entry {
        kind: Kind::Stream(data),
        ..
    }) = find_mut(&mut d.root, &o.path)
    else {
        return STG_E_REVERTED;
    };
    data.resize(size as usize, 0);
    d.dirty = true;
    S_OK
}

unsafe extern "win64" fn stm_copy_to(
    this: usize,
    dest: usize,
    cb: u64,
    read: *mut u64,
    written: *mut u64,
) -> i32 {
    if dest == 0 {
        return STG_E_INVALIDPOINTER;
    }
    let o = stm(this);
    let chunk = {
        let d = doc(&o.doc);
        let Some(Entry {
            kind: Kind::Stream(data),
            ..
        }) = find(&d.root, &o.path)
        else {
            return STG_E_REVERTED;
        };
        let pos = *o.pos.lock().unwrap_or_else(|e| e.into_inner());
        let start = (pos as usize).min(data.len());
        let n = (cb.min((data.len() - start) as u64)) as usize;
        data[start..start + n].to_vec()
    };
    *o.pos.lock().unwrap_or_else(|e| e.into_inner()) += chunk.len() as u64;
    let write: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(dest, 4));
    let mut done = 0u32;
    let hr = write(dest, chunk.as_ptr(), chunk.len() as u32, &mut done);
    if !read.is_null() {
        *read = chunk.len() as u64;
    }
    if !written.is_null() {
        *written = done as u64;
    }
    hr
}

unsafe extern "win64" fn stm_commit(this: usize, _flags: u32) -> i32 {
    let o = stm(this);
    let mut d = doc(&o.doc);
    if d.dirty && !d.transacted {
        return d.save();
    }
    S_OK
}
unsafe extern "win64" fn stm_revert(_this: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn stm_lock_region(_this: usize, _off: u64, _cb: u64, _kind: u32) -> i32 {
    STG_E_INVALIDFUNCTION
}

unsafe extern "win64" fn stm_stat(this: usize, out: *mut u8, flag: u32) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let o = stm(this);
    let d = doc(&o.doc);
    let Some(e) = find(&d.root, &o.path) else {
        return STG_E_REVERTED;
    };
    let mode = if d.writable { STGM_READWRITE } else { 0 } | 0x10;
    let shown = (flag & STATFLAG_NONAME == 0).then_some(e.name.as_str());
    fill_stat(
        out,
        shown,
        STGTY_STREAM,
        e.size(),
        e.mtime,
        e.ctime,
        mode,
        &Guid::default(),
        0,
    );
    S_OK
}

unsafe extern "win64" fn stm_clone(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let o = stm(this);
    let pos = *o.pos.lock().unwrap_or_else(|e| e.into_inner());
    *out = new_stream(o.doc.clone(), o.path.clone(), pos);
    S_OK
}

// ── IEnumSTATSTG ────────────────────────────────────────────────────

static ENUM_VTBL: Vtbl<7> = Vtbl([
    enum_query_interface as *const (),
    enum_add_ref as *const (),
    enum_release as *const (),
    enum_next as *const (),
    enum_skip as *const (),
    enum_reset as *const (),
    enum_clone as *const (),
]);

unsafe fn new_enum(items: Vec<(String, u32, u64, Guid, u32, u64, u64)>, pos: usize) -> usize {
    Box::into_raw(Box::new(EnumObj {
        vtbl: ENUM_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        items,
        pos: Mutex::new(pos),
    })) as usize
}
unsafe fn en(this: usize) -> &'static EnumObj {
    &*(this as *const EnumObj)
}
unsafe extern "win64" fn enum_query_interface(
    this: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    if guid_eq(iid, &IID_IUNKNOWN) || guid_eq(iid, &IID_IENUMSTATSTG) {
        enum_add_ref(this);
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn enum_add_ref(this: usize) -> u32 {
    en(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn enum_release(this: usize) -> u32 {
    let n = en(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut EnumObj));
    }
    n
}
unsafe extern "win64" fn enum_next(this: usize, celt: u32, out: *mut u8, fetched: *mut u32) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let o = en(this);
    let mut pos = o.pos.lock().unwrap_or_else(|e| e.into_inner());
    let mut n = 0usize;
    while n < celt as usize && *pos < o.items.len() {
        let (name, t, size, clsid, bits, mtime, ctime) = &o.items[*pos];
        fill_stat(
            out.add(n * 80),
            Some(name),
            *t,
            *size,
            *mtime,
            *ctime,
            0,
            clsid,
            *bits,
        );
        n += 1;
        *pos += 1;
    }
    if !fetched.is_null() {
        *fetched = n as u32;
    }
    if n == celt as usize {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn enum_skip(this: usize, celt: u32) -> i32 {
    let o = en(this);
    let mut pos = o.pos.lock().unwrap_or_else(|e| e.into_inner());
    *pos += celt as usize;
    if *pos <= o.items.len() {
        S_OK
    } else {
        S_FALSE
    }
}
unsafe extern "win64" fn enum_reset(this: usize) -> i32 {
    *en(this).pos.lock().unwrap_or_else(|e| e.into_inner()) = 0;
    S_OK
}
unsafe extern "win64" fn enum_clone(this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let o = en(this);
    let pos = *o.pos.lock().unwrap_or_else(|e| e.into_inner());
    *out = new_enum(o.items.clone(), pos);
    S_OK
}

// ── ILockBytes sur HGLOBAL ──────────────────────────────────────────

static LOCKBYTES_VTBL: Vtbl<10> = Vtbl([
    lb_query_interface as *const (),
    lb_add_ref as *const (),
    lb_release as *const (),
    lb_read_at as *const (),
    lb_write_at as *const (),
    lb_flush as *const (),
    lb_set_size as *const (),
    stm_lock_region as *const (), // LockRegion
    stm_lock_region as *const (), // UnlockRegion
    lb_stat as *const (),
]);

unsafe fn lb(this: usize) -> &'static LockBytesObj {
    &*(this as *const LockBytesObj)
}
unsafe extern "win64" fn lb_query_interface(this: usize, iid: *const Guid, out: *mut usize) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    if guid_eq(iid, &IID_IUNKNOWN) || guid_eq(iid, &IID_ILOCKBYTES) {
        lb_add_ref(this);
        *out = this;
        return S_OK;
    }
    *out = 0;
    E_NOINTERFACE
}
unsafe extern "win64" fn lb_add_ref(this: usize) -> u32 {
    lb(this).refs.fetch_add(1, Ordering::AcqRel) + 1
}
unsafe extern "win64" fn lb_release(this: usize) -> u32 {
    let n = lb(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let o = Box::from_raw(this as *mut LockBytesObj);
        if o.delete_on_release {
            super::kernel32::free_global_memory(o.global);
        }
    }
    n
}
unsafe extern "win64" fn lb_read_at(
    this: usize,
    off: u64,
    buf: *mut u8,
    cb: u32,
    read: *mut u32,
) -> i32 {
    let m = lb(this).memory.lock().unwrap_or_else(|e| e.into_inner());
    let start = (off as usize).min(m.bytes.len());
    let n = (cb as usize).min(m.bytes.len() - start);
    if n > 0 {
        if buf.is_null() {
            return STG_E_INVALIDPOINTER;
        }
        std::ptr::copy_nonoverlapping(m.bytes.as_ptr().add(start), buf, n);
    }
    if !read.is_null() {
        *read = n as u32;
    }
    S_OK
}
unsafe extern "win64" fn lb_write_at(
    this: usize,
    off: u64,
    buf: *const u8,
    cb: u32,
    written: *mut u32,
) -> i32 {
    if cb > 0 && buf.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let mut m = lb(this).memory.lock().unwrap_or_else(|e| e.into_inner());
    let end = off as usize + cb as usize;
    if end > m.bytes.len() {
        m.bytes.resize(end, 0);
    }
    if cb > 0 {
        std::ptr::copy_nonoverlapping(buf, m.bytes.as_mut_ptr().add(off as usize), cb as usize);
    }
    if !written.is_null() {
        *written = cb;
    }
    S_OK
}
unsafe extern "win64" fn lb_flush(_this: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn lb_set_size(this: usize, size: u64) -> i32 {
    if size > (1u64 << 31) {
        return STG_E_INSUFFICIENTMEMORY;
    }
    lb(this)
        .memory
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .bytes
        .resize(size as usize, 0);
    S_OK
}
unsafe extern "win64" fn lb_stat(this: usize, out: *mut u8, _flag: u32) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let len = lb(this)
        .memory
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .bytes
        .len();
    fill_stat(
        out,
        None,
        STGTY_LOCKBYTES,
        len as u64,
        0,
        0,
        0,
        &Guid::default(),
        0,
    );
    S_OK
}

// ── Fonctions exportées ─────────────────────────────────────────────

unsafe fn open_doc(backing: Backing, mode: u32, root: Entry, out: *mut usize) {
    let d = Doc {
        root,
        backing,
        writable: write_mode(mode),
        transacted: mode & STGM_TRANSACTED != 0,
        dirty: false,
        delete_on_release: mode & STGM_DELETEONRELEASE != 0,
    };
    *out = new_storage(Arc::new(DocCell(Mutex::new(d))), Vec::new());
}

unsafe fn temp_docfile() -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir();
    let p = dir.join(format!(
        "~stg{}-{}.tmp",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    p.to_string_lossy().into_owned()
}

pub unsafe extern "win64" fn stg_create_docfile(
    name: *const u16,
    mode: u32,
    _reserved: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let win = wide(name).unwrap_or_else(|| temp_docfile());
    let path = crate::paths::to_linux(&win);
    let mut root = Entry::storage("Root Entry");
    let mut hr = S_OK;
    if path.exists() {
        if mode & STGM_CONVERT != 0 {
            let old = std::fs::read(&path).unwrap_or_default();
            root.children_mut()
                .unwrap()
                .push(Entry::stream("CONTENTS", old));
            hr = STG_S_CONVERTED;
        } else if mode & STGM_CREATE == 0 {
            return STG_E_FILEALREADYEXISTS;
        }
    }
    let mut d = Doc {
        root,
        backing: Backing::File { path, name: win },
        writable: true,
        transacted: mode & STGM_TRANSACTED != 0,
        dirty: false,
        delete_on_release: mode & STGM_DELETEONRELEASE != 0,
    };
    let saved = d.save();
    if saved < 0 {
        return saved;
    }
    d.writable = write_mode(mode);
    *out = new_storage(Arc::new(DocCell(Mutex::new(d))), Vec::new());
    hr
}

pub unsafe extern "win64" fn stg_open_storage(
    name: *const u16,
    priority: usize,
    mode: u32,
    _exclude: usize,
    _reserved: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    if name.is_null() && is_ours(priority) {
        // Ouverture depuis une instance prioritaire : même support.
        let o = stg(priority);
        let d = doc(&o.doc);
        if let Backing::File { name, .. } = &d.backing {
            let w: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            drop(d);
            return stg_open_storage(w.as_ptr(), 0, mode, 0, 0, out);
        }
        return STG_E_INVALIDPARAMETER;
    }
    let Some(win) = wide(name) else {
        return STG_E_INVALIDNAME;
    };
    let path = crate::paths::to_linux(&win);
    let Ok(bytes) = std::fs::read(&path) else {
        return STG_E_FILENOTFOUND;
    };
    let root = match parse_cfb(&bytes) {
        Ok(r) => r,
        Err(hr) => return hr,
    };
    open_doc(Backing::File { path, name: win }, mode, root, out);
    S_OK
}

pub unsafe extern "win64" fn stg_create_docfile_on_ilockbytes(
    lb: usize,
    mode: u32,
    _r: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() || lb == 0 {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    com_add_ref(lb);
    let mut d = Doc {
        root: Entry::storage("Root Entry"),
        backing: Backing::Bytes(lb),
        writable: true,
        transacted: mode & STGM_TRANSACTED != 0,
        dirty: false,
        delete_on_release: false,
    };
    let hr = d.save();
    d.writable = write_mode(mode);
    let cell = Arc::new(DocCell(Mutex::new(d)));
    if hr < 0 {
        return hr;
    }
    *out = new_storage(cell, Vec::new());
    S_OK
}

pub unsafe extern "win64" fn stg_open_storage_on_ilockbytes(
    lb: usize,
    _priority: usize,
    mode: u32,
    _exclude: usize,
    _r: u32,
    out: *mut usize,
) -> i32 {
    if out.is_null() || lb == 0 {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let bytes = match lockbytes_read_all(lb) {
        Ok(b) => b,
        Err(hr) => return hr,
    };
    let root = match parse_cfb(&bytes) {
        Ok(r) => r,
        Err(hr) => return hr,
    };
    com_add_ref(lb);
    open_doc(Backing::Bytes(lb), mode, root, out);
    S_OK
}

pub unsafe extern "win64" fn stg_is_storage_file(name: *const u16) -> i32 {
    let Some(win) = wide(name) else {
        return STG_E_INVALIDNAME;
    };
    let mut head = [0u8; 512];
    let Ok(mut f) = std::fs::File::open(crate::paths::to_linux(&win)) else {
        return STG_E_FILENOTFOUND;
    };
    use std::io::Read;
    let n = f.read(&mut head).unwrap_or(0);
    if is_cfb(&head[..n]) {
        S_OK
    } else {
        S_FALSE
    }
}

pub unsafe extern "win64" fn stg_is_storage_ilockbytes(lb: usize) -> i32 {
    if lb == 0 {
        return STG_E_INVALIDPOINTER;
    }
    let mut head = [0u8; 512];
    let mut got = 0u32;
    let read_at: unsafe extern "win64" fn(usize, u64, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(lb, 3));
    let hr = read_at(lb, 0, head.as_mut_ptr(), 512, &mut got);
    if hr < 0 {
        return hr;
    }
    if is_cfb(&head[..got as usize]) {
        S_OK
    } else {
        S_FALSE
    }
}

pub unsafe extern "win64" fn stg_create_storage_ex(
    name: *const u16,
    mode: u32,
    fmt: u32,
    _attrs: u32,
    _options: usize,
    _sd: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    if fmt != STGFMT_STORAGE && fmt != STGFMT_DOCFILE {
        return STG_E_INVALIDPARAMETER;
    }
    let mut s = 0usize;
    let hr = stg_create_docfile(name, mode, 0, &mut s);
    if hr < 0 {
        return hr;
    }
    let q = stg_query_interface(s, iid, out);
    stg_release(s);
    if q < 0 {
        q
    } else {
        hr
    }
}

pub unsafe extern "win64" fn stg_open_storage_ex(
    name: *const u16,
    mode: u32,
    fmt: u32,
    _attrs: u32,
    _options: usize,
    _sd: usize,
    iid: *const Guid,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    if fmt != STGFMT_STORAGE && fmt != STGFMT_DOCFILE && fmt != STGFMT_ANY {
        return STG_E_INVALIDPARAMETER;
    }
    let mut s = 0usize;
    let hr = stg_open_storage(name, 0, mode, 0, 0, &mut s);
    if hr < 0 {
        return hr;
    }
    let q = stg_query_interface(s, iid, out);
    stg_release(s);
    q
}

pub unsafe extern "win64" fn read_class_stg(storage: usize, clsid: *mut Guid) -> i32 {
    if storage == 0 || clsid.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let mut st = [0u8; 80];
    let stat: unsafe extern "win64" fn(usize, *mut u8, u32) -> i32 =
        std::mem::transmute(slot(storage, 17));
    let hr = stat(storage, st.as_mut_ptr(), STATFLAG_NONAME);
    if hr < 0 {
        return hr;
    }
    std::ptr::write_unaligned(
        clsid,
        std::ptr::read_unaligned(st.as_ptr().add(56) as *const Guid),
    );
    S_OK
}

pub unsafe extern "win64" fn write_class_stg(storage: usize, clsid: *const Guid) -> i32 {
    if storage == 0 || clsid.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let set: unsafe extern "win64" fn(usize, *const Guid) -> i32 =
        std::mem::transmute(slot(storage, 15));
    set(storage, clsid)
}

pub unsafe extern "win64" fn read_class_stm(stream: usize, clsid: *mut Guid) -> i32 {
    if stream == 0 || clsid.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let mut b = [0u8; 16];
    let mut got = 0u32;
    let read: unsafe extern "win64" fn(usize, *mut u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stream, 3));
    let hr = read(stream, b.as_mut_ptr(), 16, &mut got);
    if hr < 0 {
        return hr;
    }
    if got != 16 {
        return STG_E_READFAULT;
    }
    std::ptr::write_unaligned(clsid, guid_at(&b, 0));
    S_OK
}

pub unsafe extern "win64" fn write_class_stm(stream: usize, clsid: *const Guid) -> i32 {
    if stream == 0 || clsid.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    let mut b = [0u8; 16];
    put_guid(&mut b, 0, &std::ptr::read_unaligned(clsid));
    let write: unsafe extern "win64" fn(usize, *const u8, u32, *mut u32) -> i32 =
        std::mem::transmute(slot(stream, 4));
    let mut done = 0u32;
    write(stream, b.as_ptr(), 16, &mut done)
}

pub unsafe extern "win64" fn create_ilockbytes_on_hglobal(
    global: usize,
    delete: i32,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    *out = 0;
    if crate::wow32::active() {
        return E_NOTIMPL;
    }
    let (global, memory) = if global == 0 {
        match super::kernel32::create_global_memory(0, 2) {
            Some(c) => c,
            None => return STG_E_INSUFFICIENTMEMORY,
        }
    } else {
        match super::kernel32::lookup_global_memory(global) {
            Some(m) => (global, m),
            None => return STG_E_INVALIDPARAMETER,
        }
    };
    *out = Box::into_raw(Box::new(LockBytesObj {
        vtbl: LOCKBYTES_VTBL.0.as_ptr().cast(),
        refs: AtomicU32::new(1),
        global,
        memory,
        delete_on_release: delete != 0,
    })) as usize;
    S_OK
}

pub unsafe extern "win64" fn get_hglobal_from_ilockbytes(lbp: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return STG_E_INVALIDPOINTER;
    }
    if lbp == 0 || *(lbp as *const *const usize) != LOCKBYTES_VTBL.0.as_ptr().cast() {
        *out = 0;
        return STG_E_INVALIDPARAMETER;
    }
    *out = lb(lbp).global;
    S_OK
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "StgCreateDocfile" => fn stg_create_docfile,
        "StgOpenStorage" => fn stg_open_storage,
        "StgCreateDocfileOnILockBytes" => fn stg_create_docfile_on_ilockbytes,
        "StgOpenStorageOnILockBytes" => fn stg_open_storage_on_ilockbytes,
        "StgIsStorageFile" => fn stg_is_storage_file,
        "StgIsStorageILockBytes" => fn stg_is_storage_ilockbytes,
        "StgCreateStorageEx" => fn stg_create_storage_ex,
        "StgOpenStorageEx" => fn stg_open_storage_ex,
        "ReadClassStg" => fn read_class_stg,
        "WriteClassStg" => fn write_class_stg,
        "ReadClassStm" => fn read_class_stm,
        "WriteClassStm" => fn write_class_stm,
        "CreateILockBytesOnHGlobal" => fn create_ilockbytes_on_hglobal,
        "GetHGlobalFromILockBytes" => fn get_hglobal_from_ilockbytes
    }
}

#[cfg(test)]
mod tests;
