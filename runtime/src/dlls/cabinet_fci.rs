//! cabinet.dll : création de cabinets (FCI, lot D du relevé, #1512).
//!
//! electron-winstaller et panda3d importent FCICreate, FCIAddFile,
//! FCIFlushFolder, FCIFlushCabinet et FCIDestroy par leurs numéros d'ordre
//! (10 à 14). L'API suit la documentation publique de fci.h : l'application
//! fournit les rappels d'entrée-sortie, FCI lit chaque source par
//! pfnfcigoi/pfnread et écrit le cabinet par pfnopen/pfnwrite.
//!
//! Winlux garde le contenu des dossiers en mémoire au lieu de passer par
//! des fichiers temporaires (pfnfcigtf, pfnseek et pfndelete ne servent
//! donc pas). Les dossiers sont écrits en MSZIP (DEFLATE par blocs de
//! 32 Kio) dès qu'une compression est demandée, y compris pour LZX et
//! Quantum : le format du cabinet note la compression réelle de chaque
//! dossier, et tout lecteur de cabinets sait l'extraire. Un dossier
//! demandé sans compression est stocké tel quel. Le découpage en plusieurs
//! cabinets selon `CCAB.cb` n'est pas fait : chaque FCIFlushCabinet écrit
//! un seul cabinet complet.

#![allow(clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::io::Write;

type PfnFilePlaced = unsafe extern "win64" fn(*mut u8, *const u8, i32, i32, *mut c_void) -> i32;
type PfnOpen = unsafe extern "win64" fn(*const u8, i32, i32, *mut i32, *mut c_void) -> isize;
type PfnRead = unsafe extern "win64" fn(isize, *mut c_void, u32, *mut i32, *mut c_void) -> u32;
type PfnWrite = unsafe extern "win64" fn(isize, *const c_void, u32, *mut i32, *mut c_void) -> u32;
type PfnClose = unsafe extern "win64" fn(isize, *mut i32, *mut c_void) -> i32;
type PfnGetNextCab = unsafe extern "win64" fn(*mut u8, u32, *mut c_void) -> i32;
type PfnStatus = unsafe extern "win64" fn(u32, u32, u32, *mut c_void) -> i32;
type PfnGetOpenInfo = unsafe extern "win64" fn(
    *const u8,
    *mut u16,
    *mut u16,
    *mut u16,
    *mut i32,
    *mut c_void,
) -> isize;

const FCIERR_OPEN_SRC: i32 = 1;
const FCIERR_READ_SRC: i32 = 2;
const FCIERR_BAD_COMPR_TYPE: i32 = 5;
const FCIERR_CAB_FILE: i32 = 6;
const FCIERR_USER_ABORT: i32 = 7;

const STATUS_FILE: u32 = 0;
const STATUS_FOLDER: u32 = 1;
const STATUS_CABINET: u32 = 2;

/// CCAB (fci.h) : 802 octets, 804 avec l'alignement.
const CCAB_SIZE: usize = 804;
const CCAB_RESERVE_HEADER: usize = 8;
const CCAB_RESERVE_FOLDER: usize = 12;
const CCAB_RESERVE_DATA: usize = 16;
const CCAB_FOLDER_THRESH: usize = 4;
const CCAB_ICAB: usize = 20;
const CCAB_SET_ID: usize = 32;
const CCAB_CAB: usize = 290;
const CCAB_CAB_PATH: usize = 546;

/// _O_RDWR | _O_BINARY | _O_CREAT | _O_TRUNC, puis _S_IREAD | _S_IWRITE.
const OPEN_CREATE: i32 = 0x0002 | 0x8000 | 0x0100 | 0x0200;
const PMODE_RW: i32 = 0x0100 | 0x0080;
/// _A_EXEC : le fichier est à lancer après extraction.
const A_EXEC: u16 = 0x40;
const BLOCK: usize = 32 * 1024;

#[derive(Clone, Default)]
pub struct FileEntry {
    pub name: Vec<u8>,
    pub offset: u32,
    pub size: u32,
    pub date: u16,
    pub time: u16,
    pub attribs: u16,
}

#[derive(Clone, Default)]
pub struct Folder {
    /// 0 : stocké ; 1 : MSZIP.
    pub compression: u16,
    pub files: Vec<FileEntry>,
    pub data: Vec<u8>,
}

/// Réserves par en-tête, dossier et bloc (CCAB.cbReserveCF*).
#[derive(Clone, Copy, Default)]
pub struct Reserve {
    pub header: usize,
    pub folder: usize,
    pub data: usize,
}

/// Un bloc CFDATA compressé en MSZIP : « CK » puis un flux DEFLATE
/// complet. Si DEFLATE n'y gagne rien, un bloc DEFLATE stocké.
fn mszip_block(chunk: &[u8]) -> Vec<u8> {
    let mut out = b"CK".to_vec();
    let packed = crate::archive::deflate::Deflater::new(Vec::new());
    let packed = {
        let mut d = packed;
        d.write_all(chunk).ok().and_then(|_| d.finish().ok())
    };
    match packed {
        Some(p) if p.len() < chunk.len() + 5 => out.extend_from_slice(&p),
        _ => {
            out.push(1);
            out.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            out.extend_from_slice(&(!(chunk.len() as u16)).to_le_bytes());
            out.extend_from_slice(chunk);
        }
    }
    out
}

/// Construit un cabinet complet (MS-CAB) à partir de dossiers en mémoire.
pub fn build_cabinet(folders: &[Folder], set_id: u16, index: u16, r: Reserve) -> Vec<u8> {
    let has_reserve = r.header + r.folder + r.data > 0;
    let header_len = 36 + if has_reserve { 4 + r.header } else { 0 };
    let folders_len = folders.len() * (8 + r.folder);
    let mut file_table = Vec::new();
    let mut n_files = 0u16;
    for (i, f) in folders.iter().enumerate() {
        for e in &f.files {
            file_table.extend_from_slice(&e.size.to_le_bytes());
            file_table.extend_from_slice(&e.offset.to_le_bytes());
            file_table.extend_from_slice(&(i as u16).to_le_bytes());
            file_table.extend_from_slice(&e.date.to_le_bytes());
            file_table.extend_from_slice(&e.time.to_le_bytes());
            file_table.extend_from_slice(&e.attribs.to_le_bytes());
            file_table.extend_from_slice(&e.name);
            file_table.push(0);
            n_files = n_files.wrapping_add(1);
        }
    }
    let files_at = header_len + folders_len;
    // Blocs de données de chaque dossier.
    let mut blocks: Vec<Vec<Vec<u8>>> = Vec::new();
    for f in folders {
        let mut list = Vec::new();
        for chunk in f.data.chunks(BLOCK) {
            let packed = if f.compression == 1 {
                mszip_block(chunk)
            } else {
                chunk.to_vec()
            };
            let mut b = Vec::with_capacity(8 + r.data + packed.len());
            b.extend_from_slice(&[0; 4]); // somme de contrôle facultative
            b.extend_from_slice(&(packed.len() as u16).to_le_bytes());
            b.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            b.extend(std::iter::repeat_n(0u8, r.data));
            b.extend_from_slice(&packed);
            list.push(b);
        }
        blocks.push(list);
    }
    let data_at = files_at + file_table.len();
    let total = data_at + blocks.iter().flatten().map(Vec::len).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"MSCF");
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files_at as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&[3, 1]);
    out.extend_from_slice(&(folders.len() as u16).to_le_bytes());
    out.extend_from_slice(&n_files.to_le_bytes());
    out.extend_from_slice(&(if has_reserve { 4u16 } else { 0 }).to_le_bytes());
    out.extend_from_slice(&set_id.to_le_bytes());
    out.extend_from_slice(&index.to_le_bytes());
    if has_reserve {
        out.extend_from_slice(&(r.header as u16).to_le_bytes());
        out.push(r.folder as u8);
        out.push(r.data as u8);
        out.extend(std::iter::repeat_n(0u8, r.header));
    }
    let mut at = data_at;
    for (f, list) in folders.iter().zip(&blocks) {
        out.extend_from_slice(&(at as u32).to_le_bytes());
        out.extend_from_slice(&(list.len() as u16).to_le_bytes());
        out.extend_from_slice(&f.compression.to_le_bytes());
        out.extend(std::iter::repeat_n(0u8, r.folder));
        at += list.iter().map(Vec::len).sum::<usize>();
    }
    out.extend_from_slice(&file_table);
    for b in blocks.iter().flatten() {
        out.extend_from_slice(b);
    }
    out
}

/// Contexte FCI (HFCI).
struct Fci {
    file_placed: Option<PfnFilePlaced>,
    open: PfnOpen,
    read: PfnRead,
    write: PfnWrite,
    close: PfnClose,
    erf: *mut i32,
    pv: *mut c_void,
    ccab: Box<[u8; CCAB_SIZE]>,
    folders: Vec<Folder>,
    current: Folder,
}

unsafe fn set_erf(erf: *mut i32, oper: i32, kind: i32) -> i32 {
    if !erf.is_null() {
        *erf = oper;
        *erf.add(1) = kind;
        *erf.add(2) = i32::from(oper != 0);
    }
    0
}

impl Fci {
    fn u32_at(&self, o: usize) -> usize {
        u32::from_le_bytes(self.ccab[o..o + 4].try_into().unwrap_or_default()) as usize
    }

    fn reserve(&self) -> Reserve {
        Reserve {
            header: self.u32_at(CCAB_RESERVE_HEADER).min(60_000),
            folder: self.u32_at(CCAB_RESERVE_FOLDER).min(255),
            data: self.u32_at(CCAB_RESERVE_DATA).min(255),
        }
    }

    fn text_at(&self, o: usize) -> Vec<u8> {
        let s = &self.ccab[o..o + 256];
        s[..s.iter().position(|&c| c == 0).unwrap_or(256)].to_vec()
    }

    fn flush_folder(&mut self, status: Option<PfnStatus>) -> bool {
        if self.current.files.is_empty() {
            return true;
        }
        let size = self.current.data.len() as u32;
        let compression = self.current.compression;
        self.folders.push(std::mem::take(&mut self.current));
        self.current.compression = compression;
        if let Some(s) = status {
            if unsafe { s(STATUS_FOLDER, size, size, self.pv) } == -1 {
                return false;
            }
        }
        true
    }
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn fci_create(
    erf: *mut i32,
    file_placed: Option<PfnFilePlaced>,
    _alloc: usize,
    _free: usize,
    open: Option<PfnOpen>,
    read: Option<PfnRead>,
    write: Option<PfnWrite>,
    close: Option<PfnClose>,
    _seek: usize,
    _delete: usize,
    _temp: usize,
    ccab: *const u8,
    pv: *mut c_void,
) -> usize {
    let (Some(open), Some(read), Some(write), Some(close)) = (open, read, write, close) else {
        set_erf(erf, FCIERR_OPEN_SRC, 0);
        return 0;
    };
    if ccab.is_null() {
        set_erf(erf, FCIERR_CAB_FILE, 0);
        return 0;
    }
    let mut copy = Box::new([0u8; CCAB_SIZE]);
    std::ptr::copy_nonoverlapping(ccab, copy.as_mut_ptr(), CCAB_SIZE);
    set_erf(erf, 0, 0);
    Box::into_raw(Box::new(Fci {
        file_placed,
        open,
        read,
        write,
        close,
        erf,
        pv,
        ccab: copy,
        folders: Vec::new(),
        current: Folder::default(),
    })) as usize
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn fci_add_file(
    h: usize,
    source: *const u8,
    name: *const u8,
    execute: i32,
    _next_cab: Option<PfnGetNextCab>,
    status: Option<PfnStatus>,
    open_info: Option<PfnGetOpenInfo>,
    compression: u16,
) -> i32 {
    if h == 0 {
        return 0;
    }
    let fci = &mut *(h as *mut Fci);
    let kind = compression & 0x000F;
    if kind > 3 {
        return set_erf(fci.erf, FCIERR_BAD_COMPR_TYPE, 0);
    }
    let Some(open_info) = open_info.filter(|_| !source.is_null() && !name.is_null()) else {
        return set_erf(fci.erf, FCIERR_OPEN_SRC, 0);
    };
    let (mut date, mut time, mut attribs, mut err) = (0u16, 0u16, 0u16, 0i32);
    let hf = open_info(source, &mut date, &mut time, &mut attribs, &mut err, fci.pv);
    if hf == -1 {
        return set_erf(fci.erf, FCIERR_OPEN_SRC, err);
    }
    let mut bytes = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = (fci.read)(
            hf,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
            &mut err,
            fci.pv,
        );
        if n == u32::MAX {
            (fci.close)(hf, &mut err, fci.pv);
            return set_erf(fci.erf, FCIERR_READ_SRC, err);
        }
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n as usize]);
    }
    (fci.close)(hf, &mut err, fci.pv);
    let wanted = u16::from(kind != 0);
    if wanted != fci.current.compression
        && !fci.current.files.is_empty()
        && !fci.flush_folder(status)
    {
        return set_erf(fci.erf, FCIERR_USER_ABORT, 0);
    }
    fci.current.compression = wanted;
    let name_bytes = std::ffi::CStr::from_ptr(name.cast()).to_bytes().to_vec();
    if let Some(placed) = fci.file_placed {
        let cont = 0;
        if placed(
            fci.ccab.as_mut_ptr(),
            name,
            bytes.len() as i32,
            cont,
            fci.pv,
        ) == -1
        {
            return set_erf(fci.erf, FCIERR_USER_ABORT, 0);
        }
    }
    let size = bytes.len() as u32;
    fci.current.files.push(FileEntry {
        name: name_bytes,
        offset: fci.current.data.len() as u32,
        size,
        date,
        time,
        attribs: if execute != 0 {
            attribs | A_EXEC
        } else {
            attribs
        },
    });
    fci.current.data.extend_from_slice(&bytes);
    if let Some(s) = status {
        if s(STATUS_FILE, size, size, fci.pv) == -1 {
            return set_erf(fci.erf, FCIERR_USER_ABORT, 0);
        }
    }
    let thresh = fci.u32_at(CCAB_FOLDER_THRESH);
    if thresh > 0 && fci.current.data.len() >= thresh && !fci.flush_folder(status) {
        return set_erf(fci.erf, FCIERR_USER_ABORT, 0);
    }
    set_erf(fci.erf, 0, 0);
    1
}

unsafe extern "win64" fn fci_flush_folder(
    h: usize,
    _next_cab: Option<PfnGetNextCab>,
    status: Option<PfnStatus>,
) -> i32 {
    if h == 0 {
        return 0;
    }
    let fci = &mut *(h as *mut Fci);
    if !fci.flush_folder(status) {
        return set_erf(fci.erf, FCIERR_USER_ABORT, 0);
    }
    set_erf(fci.erf, 0, 0);
    1
}

unsafe extern "win64" fn fci_flush_cabinet(
    h: usize,
    get_next: i32,
    next_cab: Option<PfnGetNextCab>,
    status: Option<PfnStatus>,
) -> i32 {
    if h == 0 {
        return 0;
    }
    let fci = &mut *(h as *mut Fci);
    if !fci.flush_folder(status) {
        return set_erf(fci.erf, FCIERR_USER_ABORT, 0);
    }
    let set_id = u16::from_le_bytes([fci.ccab[CCAB_SET_ID], fci.ccab[CCAB_SET_ID + 1]]);
    let index = fci.u32_at(CCAB_ICAB) as u16;
    let cab = build_cabinet(&fci.folders, set_id, index, fci.reserve());
    if let Some(s) = status {
        if s(STATUS_CABINET, cab.len() as u32, cab.len() as u32, fci.pv) == -1 {
            return set_erf(fci.erf, FCIERR_USER_ABORT, 0);
        }
    }
    let mut path = fci.text_at(CCAB_CAB_PATH);
    path.extend_from_slice(&fci.text_at(CCAB_CAB));
    path.push(0);
    let mut err = 0;
    let hf = (fci.open)(path.as_ptr(), OPEN_CREATE, PMODE_RW, &mut err, fci.pv);
    if hf == -1 {
        return set_erf(fci.erf, FCIERR_CAB_FILE, err);
    }
    for chunk in cab.chunks(64 * 1024) {
        let n = (fci.write)(
            hf,
            chunk.as_ptr().cast(),
            chunk.len() as u32,
            &mut err,
            fci.pv,
        );
        if n != chunk.len() as u32 {
            (fci.close)(hf, &mut err, fci.pv);
            return set_erf(fci.erf, FCIERR_CAB_FILE, err);
        }
    }
    if (fci.close)(hf, &mut err, fci.pv) == -1 {
        return set_erf(fci.erf, FCIERR_CAB_FILE, err);
    }
    fci.folders.clear();
    if get_next != 0 {
        let next = (index as u32 + 1).to_le_bytes();
        fci.ccab[CCAB_ICAB..CCAB_ICAB + 4].copy_from_slice(&next);
        if let Some(f) = next_cab {
            if f(fci.ccab.as_mut_ptr(), cab.len() as u32, fci.pv) == 0 {
                return set_erf(fci.erf, FCIERR_USER_ABORT, 0);
            }
        }
    }
    set_erf(fci.erf, 0, 0);
    1
}

unsafe extern "win64" fn fci_destroy(h: usize) -> i32 {
    if h == 0 {
        return 0;
    }
    drop(Box::from_raw(h as *mut Fci));
    1
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "FCICreate" => fn fci_create,
        "FCIAddFile" => fn fci_add_file,
        "FCIFlushFolder" => fn fci_flush_folder,
        "FCIFlushCabinet" => fn fci_flush_cabinet,
        "FCIDestroy" => fn fci_destroy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msi::cab::Cabinet;

    fn roundtrip(folders: &[Folder], r: Reserve) -> Vec<(String, Vec<u8>)> {
        let cab = build_cabinet(folders, 7, 0, r);
        let parsed = Cabinet::parse(&cab).unwrap();
        let mut got = Vec::new();
        parsed
            .extract(&mut |n, d| {
                got.push((n.to_string(), d.to_vec()));
                Ok(())
            })
            .unwrap();
        got
    }

    fn folder(compression: u16, files: &[(&str, &[u8])]) -> Folder {
        let mut f = Folder {
            compression,
            ..Default::default()
        };
        for (n, d) in files {
            f.files.push(FileEntry {
                name: n.as_bytes().to_vec(),
                offset: f.data.len() as u32,
                size: d.len() as u32,
                attribs: 0x20,
                ..Default::default()
            });
            f.data.extend_from_slice(d);
        }
        f
    }

    #[test]
    fn cabinet_stocke_et_mszip_relus() {
        let big: Vec<u8> = (0..100_000u32).map(|i| (i % 97) as u8).collect();
        let noise: Vec<u8> = (0..40_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        let folders = [
            folder(0, &[("a.txt", b"bonjour"), ("b.bin", &noise)]),
            folder(1, &[("big.bin", &big), ("c.txt", b"")]),
        ];
        for r in [
            Reserve::default(),
            Reserve {
                header: 6,
                folder: 2,
                data: 3,
            },
        ] {
            let got = roundtrip(&folders, r);
            assert_eq!(got.len(), 4);
            assert_eq!(got[0], ("a.txt".into(), b"bonjour".to_vec()));
            assert_eq!(got[1].1, noise);
            assert_eq!(got[2].1, big);
            assert!(got[3].1.is_empty());
        }
        let packed = build_cabinet(&folders[1..], 0, 0, Reserve::default());
        assert!(packed.len() < big.len() / 4, "MSZIP doit compresser");
    }
}
