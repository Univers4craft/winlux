//! Format TAR (ustar, extensions GNU « L/K » et pax) : lecture et écriture.

use super::util::*;
use std::io::{self, Read, Write};
use std::path::Path;

fn bad(msg: &str) -> ArcError {
    ArcError::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("tar : {msg}"),
    ))
}

fn octal(field: &[u8]) -> u64 {
    if field.first().is_some_and(|&b| b & 0x80 != 0) {
        // Codage base 256 (GNU) pour les grandes tailles.
        let mut v = (field[0] & 0x7F) as u64;
        for &b in &field[1..] {
            v = (v << 8) | b as u64;
        }
        return v;
    }
    let mut v = 0u64;
    for &b in field {
        match b {
            b'0'..=b'7' => v = (v << 3) | (b - b'0') as u64,
            b' ' | 0 if v > 0 => break,
            _ => {}
        }
    }
    v
}

fn cstr(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut got = 0;
    while got < buf.len() {
        let n = r.read(&mut buf[got..])?;
        if n == 0 {
            if got == 0 {
                return Ok(false);
            }
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "tar tronqué"));
        }
        got += n;
    }
    Ok(true)
}

fn skip<R: Read>(r: &mut R, mut n: u64) -> io::Result<()> {
    let mut buf = [0u8; 8192];
    while n > 0 {
        let k = n.min(buf.len() as u64) as usize;
        r.read_exact(&mut buf[..k])?;
        n -= k as u64;
    }
    Ok(())
}

fn padding(size: u64) -> u64 {
    (512 - size % 512) % 512
}

fn read_payload<R: Read>(r: &mut R, size: u64) -> io::Result<Vec<u8>> {
    let mut v = vec![0u8; size.min(16 << 20) as usize];
    r.read_exact(&mut v)?;
    skip(r, size - v.len() as u64 + padding(size))?;
    Ok(v)
}

/// Extrait un flux TAR dans `dest`. `progress` compte les octets de fichiers.
pub fn extract<R: Read>(mut r: R, dest: &Path, progress: &Progress) -> Res<()> {
    std::fs::create_dir_all(dest)?;
    let mut long_name: Option<String> = None;
    let mut long_link: Option<String> = None;
    let mut pax_size: Option<u64> = None;
    let mut links = Vec::new();
    let mut hdr = [0u8; 512];
    loop {
        if !read_full(&mut r, &mut hdr)? {
            break;
        }
        if hdr.iter().all(|&b| b == 0) {
            break;
        }
        let sum: u64 = hdr
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    b as u64
                }
            })
            .sum();
        if octal(&hdr[148..156]) != sum {
            return Err(bad("somme de contrôle d'en-tête incorrecte"));
        }
        let kind = hdr[156];
        let mut size = octal(&hdr[124..136]);
        if let Some(s) = pax_size.take() {
            size = s;
        }
        match kind {
            b'L' => {
                long_name = Some(cstr(&read_payload(&mut r, size)?));
                continue;
            }
            b'K' => {
                long_link = Some(cstr(&read_payload(&mut r, size)?));
                continue;
            }
            b'x' => {
                let data = read_payload(&mut r, size)?;
                let text = String::from_utf8_lossy(&data);
                for rec in text.lines() {
                    let kv = rec.split_once(' ').map(|x| x.1).unwrap_or("");
                    if let Some(v) = kv.strip_prefix("path=") {
                        long_name = Some(v.to_string());
                    } else if let Some(v) = kv.strip_prefix("linkpath=") {
                        long_link = Some(v.to_string());
                    } else if let Some(v) = kv.strip_prefix("size=") {
                        pax_size = v.trim().parse().ok();
                    }
                }
                continue;
            }
            b'g' => {
                skip(&mut r, size + padding(size))?;
                continue;
            }
            _ => {}
        }
        let name = long_name.take().unwrap_or_else(|| {
            let n = cstr(&hdr[0..100]);
            let prefix = if &hdr[257..262] == b"ustar" {
                cstr(&hdr[345..500])
            } else {
                String::new()
            };
            if prefix.is_empty() {
                n
            } else {
                format!("{prefix}/{n}")
            }
        });
        let link = long_link.take().unwrap_or_else(|| cstr(&hdr[157..257]));
        let mode = octal(&hdr[100..108]) as u32;
        let mtime = octal(&hdr[136..148]) as i64;
        let rel = safe_relative(&name);
        let usable = rel.as_ref().is_some_and(|r| no_symlink_parent(dest, r));
        match kind {
            b'5' => {
                if let (true, Some(rel)) = (usable, &rel) {
                    std::fs::create_dir_all(dest.join(rel))?;
                }
                skip(&mut r, size + padding(size))?;
            }
            b'2' => {
                if let (true, Some(rel)) = (usable, rel) {
                    links.push((rel, link));
                }
                skip(&mut r, size + padding(size))?;
            }
            b'0' | 0 | b'7' | b'1' => {
                let (true, Some(rel)) = (usable, rel) else {
                    skip(&mut r, size + padding(size))?;
                    continue;
                };
                let out = dest.join(&rel);
                if let Some(p) = out.parent() {
                    std::fs::create_dir_all(p)?;
                }
                if kind == b'1' {
                    // Lien physique : copie de la cible déjà extraite.
                    if let Some(t) = safe_relative(&link) {
                        let _ = std::fs::copy(dest.join(t), &out);
                    }
                    skip(&mut r, size + padding(size))?;
                    continue;
                }
                let _ = std::fs::remove_file(&out);
                let mut w = io::BufWriter::new(std::fs::File::create(&out)?);
                let mut left = size;
                let mut buf = vec![0u8; 64 * 1024];
                while left > 0 {
                    let k = left.min(buf.len() as u64) as usize;
                    r.read_exact(&mut buf[..k])?;
                    w.write_all(&buf[..k])?;
                    left -= k as u64;
                    progress.add(k as u64)?;
                }
                w.flush()?;
                drop(w);
                skip(&mut r, padding(size))?;
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &out,
                    std::fs::Permissions::from_mode((mode & 0o777) | 0o600),
                );
                set_mtime(&out, mtime);
            }
            _ => skip(&mut r, size + padding(size))?,
        }
    }
    for (rel, target) in links {
        if symlink_stays_inside(&rel, &target) {
            let p = dest.join(&rel);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            let _ = std::os::unix::fs::symlink(&target, p);
        }
    }
    Ok(())
}

fn put_octal(field: &mut [u8], v: u64) {
    let width = field.len() - 1;
    if v >= 1u64 << (3 * width as u32).min(63) {
        field.fill(0);
        field[0] = 0x80;
        let bytes = v.to_be_bytes();
        let n = field.len();
        field[n - 8..].copy_from_slice(&bytes);
        return;
    }
    let s = format!("{v:0width$o}");
    field[..width].copy_from_slice(s.as_bytes());
    field[width] = 0;
}

fn header(name: &[u8], kind: u8, size: u64, mode: u32, mtime: i64, link: &[u8]) -> [u8; 512] {
    let mut h = [0u8; 512];
    h[..name.len().min(100)].copy_from_slice(&name[..name.len().min(100)]);
    put_octal(&mut h[100..108], mode as u64 & 0o7777);
    put_octal(&mut h[108..116], 0);
    put_octal(&mut h[116..124], 0);
    put_octal(&mut h[124..136], size);
    put_octal(&mut h[136..148], mtime.max(0) as u64);
    h[156] = kind;
    h[157..157 + link.len().min(100)].copy_from_slice(&link[..link.len().min(100)]);
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[265..269].copy_from_slice(b"root");
    h[297..301].copy_from_slice(b"root");
    h[148..156].fill(b' ');
    let sum: u64 = h.iter().map(|&b| b as u64).sum();
    let s = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(s.as_bytes());
    h
}

fn write_long<W: Write>(w: &mut W, kind: u8, data: &[u8]) -> io::Result<()> {
    let mut d = data.to_vec();
    d.push(0);
    w.write_all(&header(
        b"././@LongLink",
        kind,
        d.len() as u64,
        0o644,
        0,
        b"",
    ))?;
    w.write_all(&d)?;
    w.write_all(&vec![0u8; padding(d.len() as u64) as usize])
}

/// Écrit un flux TAR à partir de `items` (nom dans l'archive, chemin disque).
pub fn create<W: Write>(
    mut w: W,
    items: &[(String, std::path::PathBuf)],
    progress: &Progress,
) -> Res<W> {
    use std::os::unix::fs::PermissionsExt;
    let total: u64 = items
        .iter()
        .filter_map(|(_, p)| std::fs::symlink_metadata(p).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    progress.set_total(total.max(1));
    let mut written = 0u64;
    for (name, path) in items {
        let meta = std::fs::symlink_metadata(path)?;
        let mode = meta.permissions().mode();
        let mtime = mtime_secs(&meta);
        let mut n = name.clone();
        if meta.is_dir() && !n.ends_with('/') {
            n.push('/');
        }
        let nb = n.as_bytes();
        if nb.len() > 100 {
            write_long(&mut w, b'L', nb)?;
            written += 1024 + padding(nb.len() as u64 + 1) + nb.len() as u64 + 1 - 512;
        }
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(path)?.to_string_lossy().into_owned();
            if target.len() > 100 {
                write_long(&mut w, b'K', target.as_bytes())?;
            }
            w.write_all(&header(nb, b'2', 0, 0o777, mtime, target.as_bytes()))?;
        } else if meta.is_dir() {
            w.write_all(&header(nb, b'5', 0, mode, mtime, b""))?;
        } else if meta.is_file() {
            let size = meta.len();
            w.write_all(&header(nb, b'0', size, mode, mtime, b""))?;
            let mut f = std::fs::File::open(path)?;
            let mut left = size;
            let mut buf = vec![0u8; 64 * 1024];
            while left > 0 {
                let k = left.min(buf.len() as u64) as usize;
                let got = f.read(&mut buf[..k])?;
                if got == 0 {
                    // Fichier raccourci pendant l'archivage : on complète.
                    buf[..k].fill(0);
                    w.write_all(&buf[..k])?;
                    left -= k as u64;
                    continue;
                }
                w.write_all(&buf[..got])?;
                left -= got as u64;
                progress.add(got as u64)?;
            }
            w.write_all(&vec![0u8; padding(size) as usize])?;
        }
        written += 512;
    }
    let _ = written;
    w.write_all(&[0u8; 1024])?;
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_long_names() {
        let d = std::env::temp_dir().join(format!("winlux-tar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let src = d.join("src");
        let long = "n".repeat(150);
        std::fs::create_dir_all(src.join(&long)).unwrap();
        std::fs::write(src.join(&long).join("f.txt"), "contenu").unwrap();
        std::fs::write(src.join("b.bin"), vec![7u8; 1500]).unwrap();
        std::os::unix::fs::symlink("b.bin", src.join("lien")).unwrap();
        let items: Vec<_> = walk(&src)
            .unwrap()
            .into_iter()
            .map(|e| (archive_name(&e.rel), src.join(&e.rel)))
            .collect();
        let data = create(Vec::new(), &items, &Progress::default()).unwrap();
        let out = d.join("out");
        extract(&data[..], &out, &Progress::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(out.join(&long).join("f.txt")).unwrap(),
            "contenu"
        );
        assert_eq!(std::fs::read(out.join("b.bin")).unwrap(), vec![7u8; 1500]);
        assert_eq!(
            std::fs::read_link(out.join("lien")).unwrap(),
            std::path::PathBuf::from("b.bin")
        );
        // GNU tar sait relire notre archive.
        std::fs::write(d.join("a.tar"), &data).unwrap();
        if let Ok(o) = std::process::Command::new("tar")
            .arg("-tf")
            .arg(d.join("a.tar"))
            .output()
        {
            assert!(o.status.success());
            assert!(String::from_utf8_lossy(&o.stdout).contains("f.txt"));
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn traversal_is_skipped() {
        let d = std::env::temp_dir().join(format!("winlux-tar-evil-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut data = Vec::new();
        data.extend_from_slice(&header(b"../evil.txt", b'0', 4, 0o644, 0, b""));
        data.extend_from_slice(b"evil");
        data.extend_from_slice(&[0u8; 508]);
        data.extend_from_slice(&[0u8; 1024]);
        extract(&data[..], &d.join("out"), &Progress::default()).unwrap();
        assert!(!d.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
