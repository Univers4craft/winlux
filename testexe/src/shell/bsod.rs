//! Écran bleu de la mort (BSOD) style Windows 10/11 : affiché quand le
//! shell détecte un crash critique via /run/winlux/crash-bsod. Affiche le
//! stop code, le QR code (simplifié en grille), le message d'erreur, et un
//! compteur de redémarrage automatique.

use crate::gfx::*;
use crate::wm::Gfx;
use winlux_testexe::*;

pub static mut ACTIVE: bool = false;
static mut STOP_CODE: [u8; 64] = [0; 64];
static mut STOP_LEN: usize = 0;
static mut EXCEPTION: [u8; 64] = [0; 64];
static mut EXCEPTION_LEN: usize = 0;
static mut ERROR_CODE: [u8; 20] = [0; 20];
static mut ERROR_CODE_LEN: usize = 0;
static mut FAULT_ADDR: [u8; 20] = [0; 20];
static mut FAULT_ADDR_LEN: usize = 0;
static mut EXE_NAME: [u8; 128] = [0; 128];
static mut EXE_NAME_LEN: usize = 0;
static mut PROGRESS: u32 = 0;
static mut START_TIME: u32 = 0;
static mut LAST_CHECK: u32 = 0;
const BSOD_DURATION_MS: u32 = 15000;

/// Propose de signaler le dernier plantage (boîte de dialogue winlux-report,
/// qui ne propose rien si l'utilisateur l'a désactivé dans Paramètres).
unsafe fn offer_report() {
    ShellExecuteA(
        core::ptr::null_mut(),
        c"open".as_ptr(),
        c"winlux-report".as_ptr(),
        c"--crash".as_ptr(),
        core::ptr::null(),
        0,
    );
}

pub unsafe fn check_crash() {
    let now = GetTickCount();
    if ACTIVE || now.wrapping_sub(LAST_CHECK) < 2000 {
        return;
    }
    LAST_CHECK = now;
    // Échec sans faute mémoire (fonction Windows absente, erreur interne) :
    // pas d'écran bleu, seulement la proposition de signalement.
    let offer = c"Z:\\run\\winlux\\crash-offer".as_ptr();
    if GetFileAttributesA(offer) != u32::MAX {
        DeleteFileA(offer);
        offer_report();
    }
    let path = c"Z:\\run\\winlux\\crash-bsod".as_ptr();
    let handle = CreateFileA(
        path,
        0x80000000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if handle == -1isize as Handle {
        return;
    }
    let mut buf = [0u8; 512];
    let mut bytes_read = 0u32;
    ReadFile(
        handle,
        buf.as_mut_ptr().cast(),
        buf.len() as u32,
        &mut bytes_read,
        core::ptr::null_mut(),
    );
    CloseHandle(handle);
    if bytes_read == 0 {
        return;
    }
    // Supprimer le fichier pour ne pas re-déclencher
    DeleteFileA(path);

    // Parser: stop_code\nexception\ncode\naddr\nexe_name\n
    let data = &buf[..bytes_read as usize];
    let mut lines = data.split(|&b| b == b'\n');
    if let Some(line) = lines.next() {
        let n = line.len().min(STOP_CODE.len());
        STOP_CODE[..n].copy_from_slice(&line[..n]);
        STOP_LEN = n;
    }
    if let Some(line) = lines.next() {
        let n = line.len().min(EXCEPTION.len());
        EXCEPTION[..n].copy_from_slice(&line[..n]);
        EXCEPTION_LEN = n;
    }
    if let Some(line) = lines.next() {
        let n = line.len().min(ERROR_CODE.len());
        ERROR_CODE[..n].copy_from_slice(&line[..n]);
        ERROR_CODE_LEN = n;
    }
    if let Some(line) = lines.next() {
        let n = line.len().min(FAULT_ADDR.len());
        FAULT_ADDR[..n].copy_from_slice(&line[..n]);
        FAULT_ADDR_LEN = n;
    }
    if let Some(line) = lines.next() {
        let n = line.len().min(EXE_NAME.len());
        EXE_NAME[..n].copy_from_slice(&line[..n]);
        EXE_NAME_LEN = n;
    }

    ACTIVE = true;
    PROGRESS = 0;
    START_TIME = now;
}

pub unsafe fn tick() -> bool {
    if !ACTIVE {
        return false;
    }
    let elapsed = GetTickCount().wrapping_sub(START_TIME);
    PROGRESS = ((elapsed * 100) / BSOD_DURATION_MS).min(100) as u32;
    if elapsed >= BSOD_DURATION_MS {
        dismiss();
    }
    true
}

pub unsafe fn dismiss() {
    if ACTIVE {
        ACTIVE = false;
        offer_report();
    }
}

pub unsafe fn draw(hdc: Handle, b: &mut Brushes, f: &Fonts, w: i32, h: i32) {
    if !ACTIVE {
        return;
    }
    // Fond bleu Windows BSOD
    let blue = rgb(0, 120, 215);
    fill(hdc, b, 0, 0, w, h, blue);

    SetBkMode(hdc, 1); // TRANSPARENT
    SetTextColor(hdc, 0x00FFFFFF);

    // Smiley triste :(
    let sad_font = CreateFontA(
        120,
        0,
        0,
        0,
        200,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"DejaVu Sans".as_ptr(),
    );
    SelectObject(hdc, sad_font);
    TextOutA(hdc, w / 10, h / 8, c":(".as_ptr(), 2);
    DeleteObject(sad_font);

    let title_font = CreateFontA(
        22,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"DejaVu Sans".as_ptr(),
    );
    let body_font = CreateFontA(
        14,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"DejaVu Sans".as_ptr(),
    );
    let small_font = CreateFontA(
        11,
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"DejaVu Sans".as_ptr(),
    );
    let code_font = CreateFontA(
        13,
        0,
        0,
        0,
        700,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        c"DejaVu Sans".as_ptr(),
    );

    let left = w / 10;
    let mut y = h / 8 + 140;

    // Message principal
    SelectObject(hdc, title_font);
    TextOutA(
        hdc,
        left,
        y,
        c"Votre Winlux a rencontr\xE9 un probl\xE8me et doit red\xE9marrer.".as_ptr(),
        -1,
    );
    y += 30;
    SelectObject(hdc, body_font);
    TextOutA(
        hdc,
        left,
        y,
        c"Nous collectons des informations sur l'erreur, puis nous red\xE9marrerons pour vous."
            .as_ptr(),
        -1,
    );
    y += 30;

    // Barre de progression
    let mut pct_buf = [0u8; 8];
    let pct_len = fmt_u32(PROGRESS, &mut pct_buf);
    let mut prog_text = [0u8; 16];
    prog_text[..pct_len].copy_from_slice(&pct_buf[..pct_len]);
    prog_text[pct_len] = b'%';
    prog_text[pct_len + 1] = b' ';
    prog_text[pct_len + 2] = b't';
    prog_text[pct_len + 3] = b'e';
    prog_text[pct_len + 4] = b'r';
    prog_text[pct_len + 5] = b'm';
    prog_text[pct_len + 6] = b'i';
    prog_text[pct_len + 7] = b'n';
    let plen = pct_len + 8;
    // Avoid \xe9 in the slice since it's not easy without alloc
    prog_text[plen.min(15)] = 0;
    SelectObject(hdc, title_font);
    TextOutA(hdc, left, y, prog_text.as_ptr().cast(), plen as i32);
    y += 50;

    // QR code simplifié (grille décorative représentant un QR code)
    let qr_x = left;
    let qr_y = y;
    let qr_size = 80;
    let cell = qr_size / 10;
    let white = rgb(255, 255, 255);
    // Motifs d'angle (coins du QR code)
    for row in 0..10 {
        for col in 0..10 {
            let is_corner = (row < 3 && col < 3) || (row < 3 && col >= 7) || (row >= 7 && col < 3);
            let is_border = is_corner
                && (row == 0
                    || row == 2
                    || col == 0
                    || col == 2
                    || col == 7
                    || col == 9
                    || row == 7
                    || row == 9);
            let is_center = is_corner
                && row != 0
                && row != 2
                && col != 0
                && col != 2
                && col != 7
                && col != 9
                && row != 7
                && row != 9;
            let is_data = !is_corner && ((row + col) % 3 == 0 || (row * col) % 2 == 0);
            let color = if is_border || is_center || is_data {
                white
            } else {
                blue
            };
            if color == white {
                fill(
                    hdc,
                    b,
                    qr_x + col * cell,
                    qr_y + row * cell,
                    qr_x + (col + 1) * cell,
                    qr_y + (row + 1) * cell,
                    white,
                );
            }
        }
    }

    // Texte à côté du QR code
    let info_x = qr_x + qr_size + 20;
    SelectObject(hdc, body_font);
    TextOutA(
        hdc,
        info_x,
        qr_y,
        c"Pour plus d'informations, recherchez en ligne :".as_ptr(),
        -1,
    );

    // Stop code
    SelectObject(hdc, code_font);
    let mut y2 = qr_y + 24;
    if STOP_LEN > 0 {
        TextOutA(hdc, info_x, y2, STOP_CODE.as_ptr().cast(), STOP_LEN as i32);
        y2 += 20;
    }

    // Informations techniques
    SelectObject(hdc, small_font);
    y2 += 10;
    TextOutA(hdc, info_x, y2, c"Informations techniques :".as_ptr(), -1);
    y2 += 18;
    if EXCEPTION_LEN > 0 {
        let mut line = [0u8; 80];
        let prefix = b"Exception : ";
        line[..prefix.len()].copy_from_slice(prefix);
        let n = EXCEPTION_LEN.min(line.len() - prefix.len());
        line[prefix.len()..prefix.len() + n].copy_from_slice(&EXCEPTION[..n]);
        TextOutA(
            hdc,
            info_x,
            y2,
            line.as_ptr().cast(),
            (prefix.len() + n) as i32,
        );
        y2 += 16;
    }
    if ERROR_CODE_LEN > 0 {
        let mut line = [0u8; 40];
        let prefix = b"Code : ";
        line[..prefix.len()].copy_from_slice(prefix);
        let n = ERROR_CODE_LEN.min(line.len() - prefix.len());
        line[prefix.len()..prefix.len() + n].copy_from_slice(&ERROR_CODE[..n]);
        TextOutA(
            hdc,
            info_x,
            y2,
            line.as_ptr().cast(),
            (prefix.len() + n) as i32,
        );
        y2 += 16;
    }
    if FAULT_ADDR_LEN > 0 {
        let mut line = [0u8; 40];
        let prefix = b"Adresse : ";
        line[..prefix.len()].copy_from_slice(prefix);
        let n = FAULT_ADDR_LEN.min(line.len() - prefix.len());
        line[prefix.len()..prefix.len() + n].copy_from_slice(&FAULT_ADDR[..n]);
        TextOutA(
            hdc,
            info_x,
            y2,
            line.as_ptr().cast(),
            (prefix.len() + n) as i32,
        );
        y2 += 16;
    }
    if EXE_NAME_LEN > 0 {
        let mut line = [0u8; 160];
        let prefix = b"Module : ";
        line[..prefix.len()].copy_from_slice(prefix);
        let n = EXE_NAME_LEN.min(line.len() - prefix.len());
        line[prefix.len()..prefix.len() + n].copy_from_slice(&EXE_NAME[..n]);
        TextOutA(
            hdc,
            info_x,
            y2,
            line.as_ptr().cast(),
            (prefix.len() + n) as i32,
        );
    }

    DeleteObject(title_font);
    DeleteObject(body_font);
    DeleteObject(small_font);
    DeleteObject(code_font);
}

fn fmt_u32(mut n: u32, buf: &mut [u8; 8]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut i = 8;
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let len = 8 - i;
    buf.copy_within(i..8, 0);
    len
}

pub unsafe fn mouse_down() -> bool {
    if !ACTIVE {
        return false;
    }
    // Un clic ferme l'écran bleu
    dismiss();
    true
}

pub unsafe fn key_down() -> bool {
    if !ACTIVE {
        return false;
    }
    dismiss();
    true
}
