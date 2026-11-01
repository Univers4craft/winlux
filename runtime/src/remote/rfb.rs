//! Protocole RFB 3.3/3.7/3.8 (VNC) côté serveur : poignée de main,
//! sécurité RSA-AES (RA2 = 5, RA2_256 = 129, compatibles TigerVNC) ou défi
//! VNC historique (2), puis envoi du framebuffer (Raw, Hextile, pointeur,
//! redimensionnement) et réception du clavier et de la souris.

use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::crypto::{self, Eax, RsaKey, RsaPublic};
use crate::screen::{Cursor, Frame, Input, Screen};

pub const SEC_VNC: u8 = 2;
pub const SEC_RA2: u8 = 5;
pub const SEC_RA2_256: u8 = 129;

const ENC_RAW: i32 = 0;
const ENC_HEXTILE: i32 = 5;
const ENC_CURSOR: i32 = -239;
const ENC_DESKTOP_SIZE: i32 = -223;

fn err(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, msg.into())
}

// ---------------------------------------------------------------------------
// Flux, éventuellement chiffrés (messages AES-EAX : longueur, données, tag)
// ---------------------------------------------------------------------------

struct Cipher {
    eax: Eax,
    nonce: [u8; 16],
}

impl Cipher {
    fn new(key: &[u8]) -> Cipher {
        Cipher {
            eax: Eax::new(key),
            nonce: [0; 16],
        }
    }
    fn advance(&mut self) {
        // Compteur de messages petit-boutiste, comme TigerVNC.
        for b in self.nonce.iter_mut() {
            *b = b.wrapping_add(1);
            if *b != 0 {
                break;
            }
        }
    }
}

pub struct In {
    inner: Box<dyn Read + Send>,
    cipher: Option<Cipher>,
    plain: Vec<u8>,
    pos: usize,
}

impl In {
    pub fn new(inner: Box<dyn Read + Send>) -> In {
        In {
            inner,
            cipher: None,
            plain: Vec::new(),
            pos: 0,
        }
    }
    fn fill(&mut self) -> io::Result<()> {
        let Some(c) = self.cipher.as_mut() else {
            return Ok(());
        };
        let mut len = [0u8; 2];
        self.inner.read_exact(&mut len)?;
        let n = u16::from_be_bytes(len) as usize;
        let mut data = vec![0u8; n];
        self.inner.read_exact(&mut data)?;
        let mut tag = [0u8; 16];
        self.inner.read_exact(&mut tag)?;
        if !c.eax.open(&c.nonce, &len, &mut data, &tag) {
            return Err(err("message chiffré altéré ou clé incorrecte"));
        }
        c.advance();
        self.plain.drain(..self.pos);
        self.pos = 0;
        self.plain.extend_from_slice(&data);
        Ok(())
    }
    pub fn read_exact(&mut self, out: &mut [u8]) -> io::Result<()> {
        if self.cipher.is_none() {
            return self.inner.read_exact(out);
        }
        while self.plain.len() - self.pos < out.len() {
            self.fill()?;
        }
        out.copy_from_slice(&self.plain[self.pos..self.pos + out.len()]);
        self.pos += out.len();
        Ok(())
    }
    pub fn bytes(&mut self, n: usize) -> io::Result<Vec<u8>> {
        let mut v = vec![0u8; n];
        self.read_exact(&mut v)?;
        Ok(v)
    }
    pub fn u8(&mut self) -> io::Result<u8> {
        let mut b = [0u8; 1];
        self.read_exact(&mut b)?;
        Ok(b[0])
    }
    pub fn u16(&mut self) -> io::Result<u16> {
        let mut b = [0u8; 2];
        self.read_exact(&mut b)?;
        Ok(u16::from_be_bytes(b))
    }
    pub fn u32(&mut self) -> io::Result<u32> {
        let mut b = [0u8; 4];
        self.read_exact(&mut b)?;
        Ok(u32::from_be_bytes(b))
    }
}

pub struct Out {
    inner: Box<dyn Write + Send>,
    cipher: Option<Cipher>,
    buf: Vec<u8>,
}

impl Out {
    pub fn new(inner: Box<dyn Write + Send>) -> Out {
        Out {
            inner,
            cipher: None,
            buf: Vec::new(),
        }
    }
    pub fn put(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.put(&v.to_be_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.put(&v.to_be_bytes());
    }
    pub fn flush(&mut self) -> io::Result<()> {
        let data = std::mem::take(&mut self.buf);
        match self.cipher.as_mut() {
            None => self.inner.write_all(&data)?,
            Some(c) => {
                let mut frame = Vec::with_capacity(data.len() + data.len() / 4096 * 18 + 18);
                for chunk in data.chunks(8192) {
                    let len = (chunk.len() as u16).to_be_bytes();
                    let mut sealed = chunk.to_vec();
                    let tag = c.eax.seal(&c.nonce, &len, &mut sealed);
                    c.advance();
                    frame.extend_from_slice(&len);
                    frame.extend_from_slice(&sealed);
                    frame.extend_from_slice(&tag);
                }
                self.inner.write_all(&frame)?;
            }
        }
        self.inner.flush()
    }
}

// ---------------------------------------------------------------------------
// Authentification
// ---------------------------------------------------------------------------

pub struct Security<'a> {
    pub key: &'a RsaKey,
    /// Vérifie un compte et son mot de passe (liste des utilisateurs autorisés).
    pub check: &'a (dyn Fn(&str, &[u8]) -> bool + Sync),
    /// Mot de passe VNC historique (sans chiffrement), si l'option est active.
    pub legacy_password: Option<Vec<u8>>,
}

/// Empreinte affichée par TigerVNC : 8 premiers octets du SHA-1 de la clé.
pub fn fingerprint(key: &RsaKey) -> String {
    let size = key.size();
    let digest = crypto::sha1(&[
        &((size * 8) as u32).to_be_bytes(),
        &key.n.to_be(size),
        &key.e.to_be(size),
    ]);
    digest[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("-")
}

fn send_reason(out: &mut Out, minor: u8, reason: &str) -> io::Result<()> {
    out.u32(1);
    if minor >= 8 {
        out.u32(reason.len() as u32);
        out.put(reason.as_bytes());
    }
    out.flush()
}

/// Poignée de main complète. Renvoie le compte authentifié (vide pour le
/// mot de passe VNC historique) et le niveau de protocole.
pub fn handshake(inp: &mut In, out: &mut Out, sec: &Security) -> io::Result<(String, u8)> {
    out.put(b"RFB 003.008\n");
    out.flush()?;
    let version = inp.bytes(12)?;
    if &version[..4] != b"RFB " || version[11] != b'\n' {
        return Err(err("client RFB invalide"));
    }
    let minor_raw: u32 = std::str::from_utf8(&version[8..11])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let minor = match minor_raw {
        0..=6 => 3,
        7 => 7,
        _ => 8,
    };
    let mut types = vec![SEC_RA2_256, SEC_RA2];
    if sec.legacy_password.is_some() {
        types.push(SEC_VNC);
    }
    let chosen = if minor == 3 {
        // RFB 3.3 : le serveur impose le type ; seul le défi VNC existe.
        if sec.legacy_password.is_none() {
            out.u32(0);
            let reason = "Ce client ne prend pas en charge le chiffrement RSA-AES.";
            out.u32(reason.len() as u32);
            out.put(reason.as_bytes());
            out.flush()?;
            return Err(err("client RFB 3.3 sans chiffrement refusé"));
        }
        out.u32(SEC_VNC as u32);
        out.flush()?;
        SEC_VNC
    } else {
        out.u8(types.len() as u8);
        out.put(&types);
        out.flush()?;
        let t = inp.u8()?;
        if !types.contains(&t) {
            send_reason(out, minor, "Type de sécurité refusé par Winlux.")?;
            return Err(err(format!("type de sécurité {t} refusé")));
        }
        t
    };
    let user = match chosen {
        SEC_VNC => {
            let mut challenge = [0u8; 16];
            crypto::random_bytes(&mut challenge);
            out.put(&challenge);
            out.flush()?;
            let response = inp.bytes(16)?;
            let expected =
                crypto::vnc_response(sec.legacy_password.as_deref().unwrap_or(b""), &challenge);
            if !constant_eq(&response, &expected) {
                send_reason(out, minor, "Mot de passe incorrect.")?;
                return Err(err("mot de passe VNC incorrect"));
            }
            String::new()
        }
        _ => {
            let (user, password) = rsa_aes(inp, out, sec.key, chosen == SEC_RA2_256)?;
            if !(sec.check)(&user, &password) {
                send_reason(
                    out,
                    minor,
                    "Compte ou mot de passe incorrect, ou compte non autorisé.",
                )?;
                return Err(err(format!("authentification refusée pour « {user} »")));
            }
            user
        }
    };
    out.u32(0);
    out.flush()?;
    Ok((user, minor))
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Sécurité RSA-AES : échange de clés publiques et d'aléas chiffrés,
/// dérivation des clés de session, vérification croisée des clés, puis
/// identifiants envoyés chiffrés. Tout le reste de la session est chiffré.
fn rsa_aes(inp: &mut In, out: &mut Out, key: &RsaKey, wide: bool) -> io::Result<(String, Vec<u8>)> {
    let size = key.size();
    let server_n = key.n.to_be(size);
    let server_e = key.e.to_be(size);
    let server_bits = (size * 8) as u32;
    out.u32(server_bits);
    out.put(&server_n);
    out.put(&server_e);
    out.flush()?;

    let client_bits = inp.u32()?;
    if !(1024..=8192).contains(&client_bits) && !cfg!(test) {
        return Err(err("clé RSA du client de taille invalide"));
    }
    let csize = (client_bits as usize).div_ceil(8);
    let client_n = inp.bytes(csize)?;
    let client_e = inp.bytes(csize)?;
    let client = RsaPublic {
        n: crypto::Big::from_be(&client_n),
        e: crypto::Big::from_be(&client_e),
        size: csize,
    };

    let rlen = if wide { 32 } else { 16 };
    let mut server_random = vec![0u8; rlen];
    crypto::random_bytes(&mut server_random);
    let sealed = client
        .encrypt(&server_random)
        .ok_or_else(|| err("clé RSA du client inutilisable"))?;
    out.u16(sealed.len() as u16);
    out.put(&sealed);
    out.flush()?;

    let len = inp.u16()? as usize;
    if len != size {
        return Err(err("aléa du client de taille invalide"));
    }
    let client_random = key
        .decrypt(&inp.bytes(len)?)
        .filter(|r| r.len() == rlen)
        .ok_or_else(|| err("aléa du client indéchiffrable"))?;

    let digest = |a: &[u8], b: &[u8]| -> Vec<u8> {
        if wide {
            crypto::sha256(&[a, b]).to_vec()
        } else {
            crypto::sha1(&[a, b])[..16].to_vec()
        }
    };
    inp.cipher = Some(Cipher::new(&digest(&server_random, &client_random)));
    out.cipher = Some(Cipher::new(&digest(&client_random, &server_random)));

    let hash = |first: (u32, &[u8], &[u8]), second: (u32, &[u8], &[u8])| -> Vec<u8> {
        let parts: [&[u8]; 6] = [
            &first.0.to_be_bytes(),
            first.1,
            first.2,
            &second.0.to_be_bytes(),
            second.1,
            second.2,
        ];
        if wide {
            crypto::sha256(&parts).to_vec()
        } else {
            crypto::sha1(&parts).to_vec()
        }
    };
    let server_side = (server_bits, server_n.as_slice(), server_e.as_slice());
    let client_side = (client_bits, client_n.as_slice(), client_e.as_slice());
    out.put(&hash(server_side, client_side));
    out.flush()?;
    let theirs = inp.bytes(if wide { 32 } else { 20 })?;
    if !constant_eq(&theirs, &hash(client_side, server_side)) {
        return Err(err("vérification des clés échouée (interception possible)"));
    }

    out.u8(1); // sous-type : compte + mot de passe
    out.flush()?;
    let ulen = inp.u8()? as usize;
    let user = String::from_utf8_lossy(&inp.bytes(ulen)?).into_owned();
    let plen = inp.u8()? as usize;
    let password = inp.bytes(plen)?;
    Ok((user, password))
}

// ---------------------------------------------------------------------------
// Format de pixel et encodages
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelFormat {
    pub bpp: u8,
    pub depth: u8,
    pub big_endian: bool,
    pub true_colour: bool,
    pub max: [u16; 3],
    pub shift: [u8; 3],
}

impl PixelFormat {
    pub const NATIVE: PixelFormat = PixelFormat {
        bpp: 32,
        depth: 24,
        big_endian: false,
        true_colour: true,
        max: [255, 255, 255],
        shift: [16, 8, 0],
    };
    fn write(&self, out: &mut Out) {
        out.put(&[
            self.bpp,
            self.depth,
            self.big_endian as u8,
            self.true_colour as u8,
        ]);
        for m in self.max {
            out.u16(m);
        }
        out.put(&self.shift);
        out.put(&[0, 0, 0]);
    }
    fn parse(b: &[u8]) -> PixelFormat {
        PixelFormat {
            bpp: b[0],
            depth: b[1],
            big_endian: b[2] != 0,
            true_colour: b[3] != 0,
            max: [
                u16::from_be_bytes([b[4], b[5]]),
                u16::from_be_bytes([b[6], b[7]]),
                u16::from_be_bytes([b[8], b[9]]),
            ],
            shift: [b[10], b[11], b[12]],
        }
    }
    fn valid(&self) -> bool {
        matches!(self.bpp, 8 | 16 | 32) && self.true_colour && self.shift.iter().all(|&s| s < 32)
    }
    pub fn encode(&self, rgb: u32, buf: &mut Vec<u8>) {
        if *self == PixelFormat::NATIVE {
            buf.extend_from_slice(&rgb.to_le_bytes());
            return;
        }
        let mut v = 0u32;
        for i in 0..3 {
            let c = (rgb >> (16 - 8 * i)) & 0xff;
            let scaled = (c * self.max[i] as u32 + 127) / 255;
            v |= scaled << self.shift[i];
        }
        match (self.bpp, self.big_endian) {
            (8, _) => buf.push(v as u8),
            (16, false) => buf.extend_from_slice(&(v as u16).to_le_bytes()),
            (16, true) => buf.extend_from_slice(&(v as u16).to_be_bytes()),
            (_, false) => buf.extend_from_slice(&v.to_le_bytes()),
            (_, true) => buf.extend_from_slice(&v.to_be_bytes()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

const TILE: usize = 64;

/// Zones modifiées entre deux images de même taille, par tuiles de 64 px
/// regroupées horizontalement.
pub fn dirty_rects(prev: &Frame, cur: &Frame) -> Vec<Rect> {
    let (w, h) = (cur.width as usize, cur.height as usize);
    let mut rects = Vec::new();
    for ty in (0..h).step_by(TILE) {
        let th = TILE.min(h - ty);
        let mut run: Option<usize> = None;
        let mut tx = 0;
        while tx < w {
            let tw = TILE.min(w - tx);
            let changed = (ty..ty + th).any(|y| {
                let o = y * w + tx;
                prev.pixels[o..o + tw] != cur.pixels[o..o + tw]
            });
            match (changed, run) {
                (true, None) => run = Some(tx),
                (false, Some(start)) => {
                    rects.push(Rect {
                        x: start as u16,
                        y: ty as u16,
                        w: (tx - start) as u16,
                        h: th as u16,
                    });
                    run = None;
                }
                _ => {}
            }
            tx += tw;
        }
        if let Some(start) = run {
            rects.push(Rect {
                x: start as u16,
                y: ty as u16,
                w: (w - start) as u16,
                h: th as u16,
            });
        }
    }
    rects
}

fn encode_raw(frame: &Frame, r: Rect, pf: &PixelFormat, buf: &mut Vec<u8>) {
    let w = frame.width as usize;
    for y in r.y as usize..(r.y + r.h) as usize {
        let row = &frame.pixels[y * w + r.x as usize..y * w + (r.x + r.w) as usize];
        if *pf == PixelFormat::NATIVE {
            for p in row {
                buf.extend_from_slice(&p.to_le_bytes());
            }
        } else {
            for &p in row {
                pf.encode(p, buf);
            }
        }
    }
}

/// Hextile : tuiles 16x16, unies (fond seul) ou brutes.
fn encode_hextile(frame: &Frame, r: Rect, pf: &PixelFormat, buf: &mut Vec<u8>) {
    let w = frame.width as usize;
    for ty in (r.y..r.y + r.h).step_by(16) {
        let th = 16.min(r.y + r.h - ty);
        for tx in (r.x..r.x + r.w).step_by(16) {
            let tw = 16.min(r.x + r.w - tx);
            let first = frame.pixels[ty as usize * w + tx as usize];
            let solid = (ty..ty + th).all(|y| {
                let o = y as usize * w + tx as usize;
                frame.pixels[o..o + tw as usize].iter().all(|&p| p == first)
            });
            if solid {
                buf.push(2);
                pf.encode(first, buf);
            } else {
                buf.push(1);
                encode_raw(
                    frame,
                    Rect {
                        x: tx,
                        y: ty,
                        w: tw,
                        h: th,
                    },
                    pf,
                    buf,
                );
            }
        }
    }
}

fn rect_header(buf: &mut Vec<u8>, r: Rect, encoding: i32) {
    for v in [r.x, r.y, r.w, r.h] {
        buf.extend_from_slice(&v.to_be_bytes());
    }
    buf.extend_from_slice(&encoding.to_be_bytes());
}

fn encode_cursor(c: &Cursor, pf: &PixelFormat, buf: &mut Vec<u8>) {
    rect_header(
        buf,
        Rect {
            x: c.xhot,
            y: c.yhot,
            w: c.width,
            h: c.height,
        },
        ENC_CURSOR,
    );
    for &p in &c.argb {
        pf.encode(p & 0xff_ffff, buf);
    }
    let row = (c.width as usize).div_ceil(8);
    for y in 0..c.height as usize {
        let mut bits = vec![0u8; row];
        for x in 0..c.width as usize {
            if c.argb[y * c.width as usize + x] >> 24 >= 128 {
                bits[x / 8] |= 0x80 >> (x % 8);
            }
        }
        buf.extend_from_slice(&bits);
    }
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Shared {
    pf: Option<PixelFormat>,
    hextile: bool,
    cursor: bool,
    desktop_size: bool,
    /// Demande en attente : (incrémentale, zone).
    request: Option<(bool, Rect)>,
    closed: bool,
}

type Link = Arc<(Mutex<Shared>, Condvar)>;

/// Lit les messages du client jusqu'à la déconnexion.
fn reader(mut inp: In, link: Link, mut input: Option<Box<dyn Input>>) -> io::Result<()> {
    let result = (|| -> io::Result<()> {
        loop {
            let kind = inp.u8()?;
            match kind {
                0 => {
                    let b = inp.bytes(19)?;
                    let pf = PixelFormat::parse(&b[3..]);
                    if !pf.valid() {
                        return Err(err("format de pixel non pris en charge (palette)"));
                    }
                    link.0.lock().unwrap().pf = Some(pf);
                }
                2 => {
                    inp.u8()?;
                    let n = inp.u16()? as usize;
                    let mut s = link.0.lock().unwrap();
                    s.hextile = false;
                    s.cursor = false;
                    s.desktop_size = false;
                    for _ in 0..n {
                        match inp.u32()? as i32 {
                            ENC_HEXTILE => s.hextile = true,
                            ENC_CURSOR => s.cursor = true,
                            ENC_DESKTOP_SIZE => s.desktop_size = true,
                            _ => {}
                        }
                    }
                }
                3 => {
                    let incremental = inp.u8()? != 0;
                    let r = Rect {
                        x: inp.u16()?,
                        y: inp.u16()?,
                        w: inp.u16()?,
                        h: inp.u16()?,
                    };
                    let (lock, cv) = &*link;
                    let mut s = lock.lock().unwrap();
                    // Une demande complète reste complète même si une
                    // demande incrémentale arrive avant l'envoi.
                    let incremental = incremental && s.request.is_none_or(|(i, _)| i);
                    s.request = Some((incremental, r));
                    cv.notify_all();
                }
                4 => {
                    let down = inp.u8()? != 0;
                    inp.u16()?;
                    let keysym = inp.u32()?;
                    if std::env::var_os("WINLUX_REMOTE_DEBUG").is_some() {
                        eprintln!(
                            "winlux-remote : touche {keysym:#x} {}",
                            if down { "enfoncée" } else { "relâchée" }
                        );
                    }
                    if let Some(i) = input.as_mut() {
                        i.key(down, keysym);
                    }
                }
                5 => {
                    let mask = inp.u8()?;
                    let x = inp.u16()?;
                    let y = inp.u16()?;
                    if let Some(i) = input.as_mut() {
                        i.pointer(mask, x, y);
                    }
                }
                6 => {
                    inp.bytes(3)?;
                    let len = inp.u32()? & 0x7fff_ffff;
                    if len > 16 << 20 {
                        return Err(err("presse-papiers trop volumineux"));
                    }
                    inp.bytes(len as usize)?;
                }
                other => return Err(err(format!("message client inconnu : {other}"))),
            }
        }
    })();
    let (lock, cv) = &*link;
    lock.lock().unwrap().closed = true;
    cv.notify_all();
    result
}

pub struct Options {
    pub name: String,
    pub view_only: bool,
    /// Intervalle minimal entre deux mises à jour.
    pub frame_interval: Duration,
}

/// Initialisation puis boucle d'envoi de l'écran. Bloque jusqu'à la fin.
pub fn serve(
    mut inp: In,
    mut out: Out,
    mut screen: Box<dyn Screen>,
    input: Option<Box<dyn Input>>,
    opts: &Options,
) -> io::Result<()> {
    let _shared_flag = inp.u8()?;
    let mut frame = screen
        .capture()
        .ok_or_else(|| err("capture de l'écran impossible"))?;
    out.u16(frame.width);
    out.u16(frame.height);
    PixelFormat::NATIVE.write(&mut out);
    out.u32(opts.name.len() as u32);
    out.put(opts.name.as_bytes());
    out.flush()?;

    let link: Link = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
    let reader_link = link.clone();
    let input = if opts.view_only { None } else { input };
    let reader_thread = std::thread::spawn(move || reader(inp, reader_link, input));

    let mut prev: Option<Frame> = None;
    let mut last_send = Instant::now() - opts.frame_interval;
    let mut fb_size = (frame.width, frame.height);
    let mut pending: Option<(bool, Rect)>;
    loop {
        let (incremental, flags) = {
            let (lock, cv) = &*link;
            let mut s = lock.lock().unwrap();
            while s.request.is_none() && !s.closed {
                s = cv.wait_timeout(s, Duration::from_millis(200)).unwrap().0;
            }
            if s.closed {
                break;
            }
            // Take the request now: one the client sends while this frame
            // is being encoded must survive for the next round.
            let request = s.request.take().unwrap();
            let (inc, _) = request;
            pending = Some(request);
            (
                inc,
                (
                    s.pf.unwrap_or(PixelFormat::NATIVE),
                    s.hextile,
                    s.cursor,
                    s.desktop_size,
                ),
            )
        };
        let (pf, hextile, want_cursor, desktop_size) = flags;
        let since = last_send.elapsed();
        if since < opts.frame_interval {
            std::thread::sleep(opts.frame_interval - since);
        }
        if let Some(f) = screen.capture() {
            frame = f;
        }
        let mut buf = Vec::new();
        let mut count = 0u16;
        let mut full = !incremental || prev.is_none();
        if (frame.width, frame.height) != fb_size {
            if desktop_size {
                rect_header(
                    &mut buf,
                    Rect {
                        x: 0,
                        y: 0,
                        w: frame.width,
                        h: frame.height,
                    },
                    ENC_DESKTOP_SIZE,
                );
                count += 1;
                fb_size = (frame.width, frame.height);
                full = true;
            } else {
                // Client sans redimensionnement : il garde l'ancienne taille.
                frame = fit(&frame, fb_size);
            }
        }
        let rects = if full {
            vec![Rect {
                x: 0,
                y: 0,
                w: frame.width,
                h: frame.height,
            }]
        } else {
            dirty_rects(prev.as_ref().unwrap(), &frame)
        };
        if want_cursor {
            if let Some(c) = screen.cursor() {
                encode_cursor(&c, &pf, &mut buf);
                count += 1;
            }
        }
        for r in &rects {
            if r.w == 0 || r.h == 0 {
                continue;
            }
            rect_header(&mut buf, *r, if hextile { ENC_HEXTILE } else { ENC_RAW });
            if hextile {
                encode_hextile(&frame, *r, &pf, &mut buf);
            } else {
                encode_raw(&frame, *r, &pf, &mut buf);
            }
            count += 1;
        }
        if count == 0 {
            // Rien de neuf : la demande incrémentale reste en attente,
            // sauf si le client en a envoyé une nouvelle entre-temps.
            let mut s = link.0.lock().unwrap();
            if s.request.is_none() {
                s.request = pending.take();
            }
            drop(s);
            last_send = Instant::now();
            continue;
        }
        out.u8(0);
        out.u8(0);
        out.u16(count);
        out.put(&buf);
        out.flush()?;
        last_send = Instant::now();
        prev = Some(frame.clone());
    }
    match reader_thread.join() {
        Ok(Err(e)) if e.kind() != io::ErrorKind::UnexpectedEof => Err(e),
        _ => Ok(()),
    }
}

/// Recadre ou complète une image à la taille annoncée au client.
fn fit(frame: &Frame, (w, h): (u16, u16)) -> Frame {
    let mut pixels = vec![0u32; w as usize * h as usize];
    for y in 0..h.min(frame.height) as usize {
        let n = w.min(frame.width) as usize;
        pixels[y * w as usize..y * w as usize + n]
            .copy_from_slice(&frame.pixels[y * frame.width as usize..y * frame.width as usize + n]);
    }
    Frame {
        width: w,
        height: h,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    /// Client de test indépendant : rejoue le protocole RA2 côté client.
    fn client_ra2(
        stream: UnixStream,
        wide: bool,
        user: &str,
        password: &str,
    ) -> io::Result<(In, Out, u32)> {
        let mut inp = In::new(Box::new(stream.try_clone()?));
        let mut out = Out::new(Box::new(stream));
        let v = inp.bytes(12)?;
        assert_eq!(&v, b"RFB 003.008\n");
        out.put(b"RFB 003.008\n");
        out.flush()?;
        let n = inp.u8()? as usize;
        let types = inp.bytes(n)?;
        assert!(types.contains(&SEC_RA2_256) && types.contains(&SEC_RA2));
        out.u8(if wide { SEC_RA2_256 } else { SEC_RA2 });
        out.flush()?;
        let sbits = inp.u32()?;
        let ssize = (sbits as usize).div_ceil(8);
        let sn = inp.bytes(ssize)?;
        let se = inp.bytes(ssize)?;
        let ckey = RsaKey::generate(512);
        let csize = ckey.size();
        let (cn, ce) = (ckey.n.to_be(csize), ckey.e.to_be(csize));
        out.u32((csize * 8) as u32);
        out.put(&cn);
        out.put(&ce);
        out.flush()?;
        let len = inp.u16()? as usize;
        let server_random = ckey.decrypt(&inp.bytes(len)?).unwrap();
        let rlen = if wide { 32 } else { 16 };
        assert_eq!(server_random.len(), rlen);
        let mut client_random = vec![0u8; rlen];
        crypto::random_bytes(&mut client_random);
        let spub = RsaPublic {
            n: crypto::Big::from_be(&sn),
            e: crypto::Big::from_be(&se),
            size: ssize,
        };
        let sealed = spub.encrypt(&client_random).unwrap();
        out.u16(sealed.len() as u16);
        out.put(&sealed);
        out.flush()?;
        let digest = |a: &[u8], b: &[u8]| -> Vec<u8> {
            if wide {
                crypto::sha256(&[a, b]).to_vec()
            } else {
                crypto::sha1(&[a, b])[..16].to_vec()
            }
        };
        inp.cipher = Some(Cipher::new(&digest(&client_random, &server_random)));
        out.cipher = Some(Cipher::new(&digest(&server_random, &client_random)));
        let hlen = if wide { 32 } else { 20 };
        let server_hash = inp.bytes(hlen)?;
        let parts = |a: (u32, &[u8], &[u8]), b: (u32, &[u8], &[u8])| -> Vec<u8> {
            let p: [&[u8]; 6] = [&a.0.to_be_bytes(), a.1, a.2, &b.0.to_be_bytes(), b.1, b.2];
            if wide {
                crypto::sha256(&p).to_vec()
            } else {
                crypto::sha1(&p).to_vec()
            }
        };
        let s = (sbits, sn.as_slice(), se.as_slice());
        let c = ((csize * 8) as u32, cn.as_slice(), ce.as_slice());
        assert_eq!(server_hash, parts(s, c));
        out.put(&parts(c, s));
        out.flush()?;
        assert_eq!(inp.u8()?, 1);
        out.u8(user.len() as u8);
        out.put(user.as_bytes());
        out.u8(password.len() as u8);
        out.put(password.as_bytes());
        out.flush()?;
        let result = inp.u32()?;
        Ok((inp, out, result))
    }

    struct FakeScreen(u32);
    impl Screen for FakeScreen {
        fn capture(&mut self) -> Option<Frame> {
            self.0 += 1;
            let mut pixels = vec![0x00336699u32; 40 * 20];
            // Init et première image : unie ; ensuite un pixel change.
            if self.0 > 2 {
                pixels[5 * 40 + 3] = 0x00ff0000;
            }
            Some(Frame {
                width: 40,
                height: 20,
                pixels,
            })
        }
        fn cursor(&mut self) -> Option<Cursor> {
            None
        }
    }
    struct FakeInput(Arc<Mutex<Vec<String>>>);
    impl Input for FakeInput {
        fn key(&mut self, down: bool, keysym: u32) {
            self.0
                .lock()
                .unwrap()
                .push(format!("key {down} {keysym:x}"));
        }
        fn pointer(&mut self, b: u8, x: u16, y: u16) {
            self.0.lock().unwrap().push(format!("ptr {b} {x} {y}"));
        }
    }

    fn server(
        stream: UnixStream,
        key: Arc<RsaKey>,
        events: Arc<Mutex<Vec<String>>>,
    ) -> std::thread::JoinHandle<io::Result<String>> {
        std::thread::spawn(move || {
            let mut inp = In::new(Box::new(stream.try_clone()?));
            let mut out = Out::new(Box::new(stream));
            let check = |u: &str, p: &[u8]| u == "alice" && p == b"s3cret";
            let sec = Security {
                key: &key,
                check: &check,
                legacy_password: None,
            };
            let (user, _) = handshake(&mut inp, &mut out, &sec)?;
            serve(
                inp,
                out,
                Box::new(FakeScreen(0)),
                Some(Box::new(FakeInput(events))),
                &Options {
                    name: "Winlux test".into(),
                    view_only: false,
                    frame_interval: Duration::from_millis(1),
                },
            )?;
            Ok(user)
        })
    }

    #[test]
    fn session_chiffree_ra2_256_de_bout_en_bout() {
        let key = Arc::new(RsaKey::generate(768));
        for wide in [true, false] {
            let (a, b) = UnixStream::pair().unwrap();
            let events = Arc::new(Mutex::new(Vec::new()));
            let srv = server(a, key.clone(), events.clone());
            let (mut inp, mut out, result) = client_ra2(b, wide, "alice", "s3cret").unwrap();
            assert_eq!(result, 0);
            out.u8(1);
            out.flush().unwrap();
            let (w, h) = (inp.u16().unwrap(), inp.u16().unwrap());
            assert_eq!((w, h), (40, 20));
            inp.bytes(16).unwrap();
            let nlen = inp.u32().unwrap() as usize;
            assert_eq!(inp.bytes(nlen).unwrap(), b"Winlux test");
            // Encodages : Hextile ; demande complète.
            out.put(&[2, 0, 0, 1]);
            out.u32(ENC_HEXTILE as u32);
            out.put(&[3, 0, 0, 0, 0, 0, 0, 40, 0, 20]);
            out.flush().unwrap();
            assert_eq!(inp.u8().unwrap(), 0);
            inp.u8().unwrap();
            assert_eq!(inp.u16().unwrap(), 1);
            let hdr = inp.bytes(12).unwrap();
            assert_eq!(&hdr[8..], &5i32.to_be_bytes());
            // 3 x 2 tuiles 16x16 unies : sous-encodage 2 + pixel.
            for _ in 0..6 {
                assert_eq!(inp.u8().unwrap(), 2);
                assert_eq!(inp.bytes(4).unwrap(), 0x00336699u32.to_le_bytes());
            }
            // Mise à jour incrémentale : seule la tuile modifiée revient.
            out.put(&[3, 1, 0, 0, 0, 0, 0, 40, 0, 20]);
            out.flush().unwrap();
            assert_eq!(inp.u8().unwrap(), 0);
            inp.u8().unwrap();
            assert_eq!(inp.u16().unwrap(), 1);
            let hdr = inp.bytes(12).unwrap();
            assert_eq!(&hdr[..8], &[0, 0, 0, 0, 0, 40, 0, 20]);
            // Clavier et souris, puis déconnexion propre.
            out.put(&[4, 1, 0, 0, 0, 0, 0, 0x61]);
            out.put(&[5, 1, 0, 10, 0, 7]);
            out.flush().unwrap();
            drop(inp);
            drop(out);
            assert_eq!(srv.join().unwrap().unwrap(), "alice");
            let ev = events.lock().unwrap().clone();
            assert_eq!(
                ev,
                vec!["key true 61".to_string(), "ptr 1 10 7".to_string()]
            );
        }
    }

    #[test]
    fn mauvais_mot_de_passe_refuse() {
        let key = Arc::new(RsaKey::generate(768));
        let (a, b) = UnixStream::pair().unwrap();
        let srv = server(a, key, Arc::new(Mutex::new(Vec::new())));
        let (mut inp, _out, result) = client_ra2(b, true, "alice", "faux").unwrap();
        assert_eq!(result, 1);
        let len = inp.u32().unwrap() as usize;
        assert!(String::from_utf8(inp.bytes(len).unwrap())
            .unwrap()
            .contains("incorrect"));
        assert!(srv.join().unwrap().is_err());
    }

    #[test]
    fn defi_vnc_et_refus_sans_chiffrement() {
        let key = RsaKey::generate(512);
        let (a, b) = UnixStream::pair().unwrap();
        let srv = std::thread::spawn(move || {
            let mut inp = In::new(Box::new(a.try_clone().unwrap()));
            let mut out = Out::new(Box::new(a));
            let check = |_: &str, _: &[u8]| false;
            let sec = Security {
                key: &key,
                check: &check,
                legacy_password: Some(b"pass".to_vec()),
            };
            handshake(&mut inp, &mut out, &sec).map(|r| r.0)
        });
        let mut inp = In::new(Box::new(b.try_clone().unwrap()));
        let mut out = Out::new(Box::new(b));
        inp.bytes(12).unwrap();
        out.put(b"RFB 003.008\n");
        out.flush().unwrap();
        let n = inp.u8().unwrap() as usize;
        assert!(inp.bytes(n).unwrap().contains(&SEC_VNC));
        out.u8(SEC_VNC);
        out.flush().unwrap();
        let challenge: [u8; 16] = inp.bytes(16).unwrap().try_into().unwrap();
        out.put(&crypto::vnc_response(b"pass", &challenge));
        out.flush().unwrap();
        assert_eq!(inp.u32().unwrap(), 0);
        assert_eq!(srv.join().unwrap().unwrap(), "");
    }

    #[test]
    fn zones_modifiees_et_formats() {
        let a = Frame {
            width: 200,
            height: 70,
            pixels: vec![0; 200 * 70],
        };
        let mut b = a.clone();
        b.pixels[10 * 200 + 70] = 1; // tuile (64..128, 0..64)
        b.pixels[10 * 200 + 130] = 1; // tuile voisine : fusion
        b.pixels[66 * 200 + 199] = 1; // dernière rangée, dernière tuile
        assert_eq!(
            dirty_rects(&a, &b),
            vec![
                Rect {
                    x: 64,
                    y: 0,
                    w: 128,
                    h: 64
                },
                Rect {
                    x: 192,
                    y: 64,
                    w: 8,
                    h: 6
                }
            ]
        );
        let pf = PixelFormat {
            bpp: 16,
            depth: 16,
            big_endian: false,
            true_colour: true,
            max: [31, 63, 31],
            shift: [11, 5, 0],
        };
        let mut buf = Vec::new();
        pf.encode(0x00ff0000, &mut buf);
        assert_eq!(buf, 0xf800u16.to_le_bytes());
        assert!(fingerprint(&RsaKey::generate(512)).split('-').count() == 8);
    }
}
