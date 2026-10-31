//! Messages des disques branchés (clé USB, disque NTFS d'un Windows) : le
//! service `winlux --volumes daemon` écrit `Z:\run\winlux\volumes\notices.txt`
//! (`numéro  code  lettre  libellé`), le bureau en fait des notifications
//! dans la langue de l'interface.

use crate::admin_ui::read_file;
use winlux_testexe::*;

static mut LAST_POLL: u32 = 0;
/// Dernier message affiché ; `u32::MAX` tant que le fichier n'a pas été lu
/// (les messages d'avant l'ouverture de session ne sont pas rejoués).
static mut LAST_SEQ: u32 = u32::MAX;
static mut BUF: [u8; 8192] = [0; 8192];

/// Texte d'un message : `nom` est inséré à la place de `{}`.
pub fn message(code: &[u8]) -> Option<&'static [u8]> {
    let t: &Tr4 = match code {
        b"mounted" => &[
            b"{} est pr\xEAt.",
            b"{} is ready.",
            b"{} est\xE1 listo.",
            b"{} ist bereit.",
        ],
        b"hibernated" => &[
            b"{} : Windows est en veille (d\xE9marrage rapide). Ouvert en lecture seule.",
            b"{}: Windows is hibernated (Fast Startup). Opened read-only.",
            b"{}: Windows est\xE1 hibernado (inicio r\xE1pido). Abierto en solo lectura.",
            b"{}: Windows ist im Ruhezustand (Schnellstart). Schreibgesch\xFCtzt ge\xF6ffnet.",
        ],
        b"dirty" => &[
            b"{} mal ferm\xE9 par Windows : lecture seule. R\xE9parez-le dans Gestion des disques.",
            b"{} was not closed properly: read-only. Repair it in Disk Management.",
            b"{} no se cerr\xF3 bien: solo lectura. Rep\xE1relo en Administraci\xF3n de discos.",
            b"{} nicht sauber getrennt: schreibgesch\xFCtzt. In der Datentr\xE4gerverwaltung reparieren.",
        ],
        b"bitlocker" => &[
            b"{} est chiffr\xE9 avec BitLocker et ne peut pas \xEAtre ouvert.",
            b"{} is encrypted with BitLocker and cannot be opened.",
            b"{} est\xE1 cifrado con BitLocker y no se puede abrir.",
            b"{} ist mit BitLocker verschl\xFCsselt und kann nicht ge\xF6ffnet werden.",
        ],
        b"failed" => &[
            b"{} n'a pas pu \xEAtre ouvert. V\xE9rifiez-le sous Windows (chkdsk /f).",
            b"{} could not be opened. Check it in Windows (chkdsk /f).",
            b"No se pudo abrir {}. Compru\xE9belo en Windows (chkdsk /f).",
            b"{} konnte nicht ge\xF6ffnet werden. Unter Windows pr\xFCfen (chkdsk /f).",
        ],
        b"repaired" => &[
            b"{} est r\xE9par\xE9 : l'\xE9criture est de nouveau possible.",
            b"{} was repaired and can be written to again.",
            b"{} se repar\xF3 y se puede volver a escribir.",
            b"{} wurde repariert und ist wieder beschreibbar.",
        ],
        _ => return None,
    };
    Some(tr4(t))
}

fn column(line: &[u8], i: usize) -> &[u8] {
    line.split(|&c| c == b'\t').nth(i).unwrap_or(b"")
}

fn number(s: &[u8]) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let mut v = 0u32;
    for &c in s {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v.wrapping_mul(10).wrapping_add((c - b'0') as u32);
    }
    Some(v)
}

/// Compose « libellé (E:) » dans le modèle du message.
fn compose(out: &mut [u8], tpl: &[u8], letter: &[u8], label: &[u8]) -> usize {
    let mut name = [0u8; 80];
    let mut n = 0;
    for &c in label.iter().take(60) {
        name[n] = c;
        n += 1;
    }
    if let Some(&l) = letter.first() {
        if n > 0 {
            name[n] = b' ';
            name[n + 1] = b'(';
            n += 2;
        }
        name[n] = l;
        name[n + 1] = b':';
        n += 2;
        if !label.is_empty() {
            name[n] = b')';
            n += 1;
        }
    }
    let mut len = 0;
    let mut i = 0;
    while i < tpl.len() && len < out.len() {
        if tpl[i..].starts_with(b"{}") {
            for &c in &name[..n] {
                if len < out.len() {
                    out[len] = c;
                    len += 1;
                }
            }
            i += 2;
        } else {
            out[len] = tpl[i];
            len += 1;
            i += 1;
        }
    }
    len
}

/// Surveillance (appelée par le bureau toutes les images) : un nouveau
/// message du service devient une notification.
pub unsafe fn system_tick() -> bool {
    let now = GetTickCount();
    if now.wrapping_sub(LAST_POLL) < 2000 {
        return false;
    }
    LAST_POLL = now;
    let buf = &mut *core::ptr::addr_of_mut!(BUF);
    let len = read_file(c"Z:\\run\\winlux\\volumes\\notices.txt".as_ptr(), buf, true);
    let mut shown = false;
    let first = LAST_SEQ == u32::MAX;
    let mut last = if first { 0 } else { LAST_SEQ };
    for line in buf[..len].split(|&c| c == b'\n') {
        let Some(seq) = number(column(line, 0)) else {
            continue;
        };
        if seq <= last {
            continue;
        }
        last = seq;
        if first {
            continue;
        }
        if let Some(tpl) = message(column(line, 1)) {
            let mut msg = [0u8; 200];
            let n = compose(&mut msg, tpl, column(line, 2), column(line, 3));
            crate::notifications::push(
                tr4(&[b"Disques", b"Drives", b"Unidades", b"Laufwerke"]),
                &msg[..n],
            );
            shown = true;
        }
    }
    LAST_SEQ = last;
    shown
}
