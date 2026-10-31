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
/// Compte propriétaire de cette session (son bureau est derrière l'écran de
/// verrouillage). usize::MAX : personne n'est encore connecté sur cet écran.
static mut OWNER: usize = usize::MAX;
/// Session ouverte par « Changer d'utilisateur » depuis un autre bureau.
static mut SPAWNED_SESSION: bool = false;
static mut SESSIONS: [u8; 1024] = [0; 1024];
static mut SESSIONS_LEN: usize = 0;
static mut SESSIONS_POLL: u32 = 0;
static mut SWITCH_NOTICE: u32 = 0;
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
    // Bureau créé par winlux-sessions pour un compte déjà authentifié sur
    // l'écran de verrouillage d'une autre session.
    let mut env = [0u8; 40];
    let n = GetEnvironmentVariableA(
        c"WINLUX_SESSION_USER".as_ptr(),
        env.as_mut_ptr().cast(),
        env.len() as u32,
    ) as usize;
    if n > 0 && n < env.len() {
        if let Some(i) = find_user(&env[..n]) {
            SELECTED_USER = i;
            OWNER = i;
            SPAWNED_SESSION = true;
            LOGGED_IN = true;
        }
    }
}

fn find_user(name: &[u8]) -> Option<usize> {
    unsafe { (0..USER_COUNT).find(|&i| &USERS[i].name[..USERS[i].name_len] == name) }
}

/// Appelle `winlux-control session-<verbe> <compte>`.
unsafe fn session_control(verb: &[u8], user: usize) {
    if user >= USER_COUNT {
        return;
    }
    let mut cmd = [0u8; 64];
    let mut n = 0;
    for part in [b"session-".as_slice(), verb, b" ", &USERS[user].name[..USERS[user].name_len]] {
        cmd[n..n + part.len()].copy_from_slice(part);
        n += part.len();
    }
    if let Ok(c) = core::ffi::CStr::from_bytes_until_nul(&cmd[..n + 1]) {
        crate::settings::control(c);
    }
    SESSIONS_POLL = 0;
}

pub fn user_count() -> usize {
    unsafe { USER_COUNT }
}
pub fn user_name(i: usize) -> &'static [u8] {
    unsafe {
        if i < USER_COUNT {
            &USERS[i].name[..USERS[i].name_len]
        } else {
            b""
        }
    }
}
pub fn owner() -> usize {
    unsafe { OWNER }
}

/// Le compte a-t-il une session ouverte (sur cet écran ou un autre) ?
pub unsafe fn has_session(i: usize) -> bool {
    if i == OWNER {
        return true;
    }
    let now = GetTickCount();
    if SESSIONS_POLL == 0 || now.wrapping_sub(SESSIONS_POLL) > 2000 {
        SESSIONS_POLL = now.max(1);
        let h = CreateFileA(
            c"Z:\\run\\winlux\\sessions.txt".as_ptr(),
            0x8000_0000,
            7,
            core::ptr::null_mut(),
            3,
            0,
            core::ptr::null_mut(),
        );
        SESSIONS_LEN = 0;
        if !h.is_null() && h as isize != -1 {
            let mut read = 0u32;
            ReadFile(
                h,
                SESSIONS.as_mut_ptr(),
                SESSIONS.len() as u32,
                &mut read,
                core::ptr::null_mut(),
            );
            CloseHandle(h);
            SESSIONS_LEN = (read as usize).min(SESSIONS.len());
        }
    }
    let name = user_name(i);
    SESSIONS[..SESSIONS_LEN]
        .split(|&c| c == b'\n')
        .filter_map(|l| l.strip_prefix(b"session="))
        .any(|l| l.split(|&c| c == b'|').next() == Some(name))
}

/// « Changer d'utilisateur » : verrouille ce bureau et propose le compte
/// choisi ; son mot de passe ouvre ou rejoint sa propre session.
pub unsafe fn switch_user(i: usize) {
    lock_screen();
    if i < USER_COUNT {
        SELECTED_USER = i;
    }
}

/// « Se déconnecter » : ferme la session de ce compte. Un bureau secondaire
/// s'arrête ; le bureau principal revient à l'écran de connexion.
pub unsafe fn sign_out() {
    if OWNER == usize::MAX {
        lock_screen();
        return;
    }
    if SPAWNED_SESSION {
        session_control(b"logoff", OWNER);
        return;
    }
    session_control(b"unregister", OWNER);
    OWNER = usize::MAX;
    lock_screen();
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
    if OWNER < USER_COUNT {
        return &USERS[OWNER].name[..USERS[OWNER].name_len];
    }
    if SELECTED_USER < USER_COUNT {
        &USERS[SELECTED_USER].name[..USERS[SELECTED_USER].name_len]
    } else {
        b"root"
    }
}

pub unsafe fn current_user_is_root() -> bool {
    if OWNER < USER_COUNT {
        return USERS[OWNER].is_root;
    }
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
        tr4(&[b"dimanche", b"Sunday", b"domingo", b"Sonntag"]),
        tr4(&[b"lundi", b"Monday", b"lunes", b"Montag"]),
        tr4(&[b"mardi", b"Tuesday", b"martes", b"Dienstag"]),
        tr4(&[b"mercredi", b"Wednesday", b"mi\xE9rcoles", b"Mittwoch"]),
        tr4(&[b"jeudi", b"Thursday", b"jueves", b"Donnerstag"]),
        tr4(&[b"vendredi", b"Friday", b"viernes", b"Freitag"]),
        tr4(&[b"samedi", b"Saturday", b"s\xE1bado", b"Samstag"]),
    ];
    let mois: [&[u8]; 12] = [
        tr4(&[b"janvier", b"January", b"enero", b"Januar"]),
        tr4(&[b"f\xE9vrier", b"February", b"febrero", b"Februar"]),
        tr4(&[b"mars", b"March", b"marzo", b"M\xE4rz"]),
        tr4(&[b"avril", b"April", b"abril", b"April"]),
        tr4(&[b"mai", b"May", b"mayo", b"Mai"]),
        tr4(&[b"juin", b"June", b"junio", b"Juni"]),
        tr4(&[b"juillet", b"July", b"julio", b"Juli"]),
        tr4(&[b"ao\xFBt", b"August", b"agosto", b"August"]),
        tr4(&[b"septembre", b"September", b"septiembre", b"September"]),
        tr4(&[b"octobre", b"October", b"octubre", b"Oktober"]),
        tr4(&[b"novembre", b"November", b"noviembre", b"November"]),
        tr4(&[b"d\xE9cembre", b"December", b"diciembre", b"Dezember"]),
    ];
    let dow = (st.day_of_week as usize) % 7;
    let mon = ((st.month as usize).wrapping_sub(1)) % 12;
    let mut date_buf = [0u8; 48];
    let mut pos = 0usize;
    let jour_nom = jours[dow];
    date_buf[pos..pos + jour_nom.len()].copy_from_slice(jour_nom);
    pos += jour_nom.len();
    // « lundi 3 mars », « Monday, 3 March », « lunes, 3 de marzo », « Montag, 3. März ».
    let lang = ui_lang();
    let sep: &[u8] = if lang == 0 { b" " } else { b", " };
    date_buf[pos..pos + sep.len()].copy_from_slice(sep);
    pos += sep.len();
    if st.day >= 10 {
        date_buf[pos] = b'0' + (st.day / 10) as u8;
        pos += 1;
    }
    date_buf[pos] = b'0' + (st.day % 10) as u8;
    pos += 1;
    let sep = [&b" "[..], b" ", b" de ", b". "][lang];
    date_buf[pos..pos + sep.len()].copy_from_slice(sep);
    pos += sep.len();
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

    if SWITCH_NOTICE != 0 && since(SWITCH_NOTICE) < 4000 {
        text_centered(
            hdc,
            f.small,
            11,
            mix(0, rgb(0xCC, 0xE4, 0xFF), fade_alpha, 1000),
            w / 2,
            h - 90,
            tr4(&[
                b"Ouverture de l'autre session : votre bureau reste verrouill\xE9 et intact.",
                b"Opening the other session: your desktop stays locked and intact.",
                b"Abriendo la otra sesi\xF3n: su escritorio sigue bloqueado e intacto.",
                b"Die andere Sitzung wird ge\xF6ffnet: Ihr Desktop bleibt gesperrt und unver\xE4ndert.",
            ]),
        );
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
            tr4(&[
                b"Mot de passe incorrect",
                b"Incorrect password",
                b"Contrase\xF1a incorrecta",
                b"Falsches Kennwort",
            ]),
        );
    }

    // Bouton "Autres utilisateurs" en bas à gauche (si >1 user).
    if USER_COUNT > 1 {
        let label = tr4(&[
            b"Autres utilisateurs",
            b"Other users",
            b"Otros usuarios",
            b"Andere Benutzer",
        ]);
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
                tr4(&[b"Mot de passe", b"Password", b"Contrase\xF1a", b"Kennwort"]),
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
            tr4(&[
                b"Se connecter",
                b"Sign in",
                b"Iniciar sesi\xF3n",
                b"Anmelden",
            ]),
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
            tr4(&[
                b"Administrateur",
                b"Administrator",
                b"Administrador",
                b"Administrator",
            ]) as &[u8]
        } else {
            tr4(&[b"Utilisateur", b"User", b"Usuario", b"Benutzer"])
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
        // Session déjà ouverte : basculer ne fermera pas son travail.
        if has_session(i) {
            text_centered(
                hdc,
                f.caption,
                12,
                mix(0, rgb(0x7F, 0xE0, 0x9A), alpha, 1000),
                cx,
                ay + avatar_size + 44,
                if i == OWNER {
                    tr4(&[
                        b"Session verrouill\xE9e",
                        b"Session locked",
                        b"Sesi\xF3n bloqueada",
                        b"Sitzung gesperrt",
                    ])
                } else {
                    tr4(&[b"Connect\xE9", b"Signed in", b"Conectado", b"Angemeldet"])
                },
            );
        }
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
                tr4(&[b"Mot de passe", b"Password", b"Contrase\xF1a", b"Kennwort"]),
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
            tr4(&[
                b"Se connecter",
                b"Sign in",
                b"Iniciar sesi\xF3n",
                b"Anmelden",
            ]),
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
        if OWNER != usize::MAX && OWNER != SELECTED_USER {
            // Un autre compte se connecte depuis l'écran verrouillé : sa
            // session s'ouvre à côté, celle-ci reste intacte et verrouillée.
            session_control(b"switch", SELECTED_USER);
            SELECTED_USER = OWNER;
            PASSWORD_LEN = 0;
            SWITCH_NOTICE = now().max(1);
            return;
        }
        if OWNER == usize::MAX {
            OWNER = SELECTED_USER;
            session_control(b"register", OWNER);
        }
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
    LOGIN_FADE_START > 0
        || (PASSWORD_ERROR && since(ERROR_TIME) < 3000)
        || (SWITCH_NOTICE != 0 && since(SWITCH_NOTICE) < 4100)
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
        tr4(&[
            b"Contr\xF4le de compte utilisateur",
            b"User Account Control",
            b"Control de cuentas de usuario",
            b"Benutzerkontensteuerung",
        ]),
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
        tr4(&[
            b"Voulez-vous autoriser cette application",
            b"Do you want to allow this app",
            b"\xBFQuiere permitir que esta aplicaci\xF3n",
            b"M\xF6chten Sie zulassen, dass diese App",
        ]),
    );
    text(
        hdc,
        f.body,
        rgb(0xE6, 0xE6, 0xE6),
        sx + 40,
        sy + 18,
        tr4(&[
            b"\xE0 apporter des modifications ?",
            b"to make changes?",
            b"haga cambios?",
            b"\xC4nderungen vornimmt?",
        ]),
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
        tr4(&[
            b"Mot de passe administrateur (root) :",
            b"Administrator (root) password:",
            b"Contrase\xF1a de administrador (root):",
            b"Administratorkennwort (root):",
        ]),
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
            tr4(&[b"Mot de passe", b"Password", b"Contrase\xF1a", b"Kennwort"]),
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
            tr4(&[
                b"Mot de passe incorrect",
                b"Incorrect password",
                b"Contrase\xF1a incorrecta",
                b"Falsches Kennwort",
            ]),
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
        tr4(&[b"Oui", b"Yes", b"S\xED", b"Ja"]),
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
        tr4(&[b"Non", b"No", b"No", b"Nein"]),
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
