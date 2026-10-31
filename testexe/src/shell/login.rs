//! Écran de connexion Winlux style Windows 11 : affiche les comptes
//! utilisateurs, champ mot de passe et charge le bureau après connexion.
//! Les textes sont en Windows-1252 pour compatibilité `TextOutA`.

use crate::gfx::*;
use winlux_testexe::*;

/// État de l'écran de connexion.
static mut LOGGED_IN: bool = false;
static mut SELECTED_USER: usize = 0;
static mut PASSWORD: [u8; 64] = [0; 64];
static mut PASSWORD_LEN: usize = 0;
static mut PASSWORD_ERROR: bool = false;
static mut ERROR_TIME: u32 = 0;
static mut LOGIN_FADE: i32 = 0;
static mut LOGIN_FADE_START: u32 = 0;
static mut SHOW_PASSWORD: bool = false;
static mut CURSOR_BLINK: u32 = 0;

const MAX_USERS: usize = 8;
static mut USERS: [User; MAX_USERS] = [User::empty(); MAX_USERS];
static mut USER_COUNT: usize = 0;

#[derive(Clone, Copy)]
struct User {
    name: [u8; 32],
    name_len: usize,
    is_root: bool,
    has_password: bool,
}

impl User {
    const fn empty() -> Self {
        User {
            name: [0; 32],
            name_len: 0,
            is_root: false,
            has_password: false,
        }
    }
}

fn now() -> u32 {
    unsafe { GetTickCount() }
}
fn since(t: u32) -> i32 {
    now().wrapping_sub(t).min(1_000_000) as i32
}

/// Initialise les utilisateurs en lisant /etc/winlux/users.conf.
/// Format : une ligne par utilisateur, "nom:root|user:password|nopass".
/// Si le fichier n'existe pas, crée un utilisateur root par défaut.
pub unsafe fn init() {
    if USER_COUNT > 0 {
        return;
    }
    let mut buf = [0u8; 512];
    let handle = CreateFileA(
        c"/etc/winlux/users.conf".as_ptr() as *const i8,
        0x8000_0000, // GENERIC_READ
        1,
        core::ptr::null_mut(),
        3, // OPEN_EXISTING
        0,
        core::ptr::null_mut(),
    );
    if handle as isize > 0 {
        let mut read = 0u32;
        ReadFile(
            handle,
            buf.as_mut_ptr() as _,
            buf.len() as u32,
            &mut read,
            core::ptr::null_mut(),
        );
        CloseHandle(handle);
        let data = &buf[..read as usize];
        for line in data.split(|&c| c == b'\n') {
            if line.is_empty() || USER_COUNT >= MAX_USERS {
                continue;
            }
            let mut parts = line.splitn(3, |&c| c == b':');
            let name = match parts.next() {
                Some(n) if !n.is_empty() => n,
                _ => continue,
            };
            let role = parts.next().unwrap_or(b"user");
            let pass = parts.next().unwrap_or(b"nopass");
            let mut u = User::empty();
            let len = name.len().min(31);
            u.name[..len].copy_from_slice(&name[..len]);
            u.name_len = len;
            u.is_root = role == b"root";
            u.has_password = pass != b"nopass";
            USERS[USER_COUNT] = u;
            USER_COUNT += 1;
        }
    }
    if USER_COUNT == 0 {
        let mut u = User::empty();
        u.name[..4].copy_from_slice(b"root");
        u.name_len = 4;
        u.is_root = true;
        u.has_password = true;
        USERS[0] = u;
        USER_COUNT = 1;
    }
}

pub unsafe fn logged_in() -> bool {
    LOGGED_IN
}

pub unsafe fn lock_screen() {
    LOGGED_IN = false;
    PASSWORD_LEN = 0;
    PASSWORD_ERROR = false;
    SHOW_PASSWORD = false;
}

pub unsafe fn skip_login() {
    LOGGED_IN = true;
}

pub unsafe fn current_user_name() -> &'static [u8] {
    if SELECTED_USER < USER_COUNT {
        &USERS[SELECTED_USER].name[..USERS[SELECTED_USER].name_len]
    } else {
        b"root"
    }
}

pub unsafe fn current_user_is_root() -> bool {
    if SELECTED_USER < USER_COUNT {
        USERS[SELECTED_USER].is_root
    } else {
        true
    }
}

/// Vérifie le mot de passe contre /etc/winlux/users.conf.
unsafe fn check_password() -> bool {
    let user = &USERS[SELECTED_USER];
    if !user.has_password {
        return true;
    }
    let pwd = &PASSWORD[..PASSWORD_LEN];
    let mut buf = [0u8; 512];
    let handle = CreateFileA(
        c"/etc/winlux/users.conf".as_ptr() as *const i8,
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if handle as isize <= 0 {
        return pwd == b"winlux";
    }
    let mut read = 0u32;
    ReadFile(
        handle,
        buf.as_mut_ptr() as _,
        buf.len() as u32,
        &mut read,
        core::ptr::null_mut(),
    );
    CloseHandle(handle);
    let data = &buf[..read as usize];
    for line in data.split(|&c| c == b'\n') {
        let mut parts = line.splitn(3, |&c| c == b':');
        let name = match parts.next() {
            Some(n) => n,
            None => continue,
        };
        if name != &user.name[..user.name_len] {
            continue;
        }
        let _role = parts.next();
        let stored_pass = parts.next().unwrap_or(b"nopass");
        if stored_pass == b"nopass" {
            return true;
        }
        return pwd == stored_pass;
    }
    pwd == b"winlux"
}

/// Dessine l'écran de connexion.
pub unsafe fn draw(hdc: Handle, w: i32, h: i32) {
    let mut b = Brushes::new();
    let f = Fonts::new();
    SetBkMode(hdc, 1);

    // Fondu de sortie vers le bureau.
    if LOGIN_FADE_START > 0 {
        let t = since(LOGIN_FADE_START);
        LOGIN_FADE = ease_out(t, 400);
        if LOGIN_FADE >= 980 {
            LOGGED_IN = true;
            LOGIN_FADE_START = 0;
            LOGIN_FADE = 0;
        }
    }

    let fade_alpha = if LOGIN_FADE_START > 0 {
        1000 - LOGIN_FADE
    } else {
        1000
    };

    // Fond dégradé bleu sombre.
    let bg_top = rgb(0x00, 0x47, 0x8A);
    let bg_bot = rgb(0x00, 0x1E, 0x3D);
    let steps = 16;
    for i in 0..steps {
        let y0 = h * i / steps;
        let y1 = h * (i + 1) / steps;
        let r = mix(bg_top & 0xFF, bg_bot & 0xFF, i as i32, steps as i32);
        let g = mix(
            (bg_top >> 8) & 0xFF,
            (bg_bot >> 8) & 0xFF,
            i as i32,
            steps as i32,
        );
        let bl = mix(
            (bg_top >> 16) & 0xFF,
            (bg_bot >> 16) & 0xFF,
            i as i32,
            steps as i32,
        );
        fill(hdc, &mut b, 0, y0, w, y1, rgb(r, g, bl));
    }

    // Heure au centre haut.
    let mut st: SystemTime = core::mem::zeroed();
    GetLocalTime(&mut st);
    let mut time_buf = [0u8; 6];
    time_buf[0] = b'0' + (st.hour / 10) as u8;
    time_buf[1] = b'0' + (st.hour % 10) as u8;
    time_buf[2] = b':';
    time_buf[3] = b'0' + (st.minute / 10) as u8;
    time_buf[4] = b'0' + (st.minute % 10) as u8;
    text_centered(
        hdc,
        f.display,
        28,
        mix(0, rgb(0xFF, 0xFF, 0xFF), fade_alpha, 1000),
        w / 2,
        h / 6,
        &time_buf[..5],
    );

    // Date sous l'heure.
    let jours: [&[u8]; 7] = [
        b"dimanche",
        b"lundi",
        b"mardi",
        b"mercredi",
        b"jeudi",
        b"vendredi",
        b"samedi",
    ];
    let mois: [&[u8]; 12] = [
        b"janvier",
        b"f\xE9vrier",
        b"mars",
        b"avril",
        b"mai",
        b"juin",
        b"juillet",
        b"ao\xFBt",
        b"septembre",
        b"octobre",
        b"novembre",
        b"d\xE9cembre",
    ];
    let dow = (st.day_of_week as usize) % 7;
    let mon = ((st.month as usize).wrapping_sub(1)) % 12;
    let mut date_buf = [0u8; 48];
    let mut pos = 0usize;
    let jour_nom = jours[dow];
    date_buf[pos..pos + jour_nom.len()].copy_from_slice(jour_nom);
    pos += jour_nom.len();
    date_buf[pos] = b' ';
    pos += 1;
    if st.day >= 10 {
        date_buf[pos] = b'0' + (st.day / 10) as u8;
        pos += 1;
    }
    date_buf[pos] = b'0' + (st.day % 10) as u8;
    pos += 1;
    date_buf[pos] = b' ';
    pos += 1;
    let mois_nom = mois[mon];
    date_buf[pos..pos + mois_nom.len()].copy_from_slice(mois_nom);
    pos += mois_nom.len();
    text_centered(
        hdc,
        f.body,
        13,
        mix(0, rgb(0xCC, 0xCC, 0xCC), fade_alpha, 1000),
        w / 2,
        h / 6 + 36,
        &date_buf[..pos],
    );

    let center_y = h / 2 - 20;

    if USER_COUNT == 1 {
        // Un seul utilisateur : afficher son avatar et champ mot de passe.
        draw_single_user(hdc, &mut b, &f, w, center_y, fade_alpha);
    } else {
        // Plusieurs utilisateurs : grille d'avatars.
        draw_user_grid(hdc, &mut b, &f, w, center_y, fade_alpha);
    }

    // Message d'erreur.
    if PASSWORD_ERROR && since(ERROR_TIME) < 3000 {
        let alpha = if since(ERROR_TIME) > 2500 {
            (3000 - since(ERROR_TIME)) * 1000 / 500
        } else {
            1000
        };
        let err_color = mix(0, rgb(0xFF, 0x66, 0x66), alpha.min(fade_alpha), 1000);
        text_centered(
            hdc,
            f.small,
            11,
            err_color,
            w / 2,
            center_y + 140,
            b"Mot de passe incorrect",
        );
    }

    // Bouton "Autres utilisateurs" en bas à gauche (si >1 user).
    if USER_COUNT > 1 {
        let label = b"Autres utilisateurs";
        text(
            hdc,
            f.small,
            mix(0, rgb(0xAA, 0xAA, 0xAA), fade_alpha, 1000),
            20,
            h - 36,
            label,
        );
    }

    // Boutons d'alimentation en bas à droite.
    draw_power_buttons(hdc, &mut b, &f, w, h, fade_alpha);

    // Fondu noir en sortie.
    if LOGIN_FADE_START > 0 && LOGIN_FADE > 0 {
        fill(hdc, &mut b, 0, 0, w, h, rgb(0, 0, 0));
        // On laisse le fondu se faire visuellement par la prochaine frame.
    }

    f.release();
    b.release();
}

unsafe fn draw_single_user(hdc: Handle, b: &mut Brushes, f: &Fonts, w: i32, cy: i32, alpha: i32) {
    let user = &USERS[SELECTED_USER];
    let name = &user.name[..user.name_len];

    // Avatar (cercle avec initiale).
    let avatar_size = 80;
    let ax = w / 2 - avatar_size / 2;
    let ay = cy - avatar_size / 2 - 30;
    ellipse(
        hdc,
        b,
        ax,
        ay,
        ax + avatar_size,
        ay + avatar_size,
        mix(0, rgb(0x00, 0x78, 0xD4), alpha, 1000),
    );
    let initial = if name.is_empty() {
        b'?'
    } else {
        name[0].to_ascii_uppercase()
    };
    let initial_buf = [initial];
    text_centered(
        hdc,
        f.display,
        28,
        mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
        w / 2,
        ay + avatar_size / 2 - 16,
        &initial_buf,
    );

    // Nom d'utilisateur.
    text_centered(
        hdc,
        f.large,
        22,
        mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
        w / 2,
        ay + avatar_size + 8,
        name,
    );

    // Champ mot de passe.
    if user.has_password {
        let field_w = 280;
        let field_h = 36;
        let fx = w / 2 - field_w / 2;
        let fy = ay + avatar_size + 44;

        let border_color = if PASSWORD_ERROR && since(ERROR_TIME) < 500 {
            mix(0, rgb(0xFF, 0x44, 0x44), alpha, 1000)
        } else {
            mix(0, rgb(0x60, 0x80, 0xA0), alpha, 1000)
        };
        round_rect(
            hdc,
            b,
            fx - 1,
            fy - 1,
            fx + field_w + 1,
            fy + field_h + 1,
            4,
            border_color,
        );
        round_rect(
            hdc,
            b,
            fx,
            fy,
            fx + field_w,
            fy + field_h,
            4,
            mix(0, rgb(0x1A, 0x3A, 0x5C), alpha, 1000),
        );

        // Contenu du champ.
        if PASSWORD_LEN > 0 {
            if SHOW_PASSWORD {
                text(
                    hdc,
                    f.body,
                    mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
                    fx + 12,
                    fy + 10,
                    &PASSWORD[..PASSWORD_LEN],
                );
            } else {
                let mut dots = [0xB7u8; 64]; // middle dot en Windows-1252
                text(
                    hdc,
                    f.body,
                    mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
                    fx + 12,
                    fy + 10,
                    &dots[..PASSWORD_LEN],
                );
            }
        } else {
            text(
                hdc,
                f.caption,
                mix(0, rgb(0x80, 0x90, 0xA0), alpha, 1000),
                fx + 12,
                fy + 11,
                b"Mot de passe",
            );
        }

        // Curseur clignotant.
        if (now() / 530) % 2 == 0 {
            let cursor_x = if PASSWORD_LEN > 0 {
                fx + 12
                    + text_width(
                        if SHOW_PASSWORD {
                            &PASSWORD[..PASSWORD_LEN]
                        } else {
                            &[0xB7; 1]
                        },
                        13,
                    ) * if SHOW_PASSWORD {
                        1
                    } else {
                        PASSWORD_LEN as i32
                    }
            } else {
                fx + 12
            };
            fill(
                hdc,
                b,
                cursor_x,
                fy + 8,
                cursor_x + crate::gfx::caret_w(1),
                fy + field_h - 8,
                mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
            );
        }

        // Bouton connexion (flèche ->).
        let btn_x = fx + field_w + 8;
        let btn_y = fy;
        round_rect(
            hdc,
            b,
            btn_x,
            btn_y,
            btn_x + field_h,
            btn_y + field_h,
            4,
            mix(0, rgb(0x00, 0x78, 0xD4), alpha, 1000),
        );
        text_centered(
            hdc,
            f.semibold,
            13,
            mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
            btn_x + field_h / 2,
            btn_y + 10,
            b"\x1A",
        );
    } else {
        // Pas de mot de passe : bouton "Se connecter".
        let btn_w = 160;
        let btn_h = 36;
        let bx = w / 2 - btn_w / 2;
        let by = ay + avatar_size + 44;
        round_rect(
            hdc,
            b,
            bx,
            by,
            bx + btn_w,
            by + btn_h,
            4,
            mix(0, rgb(0x00, 0x78, 0xD4), alpha, 1000),
        );
        text_centered(
            hdc,
            f.semibold,
            13,
            mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
            w / 2,
            by + 10,
            b"Se connecter",
        );
    }
}

unsafe fn draw_user_grid(hdc: Handle, b: &mut Brushes, f: &Fonts, w: i32, cy: i32, alpha: i32) {
    let count = USER_COUNT.min(MAX_USERS);
    let card_w = 120;
    let card_h = 130;
    let gap = 20;
    let total_w = count as i32 * card_w + (count as i32 - 1) * gap;
    let start_x = w / 2 - total_w / 2;

    for i in 0..count {
        let user = &USERS[i];
        let name = &user.name[..user.name_len];
        let cx = start_x + i as i32 * (card_w + gap) + card_w / 2;

        let selected = i == SELECTED_USER;
        if selected {
            round_rect(
                hdc,
                b,
                cx - card_w / 2 - 4,
                cy - 4,
                cx + card_w / 2 + 4,
                cy + card_h + 4,
                8,
                mix(0, rgb(0xFF, 0xFF, 0xFF), alpha / 6, 1000),
            );
        }

        // Avatar.
        let avatar_size = 56;
        let ax = cx - avatar_size / 2;
        let ay = cy + 10;
        let avatar_bg = if user.is_root {
            rgb(0xD4, 0x50, 0x00)
        } else {
            rgb(0x00, 0x78, 0xD4)
        };
        ellipse(
            hdc,
            b,
            ax,
            ay,
            ax + avatar_size,
            ay + avatar_size,
            mix(0, avatar_bg, alpha, 1000),
        );
        let initial = if name.is_empty() {
            b'?'
        } else {
            name[0].to_ascii_uppercase()
        };
        let initial_buf = [initial];
        text_centered(
            hdc,
            f.large,
            22,
            mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
            cx,
            ay + avatar_size / 2 - 12,
            &initial_buf,
        );

        // Nom.
        text_centered(
            hdc,
            f.body,
            13,
            mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
            cx,
            ay + avatar_size + 10,
            name,
        );

        // Rôle.
        let role = if user.is_root {
            b"Administrateur" as &[u8]
        } else {
            b"Utilisateur"
        };
        text_centered(
            hdc,
            f.caption,
            12,
            mix(0, rgb(0x99, 0xBB, 0xDD), alpha, 1000),
            cx,
            ay + avatar_size + 28,
            role,
        );
    }

    // Champ mot de passe sous la grille si l'utilisateur sélectionné en a un.
    let user = &USERS[SELECTED_USER];
    if user.has_password {
        let field_w = 280;
        let field_h = 36;
        let fx = w / 2 - field_w / 2;
        let fy = cy + card_h + 24;

        let border_color = if PASSWORD_ERROR && since(ERROR_TIME) < 500 {
            mix(0, rgb(0xFF, 0x44, 0x44), alpha, 1000)
        } else {
            mix(0, rgb(0x60, 0x80, 0xA0), alpha, 1000)
        };
        round_rect(
            hdc,
            b,
            fx - 1,
            fy - 1,
            fx + field_w + 1,
            fy + field_h + 1,
            4,
            border_color,
        );
        round_rect(
            hdc,
            b,
            fx,
            fy,
            fx + field_w,
            fy + field_h,
            4,
            mix(0, rgb(0x1A, 0x3A, 0x5C), alpha, 1000),
        );

        if PASSWORD_LEN > 0 {
            let mut dots = [0xB7u8; 64];
            text(
                hdc,
                f.body,
                mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
                fx + 12,
                fy + 10,
                &dots[..PASSWORD_LEN],
            );
        } else {
            text(
                hdc,
                f.caption,
                mix(0, rgb(0x80, 0x90, 0xA0), alpha, 1000),
                fx + 12,
                fy + 11,
                b"Mot de passe",
            );
        }

        // Curseur.
        if (now() / 530) % 2 == 0 {
            let cursor_x = if PASSWORD_LEN > 0 {
                fx + 12 + text_width(&[0xB7], 13) * PASSWORD_LEN as i32
            } else {
                fx + 12
            };
            fill(
                hdc,
                b,
                cursor_x,
                fy + 8,
                cursor_x + crate::gfx::caret_w(1),
                fy + field_h - 8,
                mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
            );
        }

        // Bouton connexion.
        let btn_x = fx + field_w + 8;
        round_rect(
            hdc,
            b,
            btn_x,
            fy,
            btn_x + field_h,
            fy + field_h,
            4,
            mix(0, rgb(0x00, 0x78, 0xD4), alpha, 1000),
        );
        text_centered(
            hdc,
            f.semibold,
            13,
            mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
            btn_x + field_h / 2,
            fy + 10,
            b"\x1A",
        );
    } else {
        let btn_w = 160;
        let btn_h = 36;
        let bx = w / 2 - btn_w / 2;
        let by = cy + card_h + 24;
        round_rect(
            hdc,
            b,
            bx,
            by,
            bx + btn_w,
            by + btn_h,
            4,
            mix(0, rgb(0x00, 0x78, 0xD4), alpha, 1000),
        );
        text_centered(
            hdc,
            f.semibold,
            13,
            mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
            w / 2,
            by + 10,
            b"Se connecter",
        );
    }
}

unsafe fn draw_power_buttons(hdc: Handle, b: &mut Brushes, f: &Fonts, w: i32, h: i32, alpha: i32) {
    let btn_size = 32;
    let gap = 8;
    let bx = w - btn_size * 2 - gap - 20;
    let by = h - btn_size - 12;

    // Bouton arrêt.
    ellipse(
        hdc,
        b,
        bx,
        by,
        bx + btn_size,
        by + btn_size,
        mix(0, rgb(0x40, 0x50, 0x60), alpha, 1000),
    );
    // Icône power (cercle + barre).
    let cx_p = bx + btn_size / 2;
    let cy_p = by + btn_size / 2;
    let r = 8;
    fill(
        hdc,
        b,
        cx_p - 1,
        cy_p - r - 2,
        cx_p + 1,
        cy_p - 2,
        mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
    );
    for angle_step in 0..12 {
        let a = (angle_step * 30 + 60) as f64 * core::f64::consts::PI / 180.0;
        // Approximation sans libm : 8 points sur le cercle.
        let (sin_a, cos_a) = approx_sincos(angle_step * 30 + 60);
        let px = cx_p + (sin_a * r as i32) / 1000;
        let py = cy_p - (cos_a * r as i32) / 1000;
        fill(
            hdc,
            b,
            px,
            py,
            px + 2,
            py + 2,
            mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
        );
    }

    // Bouton redémarrage.
    let rx = bx + btn_size + gap;
    ellipse(
        hdc,
        b,
        rx,
        by,
        rx + btn_size,
        by + btn_size,
        mix(0, rgb(0x40, 0x50, 0x60), alpha, 1000),
    );
    // Icône refresh (flèche circulaire simplifiée).
    let cx_r = rx + btn_size / 2;
    let cy_r = by + btn_size / 2;
    for angle_step in 0..10 {
        let (sin_a, cos_a) = approx_sincos(angle_step * 36);
        let px = cx_r + (sin_a * 7) / 1000;
        let py = cy_r - (cos_a * 7) / 1000;
        fill(
            hdc,
            b,
            px,
            py,
            px + 2,
            py + 2,
            mix(0, rgb(0xFF, 0xFF, 0xFF), alpha, 1000),
        );
    }
}

/// Approximation sin/cos en millièmes (pas de libm en no_std).
pub fn approx_sincos(deg: i32) -> (i32, i32) {
    let table: [(i32, i32); 13] = [
        (0, 1000),
        (174, 985),
        (342, 940),
        (500, 866),
        (643, 766),
        (766, 643),
        (866, 500),
        (940, 342),
        (985, 174),
        (1000, 0),
        (985, -174),
        (940, -342),
        (866, -500),
    ];
    let d = ((deg % 360) + 360) % 360;
    let (sin_val, cos_val) = if d <= 90 {
        let idx = (d * 12 / 90).min(12) as usize;
        table[idx]
    } else if d <= 180 {
        let idx = ((180 - d) * 12 / 90).min(12) as usize;
        (table[idx].0, -table[idx].1)
    } else if d <= 270 {
        let idx = ((d - 180) * 12 / 90).min(12) as usize;
        (-table[idx].0, -table[idx].1)
    } else {
        let idx = ((360 - d) * 12 / 90).min(12) as usize;
        (-table[idx].0, table[idx].1)
    };
    (sin_val, cos_val)
}

/// Tentative de connexion.
unsafe fn try_login() {
    if check_password() {
        PASSWORD_ERROR = false;
        LOGIN_FADE_START = now();
    } else {
        PASSWORD_ERROR = true;
        ERROR_TIME = now();
        PASSWORD_LEN = 0;
    }
}

/// Clic sur l'écran de connexion.
pub unsafe fn on_click(hwnd: Handle, x: i32, y: i32, w: i32, h: i32) {
    if LOGIN_FADE_START > 0 {
        return;
    }

    let center_y = h / 2 - 20;

    // Grille d'utilisateurs : clic sur un avatar.
    if USER_COUNT > 1 {
        let card_w = 120;
        let card_h = 130;
        let gap = 20;
        let count = USER_COUNT.min(MAX_USERS);
        let total_w = count as i32 * card_w + (count as i32 - 1) * gap;
        let start_x = w / 2 - total_w / 2;
        for i in 0..count {
            let cx_card = start_x + i as i32 * (card_w + gap);
            if x >= cx_card && x < cx_card + card_w && y >= center_y && y < center_y + card_h {
                if SELECTED_USER != i {
                    SELECTED_USER = i;
                    PASSWORD_LEN = 0;
                    PASSWORD_ERROR = false;
                }
                InvalidateRect(hwnd, core::ptr::null(), 0);
                return;
            }
        }
    }

    let user = &USERS[SELECTED_USER];
    if user.has_password {
        // Clic sur le bouton de connexion.
        let field_w = 280;
        let field_h = 36;
        let fx = w / 2 - field_w / 2;
        let fy = if USER_COUNT == 1 {
            let avatar_size = 80;
            let ay = center_y - avatar_size / 2 - 30;
            ay + avatar_size + 44
        } else {
            center_y + 130 + 24
        };
        let btn_x = fx + field_w + 8;
        if x >= btn_x && x < btn_x + field_h && y >= fy && y < fy + field_h {
            try_login();
            InvalidateRect(hwnd, core::ptr::null(), 0);
            return;
        }
    } else {
        // Clic sur "Se connecter".
        let btn_w = 160;
        let btn_h = 36;
        let bx = w / 2 - btn_w / 2;
        let by = if USER_COUNT == 1 {
            let avatar_size = 80;
            let ay = center_y - avatar_size / 2 - 30;
            ay + avatar_size + 44
        } else {
            center_y + 130 + 24
        };
        if x >= bx && x < bx + btn_w && y >= by && y < by + btn_h {
            try_login();
            InvalidateRect(hwnd, core::ptr::null(), 0);
            return;
        }
    }

    // Boutons d'alimentation.
    let btn_size = 32;
    let gap = 8;
    let bx = w - btn_size * 2 - gap - 20;
    let by = h - btn_size - 12;
    if x >= bx && x < bx + btn_size && y >= by && y < by + btn_size {
        crate::settings::control(c"poweroff");
        return;
    }
    if x >= bx + btn_size + gap && x < bx + btn_size * 2 + gap && y >= by && y < by + btn_size {
        crate::settings::control(c"reboot");
        return;
    }
}

/// Saisie clavier sur l'écran de connexion.
pub unsafe fn on_key(hwnd: Handle, vk: u32, ch: u16) {
    if LOGIN_FADE_START > 0 {
        return;
    }
    // Enter = valider.
    if vk == 0x0D || ch == 13 {
        try_login();
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    // Backspace.
    if vk == 0x08 || ch == 8 {
        if PASSWORD_LEN > 0 {
            PASSWORD_LEN -= 1;
            PASSWORD_ERROR = false;
        }
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    // Escape : effacer le champ.
    if vk == 0x1B {
        PASSWORD_LEN = 0;
        PASSWORD_ERROR = false;
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    // Tab : changer d'utilisateur.
    if vk == 0x09 && USER_COUNT > 1 {
        SELECTED_USER = (SELECTED_USER + 1) % USER_COUNT;
        PASSWORD_LEN = 0;
        PASSWORD_ERROR = false;
        InvalidateRect(hwnd, core::ptr::null(), 0);
        return;
    }
    // Caractère imprimable.
    if ch >= 32 && ch < 127 && PASSWORD_LEN < 63 {
        PASSWORD[PASSWORD_LEN] = ch as u8;
        PASSWORD_LEN += 1;
        PASSWORD_ERROR = false;
        InvalidateRect(hwnd, core::ptr::null(), 0);
    }
}

/// Animation de l'écran de connexion.
pub unsafe fn animating() -> bool {
    LOGIN_FADE_START > 0 || (PASSWORD_ERROR && since(ERROR_TIME) < 3000)
}

// --- Popup d'élévation de privilèges (style UAC) ---

static mut UAC_VISIBLE: bool = false;
static mut UAC_PASSWORD: [u8; 64] = [0; 64];
static mut UAC_PASSWORD_LEN: usize = 0;
static mut UAC_ERROR: bool = false;
static mut UAC_ERROR_TIME: u32 = 0;
static mut UAC_APP_NAME: [u8; 64] = [0; 64];
static mut UAC_APP_NAME_LEN: usize = 0;
static mut UAC_RESULT: i32 = 0; // 0=pending, 1=granted, -1=denied

pub unsafe fn uac_prompt(app_name: &[u8]) {
    UAC_VISIBLE = true;
    UAC_PASSWORD_LEN = 0;
    UAC_ERROR = false;
    UAC_RESULT = 0;
    let len = app_name.len().min(63);
    UAC_APP_NAME[..len].copy_from_slice(&app_name[..len]);
    UAC_APP_NAME_LEN = len;
}

pub unsafe fn uac_visible() -> bool {
    UAC_VISIBLE
}

pub unsafe fn uac_result() -> i32 {
    UAC_RESULT
}

pub unsafe fn uac_close() {
    UAC_VISIBLE = false;
    UAC_RESULT = 0;
}

unsafe fn check_root_password(pwd: &[u8]) -> bool {
    let mut buf = [0u8; 512];
    let handle = CreateFileA(
        c"/etc/winlux/users.conf".as_ptr() as *const i8,
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if handle as isize <= 0 {
        return pwd == b"winlux";
    }
    let mut read = 0u32;
    ReadFile(
        handle,
        buf.as_mut_ptr() as _,
        buf.len() as u32,
        &mut read,
        core::ptr::null_mut(),
    );
    CloseHandle(handle);
    let data = &buf[..read as usize];
    for line in data.split(|&c| c == b'\n') {
        let mut parts = line.splitn(3, |&c| c == b':');
        let name = match parts.next() {
            Some(n) => n,
            None => continue,
        };
        let role = parts.next().unwrap_or(b"user");
        if role != b"root" {
            continue;
        }
        let stored_pass = parts.next().unwrap_or(b"nopass");
        if stored_pass == b"nopass" {
            return true;
        }
        return pwd == stored_pass;
    }
    pwd == b"winlux"
}

pub unsafe fn draw_uac(hdc: Handle, w: i32, h: i32) {
    if !UAC_VISIBLE {
        return;
    }
    let mut b = Brushes::new();
    let f = Fonts::new();
    SetBkMode(hdc, 1);

    // Fond sombre semi-transparent (simulé par un fond uni sombre).
    fill(hdc, &mut b, 0, 0, w, h, rgb(0x10, 0x10, 0x18));

    // Boîte de dialogue UAC.
    let dw = 420;
    let dh = 260;
    let dx = w / 2 - dw / 2;
    let dy = h / 2 - dh / 2;
    panel(
        hdc,
        &mut b,
        dx,
        dy,
        dx + dw,
        dy + dh,
        8,
        rgb(0x2C, 0x2C, 0x2C),
        rgb(0x50, 0x50, 0x50),
    );

    // Barre jaune UAC en haut.
    fill(hdc, &mut b, dx, dy, dx + dw, dy + 4, rgb(0xFF, 0xB9, 0x00));

    // Titre.
    text_centered(
        hdc,
        f.semibold,
        13,
        rgb(0xFF, 0xFF, 0xFF),
        w / 2,
        dy + 20,
        b"Contr\xF4le de compte utilisateur",
    );

    // Icône bouclier (triangle simplifié).
    let sx = dx + 30;
    let sy = dy + 54;
    fill(hdc, &mut b, sx, sy, sx + 28, sy + 28, rgb(0xFF, 0xB9, 0x00));
    fill(
        hdc,
        &mut b,
        sx + 4,
        sy + 4,
        sx + 24,
        sy + 24,
        rgb(0x2C, 0x2C, 0x2C),
    );
    text(hdc, f.icon, rgb(0xFF, 0xB9, 0x00), sx + 8, sy + 6, b"!");

    // Message.
    text(
        hdc,
        f.body,
        rgb(0xE6, 0xE6, 0xE6),
        sx + 40,
        sy + 2,
        b"Voulez-vous autoriser cette application",
    );
    text(
        hdc,
        f.body,
        rgb(0xE6, 0xE6, 0xE6),
        sx + 40,
        sy + 18,
        b"\xE0 apporter des modifications ?",
    );

    // Nom de l'application.
    text(
        hdc,
        f.semibold,
        rgb(0xFF, 0xFF, 0xFF),
        dx + 30,
        dy + 100,
        &UAC_APP_NAME[..UAC_APP_NAME_LEN],
    );

    // Champ mot de passe root.
    text(
        hdc,
        f.caption,
        rgb(0x99, 0x99, 0x99),
        dx + 30,
        dy + 130,
        b"Mot de passe administrateur (root) :",
    );
    let fx = dx + 30;
    let fy = dy + 148;
    let field_w = dw - 60;
    let field_h = 32;
    let border_color = if UAC_ERROR && since(UAC_ERROR_TIME) < 500 {
        rgb(0xFF, 0x44, 0x44)
    } else {
        rgb(0x50, 0x60, 0x70)
    };
    round_rect(
        hdc,
        &mut b,
        fx - 1,
        fy - 1,
        fx + field_w + 1,
        fy + field_h + 1,
        4,
        border_color,
    );
    round_rect(
        hdc,
        &mut b,
        fx,
        fy,
        fx + field_w,
        fy + field_h,
        4,
        rgb(0x1A, 0x1A, 0x2A),
    );

    if UAC_PASSWORD_LEN > 0 {
        let mut dots = [0xB7u8; 64];
        text(
            hdc,
            f.body,
            rgb(0xFF, 0xFF, 0xFF),
            fx + 8,
            fy + 8,
            &dots[..UAC_PASSWORD_LEN],
        );
    } else {
        text(
            hdc,
            f.caption,
            rgb(0x60, 0x70, 0x80),
            fx + 8,
            fy + 9,
            b"Mot de passe",
        );
    }

    // Erreur.
    if UAC_ERROR && since(UAC_ERROR_TIME) < 3000 {
        text(
            hdc,
            f.small,
            rgb(0xFF, 0x66, 0x66),
            dx + 30,
            dy + 186,
            b"Mot de passe incorrect",
        );
    }

    // Boutons Oui / Non.
    let btn_w = 80;
    let btn_h = 30;
    let btn_y = dy + dh - btn_h - 16;
    // Oui
    round_rect(
        hdc,
        &mut b,
        dx + dw - btn_w * 2 - 24,
        btn_y,
        dx + dw - btn_w - 16,
        btn_y + btn_h,
        4,
        rgb(0x00, 0x78, 0xD4),
    );
    text_centered(
        hdc,
        f.semibold,
        13,
        rgb(0xFF, 0xFF, 0xFF),
        dx + dw - btn_w - 16 - btn_w / 2 - 4,
        btn_y + 7,
        b"Oui",
    );
    // Non
    round_rect(
        hdc,
        &mut b,
        dx + dw - btn_w - 8,
        btn_y,
        dx + dw - 8,
        btn_y + btn_h,
        4,
        rgb(0x40, 0x44, 0x4C),
    );
    text_centered(
        hdc,
        f.semibold,
        13,
        rgb(0xE6, 0xE6, 0xE6),
        dx + dw - 8 - btn_w / 2,
        btn_y + 7,
        b"Non",
    );

    f.release();
    b.release();
}

pub unsafe fn uac_on_click(x: i32, y: i32, w: i32, h: i32) {
    if !UAC_VISIBLE {
        return;
    }
    let dw = 420;
    let dh = 260;
    let dx = w / 2 - dw / 2;
    let dy = h / 2 - dh / 2;
    let btn_w = 80;
    let btn_h = 30;
    let btn_y = dy + dh - btn_h - 16;

    // Bouton Oui.
    if x >= dx + dw - btn_w * 2 - 24 && x < dx + dw - btn_w - 16 && y >= btn_y && y < btn_y + btn_h
    {
        if check_root_password(&UAC_PASSWORD[..UAC_PASSWORD_LEN]) {
            UAC_RESULT = 1;
            UAC_VISIBLE = false;
        } else {
            UAC_ERROR = true;
            UAC_ERROR_TIME = now();
            UAC_PASSWORD_LEN = 0;
        }
        return;
    }
    // Bouton Non.
    if x >= dx + dw - btn_w - 8 && x < dx + dw - 8 && y >= btn_y && y < btn_y + btn_h {
        UAC_RESULT = -1;
        UAC_VISIBLE = false;
        return;
    }
}

pub unsafe fn uac_on_key(vk: u32, ch: u16) {
    if !UAC_VISIBLE {
        return;
    }
    if vk == 0x0D || ch == 13 {
        if check_root_password(&UAC_PASSWORD[..UAC_PASSWORD_LEN]) {
            UAC_RESULT = 1;
            UAC_VISIBLE = false;
        } else {
            UAC_ERROR = true;
            UAC_ERROR_TIME = now();
            UAC_PASSWORD_LEN = 0;
        }
        return;
    }
    if vk == 0x1B {
        UAC_RESULT = -1;
        UAC_VISIBLE = false;
        return;
    }
    if vk == 0x08 || ch == 8 {
        if UAC_PASSWORD_LEN > 0 {
            UAC_PASSWORD_LEN -= 1;
            UAC_ERROR = false;
        }
        return;
    }
    if ch >= 32 && ch < 127 && UAC_PASSWORD_LEN < 63 {
        UAC_PASSWORD[UAC_PASSWORD_LEN] = ch as u8;
        UAC_PASSWORD_LEN += 1;
        UAC_ERROR = false;
    }
}
