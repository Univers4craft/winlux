//! Format ZIP (PKWARE APPNOTE) : lecture (stocké, DEFLATE, ZIP64, noms OEM
//! Windows, UTF-8 ou champ Unicode Path, chiffrement ZipCrypto) et écriture, écrits de zéro.

use super::crc32::{crc32_byte, Crc32};
use super::deflate::Deflater;
use super::inflate::Inflater;
use super::util::*;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Nom d'une entrée sans l'indicateur UTF-8 : un ZIP créé sous Windows
/// (Explorateur, anciens WinZip/WinRAR) l'écrit dans la page OEM de sa
/// langue (850 en Europe de l'Ouest), un ZIP Unix en UTF-8 le plus souvent.
fn legacy_name(raw: &[u8], made_by_host: u16) -> String {
    let utf8 = std::str::from_utf8(raw).ok();
    if raw.iter().all(|&b| b < 128) {
        return utf8.unwrap_or_default().to_string();
    }
    // Hôte 0 = MS-DOS/FAT, 10/11 = NTFS : page OEM même si les octets
    // ressemblent à de l'UTF-8.
    let windows = matches!(made_by_host, 0 | 10 | 11 | 14);
    match utf8 {
        Some(s) if !windows => s.to_string(),
        _ => {
            let oem = crate::intl::codepage::resolve(crate::intl::codepage::CP_OEMCP);
            crate::intl::codepage::decode(oem, raw, false).unwrap_or_default()
        }
    }
}

/// Champ « Info-ZIP Unicode Path » (0x7075) : nom UTF-8 ajouté par 7-Zip,
/// WinRAR et Info-ZIP, valable si son CRC correspond au nom d'origine.
fn unicode_path_extra(extra: &[u8], raw: &[u8]) -> Option<String> {
    let mut x = 0;
    while x + 4 <= extra.len() {
        let id = u16le(extra, x);
        let sz = u16le(extra, x + 2) as usize;
        if x + 4 + sz > extra.len() {
            break;
        }
        if id == 0x7075 && sz >= 5 && extra[x + 4] == 1 {
            let crc = u32le(extra, x + 5);
            let mut c = Crc32::new();
            c.update(raw);
            if c.value() == crc {
                return std::str::from_utf8(&extra[x + 9..x + 4 + sz])
                    .ok()
                    .map(str::to_string);
            }
        }
        x += 4 + sz;
    }
    None
}

#[derive(Clone, Debug)]
pub struct ZipEntry {
    pub name: String,
    pub flags: u16,
    pub method: u16,
    pub dos_time: u32,
    pub crc: u32,
    pub csize: u64,
    pub usize: u64,
    pub offset: u64,
    pub unix_mode: Option<u32>,
}

impl ZipEntry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
            || self.name.ends_with('\\')
            || self.unix_mode.is_some_and(|m| m & 0o170000 == 0o040000)
    }
    pub fn encrypted(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn is_symlink(&self) -> bool {
        self.unix_mode.is_some_and(|m| m & 0o170000 == 0o120000)
    }
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64le(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

fn bad(msg: &str) -> ArcError {
    ArcError::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("zip : {msg}"),
    ))
}

/// Lit le répertoire central.
pub fn list(f: &mut File) -> Res<Vec<ZipEntry>> {
    let len = f.seek(SeekFrom::End(0))?;
    let tail = len.min(65_557);
    f.seek(SeekFrom::Start(len - tail))?;
    let mut buf = vec![0u8; tail as usize];
    f.read_exact(&mut buf)?;
    let eocd = (0..buf.len().saturating_sub(21))
        .rev()
        .find(|&i| u32le(&buf, i) == 0x0605_4b50)
        .ok_or_else(|| bad("fin du répertoire central introuvable"))?;
    let mut count = u16le(&buf, eocd + 10) as u64;
    let mut cd_size = u32le(&buf, eocd + 12) as u64;
    let mut cd_off = u32le(&buf, eocd + 16) as u64;
    let base = len - tail;
    if eocd >= 20 && u32le(&buf, eocd - 20) == 0x0706_4b50 {
        let off64 = u64le(&buf, eocd - 12);
        f.seek(SeekFrom::Start(off64))?;
        let mut e = [0u8; 56];
        f.read_exact(&mut e)?;
        if u32le(&e, 0) == 0x0606_4b50 {
            count = u64le(&e, 32);
            cd_size = u64le(&e, 40);
            cd_off = u64le(&e, 48);
        }
    }
    // Archives avec données préfixées (auto-extractibles) : décalage réel.
    let cd_end_expected = cd_off + cd_size;
    let shift = (base + eocd as u64).saturating_sub(cd_end_expected);
    let shift = if u32le(&buf, eocd.saturating_sub(20)) == 0x0706_4b50 {
        0
    } else {
        shift
    };
    f.seek(SeekFrom::Start(cd_off + shift))?;
    let mut cd = vec![0u8; cd_size as usize];
    f.read_exact(&mut cd)?;
    let mut out = Vec::with_capacity(count as usize);
    let mut p = 0usize;
    while p + 46 <= cd.len() && u32le(&cd, p) == 0x0201_4b50 {
        let made_by = u16le(&cd, p + 4);
        let flags = u16le(&cd, p + 8);
        let method = u16le(&cd, p + 10);
        let dos_time = u32le(&cd, p + 12);
        let crc = u32le(&cd, p + 16);
        let mut csize = u32le(&cd, p + 20) as u64;
        let mut usize_ = u32le(&cd, p + 24) as u64;
        let nlen = u16le(&cd, p + 28) as usize;
        let xlen = u16le(&cd, p + 30) as usize;
        let clen = u16le(&cd, p + 32) as usize;
        let eattr = u32le(&cd, p + 38);
        let mut offset = u32le(&cd, p + 42) as u64;
        if p + 46 + nlen + xlen > cd.len() {
            return Err(bad("répertoire central tronqué"));
        }
        let raw = &cd[p + 46..p + 46 + nlen];
        let extra = &cd[p + 46 + nlen..p + 46 + nlen + xlen];
        let name = if flags & 0x800 != 0 {
            String::from_utf8_lossy(raw).into_owned()
        } else {
            unicode_path_extra(extra, raw).unwrap_or_else(|| legacy_name(raw, made_by >> 8))
        };
        let mut x = 0;
        while x + 4 <= extra.len() {
            let id = u16le(extra, x);
            let sz = u16le(extra, x + 2) as usize;
            if x + 4 + sz > extra.len() {
                break;
            }
            if id == 1 {
                let mut q = x + 4;
                for field in [&mut usize_, &mut csize, &mut offset] {
                    if *field == 0xFFFF_FFFF && q + 8 <= x + 4 + sz {
                        *field = u64le(extra, q);
                        q += 8;
                    }
                }
            }
            x += 4 + sz;
        }
        let unix_mode = if made_by >> 8 == 3 && eattr >> 16 != 0 {
            Some(eattr >> 16)
        } else if eattr & 0x10 != 0 {
            Some(0o040755)
        } else {
            None
        };
        out.push(ZipEntry {
            name,
            flags,
            method,
            dos_time,
            crc,
            csize,
            usize: usize_,
            offset: offset + shift,
            unix_mode,
        });
        p += 46 + nlen + xlen + clen;
    }
    Ok(out)
}

struct ZipCrypto {
    k: [u32; 3],
}

impl ZipCrypto {
    fn new(password: &[u8]) -> Self {
        let mut z = ZipCrypto {
            k: [0x1234_5678, 0x2345_6789, 0x3456_7890],
        };
        for &c in password {
            z.update(c);
        }
        z
    }
    fn update(&mut self, c: u8) {
        self.k[0] = crc32_byte(self.k[0], c);
        self.k[1] = self.k[1]
            .wrapping_add(self.k[0] & 0xFF)
            .wrapping_mul(134_775_813)
            .wrapping_add(1);
        self.k[2] = crc32_byte(self.k[2], (self.k[1] >> 24) as u8);
    }
    fn stream(&self) -> u8 {
        let t = (self.k[2] | 2) as u16;
        (t.wrapping_mul(t ^ 1) >> 8) as u8
    }
    fn decrypt(&mut self, buf: &mut [u8]) {
        for b in buf {
            let p = *b ^ self.stream();
            self.update(p);
            *b = p;
        }
    }
    fn encrypt(&mut self, buf: &mut [u8]) {
        for b in buf {
            let p = *b;
            *b = p ^ self.stream();
            self.update(p);
        }
    }
}

struct Decrypt<R> {
    inner: R,
    z: ZipCrypto,
}
impl<R: Read> Read for Decrypt<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.z.decrypt(&mut buf[..n]);
        Ok(n)
    }
}

/// Ouvre le flux décompressé d'une entrée.
pub fn open_entry<'a>(
    f: &'a mut File,
    e: &ZipEntry,
    password: Option<&str>,
) -> Res<Box<dyn Read + 'a>> {
    f.seek(SeekFrom::Start(e.offset))?;
    let mut h = [0u8; 30];
    f.read_exact(&mut h)?;
    if u32le(&h, 0) != 0x0403_4b50 {
        return Err(bad("en-tête local invalide"));
    }
    let skip = u16le(&h, 26) as i64 + u16le(&h, 28) as i64;
    f.seek(SeekFrom::Current(skip))?;
    let mut raw: Box<dyn Read + 'a> = Box::new(f.take(e.csize));
    if e.encrypted() {
        if e.method == 99 || e.flags & 0x40 != 0 {
            return Err(ArcError::Unsupported("chiffrement AES".into()));
        }
        let pw = password.ok_or(ArcError::NeedPassword)?;
        let mut z = ZipCrypto::new(pw.as_bytes());
        let mut hdr = [0u8; 12];
        raw.read_exact(&mut hdr)?;
        z.decrypt(&mut hdr);
        let check = if e.flags & 8 != 0 {
            (e.dos_time >> 8) as u8
        } else {
            (e.crc >> 24) as u8
        };
        if hdr[11] != check {
            return Err(ArcError::BadPassword);
        }
        raw = Box::new(Decrypt { inner: raw, z });
    }
    match e.method {
        0 => Ok(raw),
        8 => Ok(Box::new(Inflater::new(BufReader::with_capacity(
            64 * 1024,
            raw,
        )))),
        m => Err(ArcError::Unsupported(format!("méthode de compression {m}"))),
    }
}

/// Extrait toute l'archive dans `dest`.
pub fn extract(path: &Path, dest: &Path, password: Option<&str>, progress: &Progress) -> Res<()> {
    let mut f = File::open(path)?;
    let entries = list(&mut f)?;
    progress.set_total(entries.iter().map(|e| e.usize).sum::<u64>().max(1));
    std::fs::create_dir_all(dest)?;
    let mut links = Vec::new();
    for e in &entries {
        let Some(rel) = safe_relative(&e.name) else {
            continue;
        };
        if !no_symlink_parent(dest, &rel) {
            continue;
        }
        let out = dest.join(&rel);
        if e.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut src = open_entry(&mut f, e, password)?;
        if e.is_symlink() {
            let mut t = String::new();
            src.read_to_string(&mut t)?;
            links.push((rel, t));
            continue;
        }
        let _ = std::fs::remove_file(&out);
        let mut w = BufWriter::new(File::create(&out)?);
        let mut crc = Crc32::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            crc.update(&buf[..n]);
            w.write_all(&buf[..n])?;
            progress.add(n as u64)?;
        }
        w.flush()?;
        drop(w);
        if crc.value() != e.crc {
            return Err(if e.encrypted() {
                ArcError::BadPassword
            } else {
                bad(&format!("CRC incorrect pour {}", e.name))
            });
        }
        if let Some(m) = e.unix_mode {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &out,
                std::fs::Permissions::from_mode((m & 0o777) | 0o600),
            );
        }
        set_mtime(&out, dos_to_unix(e.dos_time));
    }
    for (rel, target) in links {
        if symlink_stays_inside(&rel, &target) {
            let _ = std::os::unix::fs::symlink(&target, dest.join(&rel));
        }
    }
    Ok(())
}

/// L'archive contient-elle des entrées chiffrées ?
#[cfg(test)]
pub fn is_encrypted(path: &Path) -> Res<bool> {
    let mut f = File::open(path)?;
    Ok(list(&mut f)?.iter().any(|e| e.encrypted()))
}

/// L'archive utilise-t-elle une méthode non gérée en interne (AES, LZMA…) ?
pub fn needs_external(path: &Path) -> Res<bool> {
    let mut f = File::open(path)?;
    Ok(list(&mut f)?
        .iter()
        .any(|e| !matches!(e.method, 0 | 8) || (e.encrypted() && e.flags & 0x40 != 0)))
}

fn local_tm(secs: i64) -> libc::tm {
    let t = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

pub fn unix_to_dos(secs: i64) -> u32 {
    let tm = local_tm(secs);
    let year = (tm.tm_year + 1900).clamp(1980, 2107) as u32;
    let date = ((year - 1980) << 9) | (((tm.tm_mon + 1) as u32) << 5) | tm.tm_mday as u32;
    let time = ((tm.tm_hour as u32) << 11) | ((tm.tm_min as u32) << 5) | (tm.tm_sec as u32 / 2);
    (date << 16) | time
}

pub fn dos_to_unix(dos: u32) -> i64 {
    let date = dos >> 16;
    let time = dos & 0xFFFF;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = ((date >> 9) + 80) as i32;
    tm.tm_mon = ((date >> 5) & 15) as i32 - 1;
    tm.tm_mday = (date & 31) as i32;
    tm.tm_hour = (time >> 11) as i32;
    tm.tm_min = ((time >> 5) & 63) as i32;
    tm.tm_sec = ((time & 31) * 2) as i32;
    tm.tm_isdst = -1;
    if tm.tm_mday == 0 {
        return 0;
    }
    unsafe { libc::mktime(&mut tm) as i64 }
}

fn random_bytes(buf: &mut [u8]) {
    let mut filled = 0;
    while filled < buf.len() {
        let r = unsafe {
            libc::getrandom(
                buf[filled..].as_mut_ptr() as *mut libc::c_void,
                buf.len() - filled,
                0,
            )
        };
        if r <= 0 {
            // Repli très improbable : horloge.
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(1);
            for (i, b) in buf[filled..].iter_mut().enumerate() {
                *b = (t >> ((i % 16) * 8)) as u8 ^ i as u8;
            }
            return;
        }
        filled += r as usize;
    }
}

struct CentralRecord {
    name: Vec<u8>,
    flags: u16,
    method: u16,
    dos_time: u32,
    crc: u32,
    csize: u64,
    usize: u64,
    offset: u64,
    mode: u32,
}

/// Écrivain qui compte et chiffre éventuellement.
struct Sink<'a, W: Write> {
    w: &'a mut W,
    n: u64,
    z: Option<ZipCrypto>,
    tmp: Vec<u8>,
}
impl<W: Write> Write for Sink<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(z) = self.z.as_mut() {
            self.tmp.clear();
            self.tmp.extend_from_slice(buf);
            z.encrypt(&mut self.tmp);
            self.w.write_all(&self.tmp)?;
        } else {
            self.w.write_all(buf)?;
        }
        self.n += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

/// Crée une archive ZIP à partir du contenu de dossiers et fichiers.
/// `items` : (nom dans l'archive, chemin sur disque).
pub fn create(
    out: &Path,
    items: &[(String, std::path::PathBuf)],
    password: Option<&str>,
    progress: &Progress,
) -> Res<()> {
    let mut w = BufWriter::with_capacity(256 * 1024, File::create(out)?);
    let mut central = Vec::new();
    let total: u64 = items
        .iter()
        .filter_map(|(_, p)| std::fs::symlink_metadata(p).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    progress.set_total(total.max(1));
    for (name, path) in items {
        let meta = std::fs::symlink_metadata(path)?;
        use std::os::unix::fs::PermissionsExt;
        let dos_time = unix_to_dos(mtime_secs(&meta));
        let offset = w.stream_position()?;
        let is_link = meta.file_type().is_symlink();
        if meta.is_dir() {
            let mut n = name.clone().into_bytes();
            if !n.ends_with(b"/") {
                n.push(b'/');
            }
            write_local(&mut w, &n, 0x800, 0, dos_time, 0, 0, 0, false)?;
            central.push(CentralRecord {
                name: n,
                flags: 0x800,
                method: 0,
                dos_time,
                crc: 0,
                csize: 0,
                usize: 0,
                offset,
                mode: 0o040000 | (meta.permissions().mode() & 0o777),
            });
            continue;
        }
        let data_link;
        let size = if is_link {
            data_link = std::fs::read_link(path)?
                .to_string_lossy()
                .into_owned()
                .into_bytes();
            data_link.len() as u64
        } else {
            data_link = Vec::new();
            meta.len()
        };
        let open = || -> io::Result<Box<dyn Read>> {
            if is_link {
                Ok(Box::new(io::Cursor::new(data_link.clone())))
            } else {
                Ok(Box::new(BufReader::with_capacity(
                    256 * 1024,
                    File::open(path)?,
                )))
            }
        };
        // Le CRC sert d'octet de contrôle du chiffrement : calculé d'abord.
        let mut crc = Crc32::new();
        if password.is_some() {
            let mut r = open()?;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                crc.update(&buf[..n]);
            }
        }
        let zip64 = size >= 0xFFFF_0000;
        let mut flags = 0x800u16;
        if password.is_some() {
            flags |= 1;
        }
        let mut method = if size == 0 { 0 } else { 8 };
        let n = name.as_bytes();
        let mut attempt = 0;
        let (csize, final_crc) = loop {
            w.seek(SeekFrom::Start(offset))?;
            write_local(&mut w, n, flags, method, dos_time, 0, 0, 0, zip64)?;
            let mut sink = Sink {
                w: &mut w,
                n: 0,
                z: password.map(|p| ZipCrypto::new(p.as_bytes())),
                tmp: Vec::new(),
            };
            if let Some(_pw) = password {
                let mut hdr = [0u8; 12];
                random_bytes(&mut hdr[..11]);
                hdr[11] = (crc.value() >> 24) as u8;
                sink.write_all(&hdr)?;
            }
            let mut c2 = Crc32::new();
            let mut r = open()?;
            let mut buf = vec![0u8; 64 * 1024];
            if method == 8 {
                let mut d = Deflater::new(&mut sink);
                loop {
                    let k = r.read(&mut buf)?;
                    if k == 0 {
                        break;
                    }
                    c2.update(&buf[..k]);
                    d.write_all(&buf[..k])?;
                    if attempt == 0 {
                        progress.add(k as u64)?;
                    }
                }
                d.finish()?;
            } else {
                loop {
                    let k = r.read(&mut buf)?;
                    if k == 0 {
                        break;
                    }
                    c2.update(&buf[..k]);
                    sink.write_all(&buf[..k])?;
                    if attempt == 0 {
                        progress.add(k as u64)?;
                    }
                }
            }
            let written = sink.n;
            let payload = written - if password.is_some() { 12 } else { 0 };
            if method == 8 && payload > size && attempt == 0 {
                // Incompressible : on réécrit l'entrée sans compression.
                method = 0;
                attempt = 1;
                continue;
            }
            break (written, c2.value());
        };
        let end = w.stream_position()?;
        w.seek(SeekFrom::Start(offset))?;
        write_local(
            &mut w, n, flags, method, dos_time, final_crc, csize, size, zip64,
        )?;
        w.seek(SeekFrom::Start(end))?;
        let mode = if is_link {
            0o120777
        } else {
            0o100000 | (meta.permissions().mode() & 0o777)
        };
        central.push(CentralRecord {
            name: n.to_vec(),
            flags,
            method,
            dos_time,
            crc: final_crc,
            csize,
            usize: size,
            offset,
            mode,
        });
    }
    // Les réécritures d'entrées incompressibles peuvent laisser un excédent.
    let cd_off = w.stream_position()?;
    for c in &central {
        let big = c.csize >= 0xFFFF_FFFF || c.usize >= 0xFFFF_0000 || c.offset >= 0xFFFF_FFFF;
        let mut extra = Vec::new();
        if big {
            extra.extend_from_slice(&1u16.to_le_bytes());
            extra.extend_from_slice(&24u16.to_le_bytes());
            extra.extend_from_slice(&c.usize.to_le_bytes());
            extra.extend_from_slice(&c.csize.to_le_bytes());
            extra.extend_from_slice(&c.offset.to_le_bytes());
        }
        let clamp = |v: u64| if big { 0xFFFF_FFFFu32 } else { v as u32 };
        w.write_all(&0x0201_4b50u32.to_le_bytes())?;
        w.write_all(&((3u16 << 8) | 45).to_le_bytes())?;
        w.write_all(&(if big { 45u16 } else { 20 }).to_le_bytes())?;
        w.write_all(&c.flags.to_le_bytes())?;
        w.write_all(&c.method.to_le_bytes())?;
        w.write_all(&c.dos_time.to_le_bytes())?;
        w.write_all(&c.crc.to_le_bytes())?;
        w.write_all(&clamp(c.csize).to_le_bytes())?;
        w.write_all(&clamp(c.usize).to_le_bytes())?;
        w.write_all(&(c.name.len() as u16).to_le_bytes())?;
        w.write_all(&(extra.len() as u16).to_le_bytes())?;
        w.write_all(&[0u8; 6])?; // commentaire, disque, attributs internes
        let dos_attr = if c.mode & 0o170000 == 0o040000 {
            0x10
        } else {
            0
        };
        w.write_all(&((c.mode << 16) | dos_attr).to_le_bytes())?;
        w.write_all(&clamp(c.offset).to_le_bytes())?;
        w.write_all(&c.name)?;
        w.write_all(&extra)?;
    }
    let cd_end = w.stream_position()?;
    let cd_size = cd_end - cd_off;
    let n = central.len() as u64;
    if n >= 0xFFFF || cd_off >= 0xFFFF_FFFF || cd_size >= 0xFFFF_FFFF {
        w.write_all(&0x0606_4b50u32.to_le_bytes())?;
        w.write_all(&44u64.to_le_bytes())?;
        w.write_all(&((3u16 << 8) | 45).to_le_bytes())?;
        w.write_all(&45u16.to_le_bytes())?;
        w.write_all(&[0u8; 8])?;
        w.write_all(&n.to_le_bytes())?;
        w.write_all(&n.to_le_bytes())?;
        w.write_all(&cd_size.to_le_bytes())?;
        w.write_all(&cd_off.to_le_bytes())?;
        w.write_all(&0x0706_4b50u32.to_le_bytes())?;
        w.write_all(&0u32.to_le_bytes())?;
        w.write_all(&cd_end.to_le_bytes())?;
        w.write_all(&1u32.to_le_bytes())?;
    }
    w.write_all(&0x0605_4b50u32.to_le_bytes())?;
    w.write_all(&[0u8; 4])?;
    let n16 = n.min(0xFFFF) as u16;
    w.write_all(&n16.to_le_bytes())?;
    w.write_all(&n16.to_le_bytes())?;
    w.write_all(&(cd_size.min(0xFFFF_FFFF) as u32).to_le_bytes())?;
    w.write_all(&(cd_off.min(0xFFFF_FFFF) as u32).to_le_bytes())?;
    w.write_all(&0u16.to_le_bytes())?;
    w.flush()?;
    let f = w.into_inner().map_err(|e| e.into_error())?;
    let end = cd_end
        + if n >= 0xFFFF || cd_off >= 0xFFFF_FFFF || cd_size >= 0xFFFF_FFFF {
            76
        } else {
            0
        }
        + 22;
    f.set_len(end)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_local<W: Write>(
    w: &mut W,
    name: &[u8],
    flags: u16,
    method: u16,
    dos_time: u32,
    crc: u32,
    csize: u64,
    usize: u64,
    zip64: bool,
) -> io::Result<()> {
    w.write_all(&0x0403_4b50u32.to_le_bytes())?;
    w.write_all(&(if zip64 { 45u16 } else { 20 }).to_le_bytes())?;
    w.write_all(&flags.to_le_bytes())?;
    w.write_all(&method.to_le_bytes())?;
    w.write_all(&dos_time.to_le_bytes())?;
    w.write_all(&crc.to_le_bytes())?;
    if zip64 {
        w.write_all(&0xFFFF_FFFFu32.to_le_bytes())?;
        w.write_all(&0xFFFF_FFFFu32.to_le_bytes())?;
    } else {
        w.write_all(&(csize as u32).to_le_bytes())?;
        w.write_all(&(usize as u32).to_le_bytes())?;
    }
    w.write_all(&(name.len() as u16).to_le_bytes())?;
    w.write_all(&(if zip64 { 20u16 } else { 0 }).to_le_bytes())?;
    w.write_all(name)?;
    if zip64 {
        w.write_all(&1u16.to_le_bytes())?;
        w.write_all(&16u16.to_le_bytes())?;
        w.write_all(&usize.to_le_bytes())?;
        w.write_all(&csize.to_le_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("winlux-zip-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample_tree(src: &Path) -> Vec<(String, std::path::PathBuf)> {
        std::fs::create_dir_all(src.join("dossier/sous")).unwrap();
        std::fs::write(src.join("lisezmoi.txt"), "Bonjour Winlux\n".repeat(500)).unwrap();
        std::fs::write(
            src.join("dossier/sous/données.bin"),
            (0..70_000u32)
                .map(|i| (i * 7919 % 256) as u8)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        std::fs::write(src.join("vide.txt"), "").unwrap();
        let mut items = Vec::new();
        for e in walk(src).unwrap() {
            items.push((archive_name(&e.rel), src.join(&e.rel)));
        }
        items
    }

    #[test]
    fn noms_de_zip_crees_sous_windows() {
        // « Été.txt » en page OEM 850 (Explorateur Windows français).
        let raw = b"\x90t\x82.txt";
        if crate::intl::codepage::resolve(crate::intl::codepage::CP_OEMCP) == 850 {
            assert_eq!(legacy_name(raw, 0), "Été.txt");
        }
        // Zip Unix sans indicateur : UTF-8.
        assert_eq!(legacy_name("Été.txt".as_bytes(), 3), "Été.txt");
        assert_eq!(legacy_name(b"plain.txt", 0), "plain.txt");
        // Champ Unicode Path (7-Zip) : prioritaire si le CRC correspond.
        let utf8 = "Ελληνικά.txt".as_bytes();
        let mut c = Crc32::new();
        c.update(raw);
        let mut extra = vec![0x75, 0x70];
        extra.extend_from_slice(&((5 + utf8.len()) as u16).to_le_bytes());
        extra.push(1);
        extra.extend_from_slice(&c.value().to_le_bytes());
        extra.extend_from_slice(utf8);
        assert_eq!(unicode_path_extra(&extra, raw).as_deref(), Some("Ελληνικά.txt"));
        assert_eq!(unicode_path_extra(&extra, b"autre"), None);
    }

    #[test]
    fn create_then_extract() {
        let d = tmp("rt");
        let src = d.join("src");
        let items = sample_tree(&src);
        let z = d.join("a.zip");
        create(&z, &items, None, &Progress::default()).unwrap();
        let out = d.join("out");
        extract(&z, &out, None, &Progress::default()).unwrap();
        for (_, p) in &items {
            let rel = p.strip_prefix(&src).unwrap();
            if p.is_file() {
                assert_eq!(
                    std::fs::read(p).unwrap(),
                    std::fs::read(out.join(rel)).unwrap()
                );
            } else {
                assert!(out.join(rel).is_dir());
            }
        }
        // Compatibilité avec un lecteur tiers (Python zipfile), s'il est présent.
        if let Ok(o) = std::process::Command::new("python3")
            .arg("-c")
            .arg("import sys,zipfile;f=zipfile.ZipFile(sys.argv[1]);assert f.testzip() is None")
            .arg(&z)
            .output()
        {
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        }
        if let Ok(o) = std::process::Command::new("unzip")
            .arg("-t")
            .arg(&z)
            .output()
        {
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn password_protected() {
        let d = tmp("pw");
        let src = d.join("src");
        let items = sample_tree(&src);
        let z = d.join("secret.zip");
        create(&z, &items, Some("motdepasse"), &Progress::default()).unwrap();
        assert!(is_encrypted(&z).unwrap());
        let out = d.join("out");
        assert!(matches!(
            extract(&z, &out, None, &Progress::default()),
            Err(ArcError::NeedPassword)
        ));
        assert!(matches!(
            extract(&z, &out, Some("faux"), &Progress::default()),
            Err(ArcError::BadPassword)
        ));
        extract(&z, &out, Some("motdepasse"), &Progress::default()).unwrap();
        if let Ok(o) = std::process::Command::new("python3")
            .arg("-c")
            .arg("import sys,zipfile;f=zipfile.ZipFile(sys.argv[1]);f.setpassword(b'motdepasse');assert f.testzip() is None")
            .arg(&z)
            .output()
        {
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        }
        assert_eq!(
            std::fs::read(out.join("lisezmoi.txt")).unwrap(),
            std::fs::read(src.join("lisezmoi.txt")).unwrap()
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn python_zip_readable() {
        // Archive produite par Python zipfile (DEFLATE), si Python est là.
        let d = tmp("py");
        let z = d.join("py.zip");
        let script = format!(
            "import zipfile\nwith zipfile.ZipFile({:?},'w',zipfile.ZIP_DEFLATED) as f:\n f.writestr('a/é.txt','x'*10000)\n f.writestr('b.txt','hello')\n",
            z.to_string_lossy()
        );
        let Ok(o) = std::process::Command::new("python3")
            .arg("-c")
            .arg(&script)
            .output()
        else {
            return;
        };
        if !o.status.success() {
            return;
        }
        let out = d.join("out");
        extract(&z, &out, None, &Progress::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(out.join("a/é.txt")).unwrap(),
            "x".repeat(10000)
        );
        assert_eq!(std::fs::read_to_string(out.join("b.txt")).unwrap(), "hello");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dos_time_roundtrip() {
        let t = 1_761_900_000i64; // 2025
        let back = dos_to_unix(unix_to_dos(t));
        assert!((back - t).abs() <= 2);
    }
}
