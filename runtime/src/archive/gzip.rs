//! Conteneur gzip (RFC 1952) autour de DEFLATE, en flux.

use super::crc32::Crc32;
use super::deflate::Deflater;
use super::inflate::{BitReader, Inflater};
use std::io::{self, BufRead, Read, Write};

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("gzip : {msg}"))
}

/// Lecteur gzip ; gère les fichiers à plusieurs membres concaténés.
pub struct GzDecoder<R: BufRead> {
    inf: Option<Inflater<R>>,
    crc: Crc32,
    size: u32,
}

fn read_header<R: BufRead>(br: &mut BitReader<R>) -> io::Result<()> {
    let mut h = [0u8; 10];
    br.read_bytes(&mut h)?;
    if h[0] != 0x1F || h[1] != 0x8B || h[2] != 8 {
        return Err(bad("en-tête invalide"));
    }
    let flags = h[3];
    if flags & 4 != 0 {
        let mut l = [0u8; 2];
        br.read_bytes(&mut l)?;
        let mut skip = vec![0u8; u16::from_le_bytes(l) as usize];
        br.read_bytes(&mut skip)?;
    }
    for bit in [8u8, 16] {
        if flags & bit != 0 {
            let mut c = [0u8; 1];
            loop {
                br.read_bytes(&mut c)?;
                if c[0] == 0 {
                    break;
                }
            }
        }
    }
    if flags & 2 != 0 {
        let mut c = [0u8; 2];
        br.read_bytes(&mut c)?;
    }
    Ok(())
}

impl<R: BufRead> GzDecoder<R> {
    pub fn new(inner: R) -> io::Result<Self> {
        let mut br = BitReader::new(inner);
        read_header(&mut br)?;
        Ok(GzDecoder {
            inf: Some(Inflater::from_bits(br)),
            crc: Crc32::new(),
            size: 0,
        })
    }
}

impl<R: BufRead> Read for GzDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let Some(inf) = self.inf.as_mut() else {
                return Ok(0);
            };
            let n = inf.read(buf)?;
            if n > 0 {
                self.crc.update(&buf[..n]);
                self.size = self.size.wrapping_add(n as u32);
                return Ok(n);
            }
            // Fin du membre : contrôle CRC et taille, puis membre suivant éventuel.
            let mut br = self.inf.take().unwrap().into_bits();
            let mut t = [0u8; 8];
            br.read_bytes(&mut t)?;
            if u32::from_le_bytes([t[0], t[1], t[2], t[3]]) != self.crc.value() {
                return Err(bad("somme CRC incorrecte"));
            }
            if u32::from_le_bytes([t[4], t[5], t[6], t[7]]) != self.size {
                return Err(bad("taille incorrecte"));
            }
            if br.at_eof()? {
                return Ok(0);
            }
            read_header(&mut br)?;
            self.crc = Crc32::new();
            self.size = 0;
            self.inf = Some(Inflater::from_bits(br));
        }
    }
}

pub struct GzEncoder<W: Write> {
    def: Deflater<W>,
    crc: Crc32,
    size: u32,
}

impl<W: Write> GzEncoder<W> {
    pub fn new(mut inner: W) -> io::Result<Self> {
        inner.write_all(&[0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3])?;
        Ok(GzEncoder {
            def: Deflater::new(inner),
            crc: Crc32::new(),
            size: 0,
        })
    }

    pub fn finish(self) -> io::Result<W> {
        let mut w = self.def.finish()?;
        w.write_all(&self.crc.value().to_le_bytes())?;
        w.write_all(&self.size.to_le_bytes())?;
        Ok(w)
    }
}

impl<W: Write> Write for GzEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.def.write(buf)?;
        self.crc.update(&buf[..n]);
        self.size = self.size.wrapping_add(n as u32);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_multimember() {
        let mut e = GzEncoder::new(Vec::new()).unwrap();
        e.write_all(b"Bonjour Winlux ").unwrap();
        let mut a = e.finish().unwrap();
        let mut e = GzEncoder::new(Vec::new()).unwrap();
        e.write_all(b"et au revoir").unwrap();
        a.extend(e.finish().unwrap());
        let mut out = String::new();
        GzDecoder::new(&a[..])
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        assert_eq!(out, "Bonjour Winlux et au revoir");
    }

    #[test]
    fn system_gzip_compatible() {
        let Ok(mut child) = std::process::Command::new("gzip")
            .arg("-dc")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
        else {
            return;
        };
        let data: Vec<u8> = (0..200_000u32)
            .map(|i| (i % 251) as u8 ^ (i / 7) as u8)
            .collect();
        let mut e = GzEncoder::new(Vec::new()).unwrap();
        e.write_all(&data).unwrap();
        let gz = e.finish().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let t = std::thread::spawn(move || stdin.write_all(&gz));
        let out = child.wait_with_output().unwrap();
        t.join().unwrap().unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, data);
    }
}
