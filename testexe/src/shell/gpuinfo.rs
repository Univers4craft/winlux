//! Rapport GPU fourni par le runtime (`WinluxQueryGpu`), partagé par les
//! Paramètres et le Gestionnaire des tâches. Une clé absente veut dire que le
//! pilote ne l'expose pas : l'interface masque alors la ligne correspondante.
#![allow(dead_code)]

use winlux_testexe::*;

const CAP: usize = 8192;
static mut REPORT: [u8; CAP] = [0; CAP];
static mut LEN: usize = 0;
static mut LAST: u32 = 0;
static mut LOADED: bool = false;

/// Relit le rapport au plus une fois par seconde (ou immédiatement si `force`).
/// Renvoie `true` si son contenu a changé.
pub unsafe fn refresh(force: bool) -> bool {
    let now = GetTickCount();
    if LOADED && !force && now.wrapping_sub(LAST) < 1000 {
        return false;
    }
    LAST = now;
    LOADED = true;
    let mut next = [0u8; CAP];
    let full = WinluxQueryGpu(next.as_mut_ptr(), CAP as u32) as usize;
    let n = full.min(CAP - 1);
    let report = &*core::ptr::addr_of!(REPORT);
    let changed = n != LEN || next[..n] != report[..n];
    if changed {
        (&mut *core::ptr::addr_of_mut!(REPORT))[..n].copy_from_slice(&next[..n]);
        LEN = n;
    }
    changed
}

/// Remplace le rapport (tests de non-régression sans vrai GPU).
pub unsafe fn set_for_test(text: &[u8]) {
    let n = text.len().min(CAP - 1);
    (&mut *core::ptr::addr_of_mut!(REPORT))[..n].copy_from_slice(&text[..n]);
    LEN = n;
    LOADED = true;
    LAST = GetTickCount();
}

unsafe fn data() -> &'static [u8] {
    let report = &*core::ptr::addr_of!(REPORT);
    &report[..LEN]
}

fn line<'a>(data: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    data.split(|&c| c == b'\n').find_map(|l| {
        (l.len() > key.len() && &l[..key.len()] == key && l[key.len()] == b'=')
            .then(|| &l[key.len() + 1..])
    })
}

/// Valeur globale (`gpu_count`, `gpu_hybrid`).
pub unsafe fn global(key: &[u8]) -> Option<&'static [u8]> {
    line(data(), key)
}

/// Valeur `gpuN_clé`, ou `None` si le pilote ne la fournit pas.
pub unsafe fn get(i: usize, key: &[u8]) -> Option<&'static [u8]> {
    let mut full = [0u8; 48];
    let mut n = 0;
    for &c in b"gpu" {
        full[n] = c;
        n += 1;
    }
    n += fmt_u32(&mut full[n..], i as u32);
    full[n] = b'_';
    n += 1;
    let k = key.len().min(full.len() - n);
    full[n..n + k].copy_from_slice(&key[..k]);
    line(data(), &full[..n + k])
}

pub fn number(s: &[u8]) -> Option<u32> {
    if s.is_empty() || !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(s.iter().fold(0u32, |n, &c| {
        n.saturating_mul(10).saturating_add((c - b'0') as u32)
    }))
}

pub unsafe fn num(i: usize, key: &[u8]) -> Option<u32> {
    get(i, key).and_then(number)
}

pub unsafe fn count() -> usize {
    global(b"gpu_count").and_then(number).unwrap_or(0).min(8) as usize
}

pub unsafe fn hybrid() -> bool {
    global(b"gpu_hybrid") == Some(b"1")
}

/// Le GPU propose-t-il au moins une mesure en direct ?
pub unsafe fn has_metrics(i: usize) -> bool {
    [
        b"load".as_slice(),
        b"vram_used_mb",
        b"temp_c",
        b"freq_mhz",
        b"power_w",
    ]
    .iter()
    .any(|k| get(i, k).is_some())
}

pub fn fmt_u32(out: &mut [u8], v: u32) -> usize {
    let mut tmp = [0u8; 10];
    let mut n = 0;
    let mut v = v;
    loop {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let n = n.min(out.len());
    for i in 0..n {
        out[i] = tmp[n - 1 - i];
    }
    n
}

/// Petit tampon de texte sur la pile pour assembler une valeur affichée.
pub struct Line {
    pub buf: [u8; 96],
    pub len: usize,
}

impl Line {
    pub const fn new() -> Self {
        Line {
            buf: [0; 96],
            len: 0,
        }
    }
    pub fn push(&mut self, s: &[u8]) -> &mut Self {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s[..n]);
        self.len += n;
        self
    }
    pub fn num(&mut self, v: u32) -> &mut Self {
        let mut t = [0u8; 10];
        let n = fmt_u32(&mut t, v);
        self.push(&t[..n])
    }
    pub fn get(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

/// Résumé lisible des mesures disponibles, ex. « 37 %  ·  1,0/16 Go  ·  56 °C ».
pub unsafe fn summary(i: usize, out: &mut Line) {
    let sep = b"   \xB7   ";
    let mut first = true;
    let mut next = |out: &mut Line| {
        if !first {
            out.push(sep);
        }
        first = false;
    };
    if let Some(v) = num(i, b"load") {
        next(out);
        out.push(tr4(&[b"Charge ", b"Load ", b"Carga ", b"Last "]))
            .num(v)
            .push(b" %");
    }
    if let (Some(u), Some(t)) = (num(i, b"vram_used_mb"), num(i, b"vram_total_mb")) {
        next(out);
        out.push(b"VRAM ")
            .num(u)
            .push(b" / ")
            .num(t)
            .push(tr4(&[b" Mo", b" MB", b" MB", b" MB"]));
    }
    if let Some(v) = num(i, b"temp_c") {
        next(out);
        out.num(v).push(b" \xB0C");
    }
    if let Some(v) = num(i, b"freq_mhz") {
        next(out);
        out.push(tr4(&[
            b"Fr\xE9quence ",
            b"Frequency ",
            b"Frecuencia ",
            b"Frequenz ",
        ]))
        .num(v)
        .push(b" MHz");
    }
    if let Some(v) = num(i, b"power_w") {
        next(out);
        out.num(v).push(b" W");
    }
    if let Some(v) = num(i, b"encoder") {
        next(out);
        out.push(tr4(&[
            b"Encodeur ",
            b"Encoder ",
            b"Codificador ",
            b"Encoder ",
        ]))
        .num(v)
        .push(b" %");
    }
    if let Some(v) = num(i, b"decoder") {
        next(out);
        out.push(tr4(&[
            b"D\xE9codeur ",
            b"Decoder ",
            b"Decodificador ",
            b"Decoder ",
        ]))
        .num(v)
        .push(b" %");
    }
}

/// Libellé du rôle, uniquement sur une machine hybride.
pub fn role_label(role: &[u8]) -> Option<&'static [u8]> {
    match role {
        b"integrated" => Some(tr4(&[
            b"GPU int\xE9gr\xE9 (\xE9conomie d'\xE9nergie)",
            b"Integrated GPU (power saving)",
            b"GPU integrada (ahorro de energ\xEDa)",
            b"Integrierte GPU (Energiesparen)",
        ])),
        b"dedicated" => Some(tr4(&[
            b"GPU d\xE9di\xE9 (hautes performances)",
            b"Dedicated GPU (high performance)",
            b"GPU dedicada (alto rendimiento)",
            b"Dedizierte GPU (H\xF6chstleistung)",
        ])),
        _ => None,
    }
}
