//! winlux-fond-anime : fond « Winlux Aurore » animé, léger (#3079).
//!
//! Une couche transparente posée au-dessus du bureau (fenêtre de type
//! « bureau », sans entrée souris : les clics vont à xfdesktop) fait
//! scintiller les étoiles les plus brillantes de l'image, exactement là où
//! `render-brand.py` les a dessinées, et fait passer une étoile filante de
//! temps en temps. Seuls de petits rectangles sont repeints ; le reste de
//! l'écran ne bouge jamais.
//!
//! Rien n'est dessiné, et le programme ne se réveille plus à chaque image,
//! quand :
//! - une fenêtre est en plein écran ou agrandie sur l'écran concerné ;
//! - l'économiseur d'écran est actif ou l'écran est éteint (DPMS) ;
//! - la batterie est faible (20 % ou moins, sur batterie).
//!
//! Le programme s'arrête tout seul quand l'animation est désactivée dans
//! Paramètres, quand aucun compositeur ne tourne (une couche transparente
//! serait noire) ou quand le fond affiché n'est plus « Winlux Aurore ».
//!
//! ```text
//! winlux-fond-anime status    état : enabled=on|off running=yes|no reason=…
//! winlux-fond-anime enable    active l'animation et la lance si elle a lieu d'être
//! winlux-fond-anime disable   désactive et arrête l'animation
//! winlux-fond-anime ensure    lance ou arrête selon le réglage et le fond (session, Paramètres)
//! winlux-fond-anime run       tourne au premier plan
//! ```
//!
//! Réglage : `~/.config/winlux/fond-anime.conf` (`ANIMATION=on|off`, désactivé
//! par défaut). Une seule instance par écran X : elle possède la sélection
//! `_WINLUX_FOND_ANIME_S<n>` ; la reprendre l'arrête (même protocole que les
//! gestionnaires de fenêtres, ICCCM § 2.8).
//!
//! Variables pour les tests : `WINLUX_FOND_ANIME_ETOILES` (fichier des
//! étoiles), `WINLUX_FOND_ANIME_POWER` (dossier remplaçant
//! /sys/class/power_supply), `WINLUX_FOND_ANIME_XFCONF` (sortie de
//! `xfconf-query -c xfce4-desktop -l -v` lue dans un fichier).

use std::collections::HashSet;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::*;
use x11rb::protocol::{dpms, randr, screensaver, shape, xfixes, Event};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::CURRENT_TIME;

use dpms::ConnectionExt as _;
use randr::ConnectionExt as _;
use screensaver::ConnectionExt as _;
use shape::ConnectionExt as _;
use xfixes::ConnectionExt as _;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

const STARS_FILE: &str = "/usr/share/winlux/wallpapers/winlux-prisme-aurore.etoiles";
/// Éclat (40 à 150 dans l'image) à partir duquel une étoile scintille.
const TWINKLE_MIN_GLOW: f64 = 125.0;
/// Cadence du scintillement et de l'étoile filante.
const TWINKLE_TICK: Duration = Duration::from_millis(250);
const METEOR_TICK: Duration = Duration::from_millis(33);
const METEOR_SECONDS: f64 = 1.1;
/// Contrôles lents : écran éteint, batterie.
const DPMS_EVERY: Duration = Duration::from_secs(10);
const BATTERY_EVERY: Duration = Duration::from_secs(60);
const BATTERY_LOW: u32 = 20;
const LEVELS: u8 = 6;

// ── Réglage ────────────────────────────────────────────────────────────────

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("winlux").join("fond-anime.conf")
}

fn enabled() -> bool {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|t| {
            t.lines()
                .rev()
                .find_map(|l| l.trim().strip_prefix("ANIMATION="))
                .map(|v| v.trim() == "on")
        })
        .unwrap_or(false)
}

fn set_enabled(on: bool) -> Res<()> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        &path,
        format!("ANIMATION={}\n", if on { "on" } else { "off" }),
    )?;
    Ok(())
}

// ── Étoiles et fond affiché ────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Star {
    u: f64,
    v: f64,
    period: f64,
    phase: f64,
}

/// Petit générateur déterministe (xorshift) : mêmes périodes à chaque session.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn load_stars() -> Vec<Star> {
    let path = std::env::var("WINLUX_FOND_ANIME_ETOILES").unwrap_or_else(|_| STARS_FILE.into());
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut stars = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<f64> = line
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if f.len() != 3 || line.starts_with("empreinte") || f[2] < TWINKLE_MIN_GLOW {
            continue;
        }
        let (u, v) = (f[0], f[1]);
        // Le logo Prisme (centre 0,5 × 0,36, haut de 16 % de l'image) couvre
        // les étoiles tirées derrière lui : elles ne scintillent pas.
        if ((u - 0.5) * 16.0 / 9.0).abs() < 0.11 && (v - 0.36).abs() < 0.11 {
            continue;
        }
        stars.push(Star {
            u,
            v,
            period: 8.0 + 8.0 * rng.next(),
            phase: rng.next(),
        });
    }
    stars
}

fn xfconf_desktop() -> String {
    if let Ok(path) = std::env::var("WINLUX_FOND_ANIME_XFCONF") {
        return std::fs::read_to_string(path).unwrap_or_default();
    }
    Command::new("xfconf-query")
        .args(["-c", "xfce4-desktop", "-l", "-v"])
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Empreinte de l'image Aurore sombre : (x, y relatifs, r, g, b) moyens,
/// lignes « empreinte » du fichier des étoiles (render-brand.py).
fn load_fingerprint() -> Vec<[f64; 5]> {
    let path = std::env::var("WINLUX_FOND_ANIME_ETOILES").unwrap_or_else(|_| STARS_FILE.into());
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("empreinte"))
        .filter_map(|l| {
            let f: Vec<f64> = l
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            (f.len() == 5).then(|| [f[0], f[1], f[2], f[3], f[4]])
        })
        .collect()
}

/// Image décodée en RGB 8 bits, ligne du haut d'abord.
struct Picture {
    w: usize,
    h: usize,
    rgb: Vec<u8>,
}

impl Picture {
    /// BMP non compressé 24 ou 32 bits : celui que pose l'installation
    /// (/usr/share/winlux/wallpaper.bmp, tiré d'Aurore par build-live-iso.sh).
    fn bmp(data: &[u8]) -> Option<Picture> {
        let u16_at = |o: usize| data.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |o: usize| {
            data.get(o..o + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        if data.get(0..2)? != b"BM" {
            return None;
        }
        let offset = u32_at(10)? as usize;
        let w = u32_at(18)? as i32;
        let raw_h = u32_at(22)? as i32;
        let bpp = u16_at(28)? as usize;
        let compression = u32_at(30)?;
        if w <= 0
            || raw_h == 0
            || !(bpp == 24 || bpp == 32)
            || !(compression == 0 || compression == 3)
        {
            return None;
        }
        let (w, h) = (w as usize, raw_h.unsigned_abs() as usize);
        let stride = (w * bpp).div_ceil(32) * 4;
        let mut rgb = vec![0u8; w * h * 3];
        for y in 0..h {
            let src_y = if raw_h > 0 { h - 1 - y } else { y };
            let row = data.get(offset + src_y * stride..offset + src_y * stride + w * bpp / 8)?;
            for x in 0..w {
                let p = &row[x * bpp / 8..];
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[p[2], p[1], p[0]]);
            }
        }
        Some(Picture { w, h, rgb })
    }

    fn png(data: &[u8]) -> Option<Picture> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().ok()?;
        let mut buf = vec![0u8; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            _ => return None,
        };
        let (w, h) = (info.width as usize, info.height as usize);
        let mut rgb = vec![0u8; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let p = &buf[y * info.line_size + x * channels..];
                let (r, g, b) = if channels >= 3 {
                    (p[0], p[1], p[2])
                } else {
                    (p[0], p[0], p[0])
                };
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[r, g, b]);
            }
        }
        Some(Picture { w, h, rgb })
    }

    fn open(path: &str) -> Option<Picture> {
        let data = std::fs::read(path).ok()?;
        if data.starts_with(b"BM") {
            Picture::bmp(&data)
        } else if data.starts_with(b"\x89PNG") {
            Picture::png(&data)
        } else {
            None
        }
    }

    /// Couleur moyenne d'un carré de 1 % de la hauteur autour de (u, v).
    fn mean(&self, u: f64, v: f64) -> [f64; 3] {
        let half = ((self.h as f64 * 0.005).round() as i64).max(2);
        let cx = (u * (self.w - 1) as f64).round() as i64;
        let cy = (v * (self.h - 1) as f64).round() as i64;
        let mut sum = [0f64; 3];
        let mut n = 0f64;
        for y in (cy - half).max(0)..=(cy + half).min(self.h as i64 - 1) {
            for x in (cx - half).max(0)..=(cx + half).min(self.w as i64 - 1) {
                let k = (y as usize * self.w + x as usize) * 3;
                for (c, total) in sum.iter_mut().enumerate() {
                    *total += self.rgb[k + c] as f64;
                }
                n += 1.0;
            }
        }
        sum.map(|s| s / n.max(1.0))
    }
}

/// L'image ressemble-t-elle à Aurore sombre ? Écart moyen de 6 au plus sur
/// 0-255 et aucun point à plus de 20 : « Winlux Nuit », la plus proche,
/// est à 22 de moyenne, une définition 1440p ou 4K d'Aurore à moins de 0,2.
fn matches_fingerprint(picture: &Picture, fingerprint: &[[f64; 5]]) -> bool {
    if fingerprint.is_empty() {
        return false;
    }
    let mut total = 0.0;
    for f in fingerprint {
        let m = picture.mean(f[0], f[1]);
        let d = (0..3).map(|c| (m[c] - f[2 + c]).abs()).sum::<f64>() / 3.0;
        if d > 20.0 {
            return false;
        }
        total += d;
    }
    total / fingerprint.len() as f64 <= 6.0
}

/// « Winlux Aurore » sombre, quelle que soit sa définition ; pas la claire,
/// qui n'a pas d'étoiles. Reconnue par son nom, sinon par son contenu :
/// l'installation pose une copie au nom neutre, /usr/share/winlux/wallpaper.bmp.
fn is_aurora(image: &str) -> bool {
    let name = image.rsplit('/').next().unwrap_or("");
    if name.starts_with("winlux-prisme-aurore") {
        return !name.starts_with("winlux-prisme-aurore-clair");
    }
    Picture::open(image).is_some_and(|p| matches_fingerprint(&p, &load_fingerprint()))
}

/// Image affichée par xfdesktop sur l'écran `monitor` (nom RandR), au bureau
/// virtuel `desktop` quand chaque bureau a son fond.
fn wallpaper_of(listing: &str, monitor: &str, desktop: u32) -> Option<String> {
    let mut single = true;
    let mut single_number = 0u32;
    for line in listing.lines() {
        let mut it = line.split_whitespace();
        match (it.next(), it.next()) {
            (Some("/backdrop/single-workspace-mode"), Some(v)) => single = v != "false",
            (Some("/backdrop/single-workspace-number"), Some(v)) => {
                single_number = v.parse().unwrap_or(0)
            }
            _ => {}
        }
    }
    let workspace = if single { single_number } else { desktop };
    let wanted = format!("/monitor{monitor}/workspace{workspace}/last-image");
    let mut fallback = None;
    for line in listing.lines() {
        let Some((key, value)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if !key.ends_with("/last-image") {
            continue;
        }
        if key.ends_with(&wanted) {
            return Some(value.trim().to_string());
        }
        if fallback.is_none() && key.ends_with(&format!("/workspace{workspace}/last-image")) {
            fallback = Some(value.trim().to_string());
        }
    }
    fallback
}

// ── Batterie ───────────────────────────────────────────────────────────────

fn battery_low() -> bool {
    let dir = std::env::var("WINLUX_FOND_ANIME_POWER")
        .unwrap_or_else(|_| "/sys/class/power_supply".into());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let read = |f: &str| {
            std::fs::read_to_string(e.path().join(f))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        read("type") == "Battery"
            && read("status") == "Discharging"
            && read("capacity")
                .parse::<u32>()
                .is_ok_and(|c| c <= BATTERY_LOW)
    })
}

// ── X11 ────────────────────────────────────────────────────────────────────

struct Atoms {
    selection: Atom,
    compositor: Atom,
    client_list: Atom,
    current_desktop: Atom,
    root_pixmap: Atom,
    wm_state: Atom,
    wm_desktop: Atom,
    wm_type: Atom,
    type_desktop: Atom,
    type_dock: Atom,
    fullscreen: Atom,
    max_vert: Atom,
    max_horz: Atom,
    hidden: Atom,
    skip_taskbar: Atom,
    skip_pager: Atom,
    wm_pid: Atom,
}

fn atom(conn: &RustConnection, name: &str) -> Res<Atom> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

impl Atoms {
    fn new(conn: &RustConnection, screen: usize) -> Res<Self> {
        Ok(Atoms {
            selection: atom(conn, &format!("_WINLUX_FOND_ANIME_S{screen}"))?,
            compositor: atom(conn, &format!("_NET_WM_CM_S{screen}"))?,
            client_list: atom(conn, "_NET_CLIENT_LIST")?,
            current_desktop: atom(conn, "_NET_CURRENT_DESKTOP")?,
            root_pixmap: atom(conn, "_XROOTPMAP_ID")?,
            wm_state: atom(conn, "_NET_WM_STATE")?,
            wm_desktop: atom(conn, "_NET_WM_DESKTOP")?,
            wm_type: atom(conn, "_NET_WM_WINDOW_TYPE")?,
            type_desktop: atom(conn, "_NET_WM_WINDOW_TYPE_DESKTOP")?,
            type_dock: atom(conn, "_NET_WM_WINDOW_TYPE_DOCK")?,
            fullscreen: atom(conn, "_NET_WM_STATE_FULLSCREEN")?,
            max_vert: atom(conn, "_NET_WM_STATE_MAXIMIZED_VERT")?,
            max_horz: atom(conn, "_NET_WM_STATE_MAXIMIZED_HORZ")?,
            hidden: atom(conn, "_NET_WM_STATE_HIDDEN")?,
            skip_taskbar: atom(conn, "_NET_WM_STATE_SKIP_TASKBAR")?,
            skip_pager: atom(conn, "_NET_WM_STATE_SKIP_PAGER")?,
            wm_pid: atom(conn, "_NET_WM_PID")?,
        })
    }
}

fn prop32(conn: &RustConnection, win: Window, prop: Atom) -> Vec<u32> {
    conn.get_property(false, win, prop, AtomEnum::ANY, 0, 4096)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default()
}

fn owner(conn: &RustConnection, selection: Atom) -> Window {
    conn.get_selection_owner(selection)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.owner)
        .unwrap_or(x11rb::NONE)
}

#[derive(Clone)]
struct Monitor {
    name: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

fn monitors(conn: &RustConnection, root: Window, screen: &Screen) -> Vec<Monitor> {
    let found: Vec<Monitor> = conn
        .extension_information(randr::X11_EXTENSION_NAME)
        .ok()
        .flatten()
        .and_then(|_| conn.randr_get_monitors(root, true).ok())
        .and_then(|c| c.reply().ok())
        .map(|r| {
            r.monitors
                .iter()
                .map(|m| Monitor {
                    name: conn
                        .get_atom_name(m.name)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|n| String::from_utf8_lossy(&n.name).into_owned())
                        .unwrap_or_default(),
                    x: m.x as i32,
                    y: m.y as i32,
                    w: m.width as i32,
                    h: m.height as i32,
                })
                .collect()
        })
        .unwrap_or_default();
    if found.is_empty() {
        vec![Monitor {
            name: "screen".into(),
            x: 0,
            y: 0,
            w: screen.width_in_pixels as i32,
            h: screen.height_in_pixels as i32,
        }]
    } else {
        found
    }
}

/// Étoile placée sur un écran : xfdesktop affiche l'image « zoomée » (style
/// 5) : elle couvre l'écran, centrée, rognée sur le côté qui dépasse.
struct Placed {
    star: Star,
    monitor: usize,
    x: i32,
    y: i32,
    radius: i32,
    level: u8,
}

struct Meteor {
    monitor: usize,
    start: Instant,
    x0: f64,
    y0: f64,
    dx: f64,
    dy: f64,
    tail: f64,
    width: f64,
    last: Option<(i32, i32, u16, u16)>,
}

struct Anim {
    conn: RustConnection,
    root: Window,
    win: Window,
    gc: Gcontext,
    atoms: Atoms,
    screen: Screen,
    stars: Vec<Star>,
    monitors: Vec<Monitor>,
    /// Écrans qui affichent « Winlux Aurore ».
    aurora: Vec<bool>,
    /// Écrans entièrement cachés par une fenêtre plein écran ou agrandie.
    covered: Vec<bool>,
    placed: Vec<Placed>,
    clients: HashSet<Window>,
    desktops_seen: HashSet<Window>,
    saver_on: bool,
    screen_off: bool,
    battery_low: bool,
    meteor: Option<Meteor>,
    next_meteor: Instant,
    rng: Rng,
    start: Instant,
    drawn: bool,
    /// Étoiles déjà calculées, par (rayon, niveau) : rien à recalculer à chaque pas.
    sprites: std::collections::HashMap<(i32, u8), Vec<u8>>,
}

#[derive(PartialEq)]
enum Verdict {
    Continue,
    Quit(&'static str),
}

impl Anim {
    fn connect() -> Res<Self> {
        let (conn, screen_num) = RustConnection::connect(None)?;
        let screen = conn.setup().roots[screen_num].clone();
        let root = screen.root;
        let atoms = Atoms::new(&conn, screen_num)?;
        // Visuel 32 bits avec alpha : sans lui la couche ne serait pas transparente.
        let visual = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.class == VisualClass::TRUE_COLOR)
            .map(|v| v.visual_id)
            .ok_or("pas de visuel 32 bits")?;
        let colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual)?;
        let win = conn.generate_id()?;
        conn.create_window(
            32,
            win,
            root,
            0,
            0,
            screen.width_in_pixels,
            screen.height_in_pixels,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .event_mask(EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY),
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, win, &CreateGCAux::new())?;
        Ok(Anim {
            conn,
            root,
            win,
            gc,
            atoms,
            screen,
            stars: load_stars(),
            monitors: Vec::new(),
            aurora: Vec::new(),
            covered: Vec::new(),
            placed: Vec::new(),
            clients: HashSet::new(),
            desktops_seen: HashSet::new(),
            saver_on: false,
            screen_off: false,
            battery_low: false,
            meteor: None,
            next_meteor: Instant::now() + Duration::from_secs(20),
            rng: Rng(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64 | 1)
                .unwrap_or(1)),
            start: Instant::now(),
            drawn: false,
            sprites: std::collections::HashMap::new(),
        })
    }

    /// Fenêtre de bureau, sur tous les bureaux virtuels, hors barre des
    /// tâches, sans focus et sans entrée : les clics traversent.
    fn setup_window(&self) -> Res<()> {
        let c = &self.conn;
        let a = &self.atoms;
        c.change_property8(
            PropMode::REPLACE,
            self.win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"winlux-fond-anime\0winlux-fond-anime\0",
        )?;
        c.change_property8(
            PropMode::REPLACE,
            self.win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Fond anime Winlux",
        )?;
        c.change_property32(
            PropMode::REPLACE,
            self.win,
            a.wm_type,
            AtomEnum::ATOM,
            &[a.type_desktop],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            self.win,
            a.wm_state,
            AtomEnum::ATOM,
            &[a.skip_taskbar, a.skip_pager],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            self.win,
            a.wm_desktop,
            AtomEnum::CARDINAL,
            &[0xFFFF_FFFF],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            self.win,
            a.wm_pid,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )?;
        // WM_HINTS : input = False.
        c.change_property32(
            PropMode::REPLACE,
            self.win,
            AtomEnum::WM_HINTS,
            AtomEnum::WM_HINTS,
            &[1, 0, 0, 0, 0, 0, 0, 0, 0],
        )?;
        c.shape_rectangles(
            shape::SO::SET,
            shape::SK::INPUT,
            ClipOrdering::UNSORTED,
            self.win,
            0,
            0,
            &[],
        )?;
        Ok(())
    }

    fn subscribe(&self) -> Res<()> {
        let c = &self.conn;
        c.change_window_attributes(
            self.root,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
        )?;
        c.xfixes_query_version(5, 0)?.reply()?;
        c.xfixes_select_selection_input(
            self.win,
            self.atoms.compositor,
            xfixes::SelectionEventMask::SET_SELECTION_OWNER
                | xfixes::SelectionEventMask::SELECTION_WINDOW_DESTROY
                | xfixes::SelectionEventMask::SELECTION_CLIENT_CLOSE,
        )?;
        if c.extension_information(screensaver::X11_EXTENSION_NAME)?
            .is_some()
        {
            c.screensaver_query_version(1, 1)?.reply()?;
            c.screensaver_select_input(self.root, screensaver::Event::NOTIFY_MASK)?;
        }
        if c.extension_information(randr::X11_EXTENSION_NAME)?
            .is_some()
        {
            c.randr_query_version(1, 5)?.reply()?;
            c.randr_select_input(self.root, randr::NotifyMask::SCREEN_CHANGE)?;
        }
        Ok(())
    }

    fn current_desktop(&self) -> u32 {
        prop32(&self.conn, self.root, self.atoms.current_desktop)
            .first()
            .copied()
            .unwrap_or(0)
    }

    /// Écrans et fonds affichés. `false` : plus rien à animer.
    fn refresh_layout(&mut self) -> bool {
        self.monitors = monitors(&self.conn, self.root, &self.screen);
        let listing = xfconf_desktop();
        let desktop = self.current_desktop();
        self.aurora = self
            .monitors
            .iter()
            .map(|m| wallpaper_of(&listing, &m.name, desktop).is_some_and(|w| is_aurora(&w)))
            .collect();
        self.covered = vec![false; self.monitors.len()];
        self.placed.clear();
        for (i, m) in self.monitors.iter().enumerate() {
            if !self.aurora[i] {
                continue;
            }
            let sw = (m.w as f64).max(m.h as f64 * 16.0 / 9.0);
            let sh = sw * 9.0 / 16.0;
            let ox = m.x as f64 + (m.w as f64 - sw) / 2.0;
            let oy = m.y as f64 + (m.h as f64 - sh) / 2.0;
            let radius = ((3.0 * sh / 1080.0).round() as i32).max(3);
            for s in &self.stars {
                let x = (ox + s.u * sw).round() as i32;
                let y = (oy + s.v * sh).round() as i32;
                if x - radius < m.x
                    || y - radius < m.y
                    || x + radius >= m.x + m.w
                    || y + radius >= m.y + m.h
                {
                    continue;
                }
                self.placed.push(Placed {
                    star: *s,
                    monitor: i,
                    x,
                    y,
                    radius,
                    level: 0,
                });
            }
        }
        let _ = self.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new()
                .width(self.screen.width_in_pixels as u32)
                .height(self.screen.height_in_pixels as u32),
        );
        self.aurora.iter().any(|&a| a)
    }

    /// Fenêtres plein écran ou agrandies, sur le bureau virtuel affiché.
    fn refresh_clients(&mut self) {
        let a = &self.atoms;
        let list = prop32(&self.conn, self.root, a.client_list);
        let desktop = self.current_desktop();
        let mut covered = vec![false; self.monitors.len()];
        let mut raise = false;
        for &w in &list {
            if w == self.win {
                continue;
            }
            if self.clients.insert(w) {
                let _ = self.conn.change_window_attributes(
                    w,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                );
            }
            let types = prop32(&self.conn, w, a.wm_type);
            if types.contains(&a.type_desktop) {
                // Bureau relancé (xfdesktop) : il passerait au-dessus de la couche.
                if self.desktops_seen.insert(w) {
                    raise = true;
                }
                continue;
            }
            if types.contains(&a.type_dock) {
                continue;
            }
            let on = prop32(&self.conn, w, a.wm_desktop)
                .first()
                .copied()
                .unwrap_or(desktop);
            if on != desktop && on != 0xFFFF_FFFF {
                continue;
            }
            let state = prop32(&self.conn, w, a.wm_state);
            if state.contains(&a.hidden) {
                continue;
            }
            let full = state.contains(&a.fullscreen)
                || (state.contains(&a.max_vert) && state.contains(&a.max_horz));
            if !full {
                continue;
            }
            let Some(geo) = self.conn.get_geometry(w).ok().and_then(|c| c.reply().ok()) else {
                continue;
            };
            let Some(pos) = self
                .conn
                .translate_coordinates(w, self.root, 0, 0)
                .ok()
                .and_then(|c| c.reply().ok())
            else {
                continue;
            };
            let cx = pos.dst_x as i32 + geo.width as i32 / 2;
            let cy = pos.dst_y as i32 + geo.height as i32 / 2;
            for (i, m) in self.monitors.iter().enumerate() {
                if cx >= m.x && cx < m.x + m.w && cy >= m.y && cy < m.y + m.h {
                    covered[i] = true;
                }
            }
        }
        self.clients.retain(|w| list.contains(w));
        self.covered = covered;
        if raise {
            let _ = self.conn.configure_window(
                self.win,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            );
        }
    }

    fn check_dpms(&mut self) {
        let off = self
            .conn
            .extension_information(dpms::X11_EXTENSION_NAME)
            .ok()
            .flatten()
            .and_then(|_| self.conn.dpms_info().ok())
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.state && r.power_level != dpms::DPMSMode::ON);
        self.screen_off = off;
    }

    fn paused(&self) -> bool {
        self.saver_on || self.screen_off || self.battery_low
    }

    fn visible(&self, monitor: usize) -> bool {
        !self.paused()
            && self.aurora.get(monitor).copied().unwrap_or(false)
            && !self.covered.get(monitor).copied().unwrap_or(true)
    }

    fn any_visible(&self) -> bool {
        (0..self.monitors.len()).any(|i| self.visible(i))
    }

    // ── Dessin ──

    fn put(&self, x: i32, y: i32, w: u16, h: u16, data: &[u8]) {
        let _ = self.conn.put_image(
            ImageFormat::Z_PIXMAP,
            self.win,
            self.gc,
            w,
            h,
            x as i16,
            y as i16,
            0,
            32,
            data,
        );
    }

    fn clear(&self, x: i32, y: i32, w: u16, h: u16) {
        let _ = self
            .conn
            .clear_area(false, self.win, x as i16, y as i16, w, h);
    }

    fn star_sprite(radius: i32, level: u8) -> Vec<u8> {
        let size = (2 * radius + 1) as usize;
        let mut out = vec![0u8; size * size * 4];
        let peak = 0.85 * level as f64 / LEVELS as f64;
        let sigma = radius as f64 * 0.38;
        for j in 0..size {
            for i in 0..size {
                let dx = i as f64 - radius as f64;
                let dy = j as f64 - radius as f64;
                let d2 = dx * dx + dy * dy;
                let mut a = (-d2 / (sigma * sigma)).exp();
                // Petite croix : l'éclat d'une étoile qui scintille.
                if dx == 0.0 || dy == 0.0 {
                    a = a.max(0.55 * (1.0 - (dx.abs() + dy.abs()) / (radius as f64 + 1.0)));
                }
                let a = (a * peak).clamp(0.0, 1.0);
                let k = (j * size + i) * 4;
                // ARGB32 prémultiplié, octets B, G, R, A ; teinte #E6DCFF.
                out[k] = (0xFF as f64 * a) as u8;
                out[k + 1] = (0xDC as f64 * a) as u8;
                out[k + 2] = (0xE6 as f64 * a) as u8;
                out[k + 3] = (255.0 * a) as u8;
            }
        }
        out
    }

    fn twinkle(&mut self, t: f64) {
        for idx in 0..self.placed.len() {
            let p = &self.placed[idx];
            if !self.visible(p.monitor) {
                continue;
            }
            let s = (2.0 * std::f64::consts::PI * (t / p.star.period + p.star.phase))
                .sin()
                .max(0.0);
            let level = (s.powi(10) * LEVELS as f64).round() as u8;
            if level == p.level {
                continue;
            }
            let size = (2 * p.radius + 1) as u16;
            if level == 0 {
                self.clear(p.x - p.radius, p.y - p.radius, size, size);
            } else {
                let (x, y, radius) = (p.x - p.radius, p.y - p.radius, p.radius);
                let sprite = self
                    .sprites
                    .entry((radius, level))
                    .or_insert_with(|| Self::star_sprite(radius, level));
                let _ = self.conn.put_image(
                    ImageFormat::Z_PIXMAP,
                    self.win,
                    self.gc,
                    size,
                    size,
                    x as i16,
                    y as i16,
                    0,
                    32,
                    sprite,
                );
            }
            self.placed[idx].level = level;
            self.drawn = true;
        }
    }

    fn start_meteor(&mut self) {
        let candidates: Vec<usize> = (0..self.monitors.len())
            .filter(|&i| self.visible(i))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let i = candidates[(self.rng.next() * candidates.len() as f64) as usize % candidates.len()];
        let m = &self.monitors[i];
        let scale = m.h as f64 / 1080.0;
        // Dans le haut du ciel, loin du logo ; de gauche à droite ou l'inverse.
        let x0 = m.x as f64 + m.w as f64 * (0.15 + 0.7 * self.rng.next());
        let y0 = m.y as f64 + m.h as f64 * (0.04 + 0.14 * self.rng.next());
        let dir = if self.rng.next() < 0.5 { -1.0 } else { 1.0 };
        let angle = (18.0 + 14.0 * self.rng.next()).to_radians();
        let travel = m.w as f64 * 0.16;
        self.meteor = Some(Meteor {
            monitor: i,
            start: Instant::now(),
            x0,
            y0,
            dx: dir * travel * angle.cos(),
            dy: travel * angle.sin(),
            tail: 110.0 * scale,
            width: 1.3 * scale.max(1.0),
            last: None,
        });
    }

    /// Une image de l'étoile filante ; `false` quand elle est passée.
    fn meteor_frame(&mut self) -> bool {
        let Some(m) = self.meteor.as_mut() else {
            return false;
        };
        let p = m.start.elapsed().as_secs_f64() / METEOR_SECONDS;
        let alive = p < 1.0 && self.monitors.get(m.monitor).is_some();
        let mut rect = m.last;
        let mut seg = None;
        if alive {
            let hx = m.x0 + m.dx * p;
            let hy = m.y0 + m.dy * p;
            let len = (m.dx * m.dx + m.dy * m.dy).sqrt();
            let (ux, uy) = (m.dx / len, m.dy / len);
            let tail = m.tail * (0.4 + 0.6 * (std::f64::consts::PI * p).sin());
            let (tx, ty) = (hx - ux * tail, hy - uy * tail);
            let pad = 3.0 + m.width;
            let x = tx.min(hx) - pad;
            let y = ty.min(hy) - pad;
            let cur = (
                x.floor() as i32,
                y.floor() as i32,
                ((tx - hx).abs() + 2.0 * pad).ceil() as u16,
                ((ty - hy).abs() + 2.0 * pad).ceil() as u16,
            );
            rect = Some(match rect {
                Some(r) => union(r, cur),
                None => cur,
            });
            seg = Some((
                hx,
                hy,
                ux,
                uy,
                tail,
                (std::f64::consts::PI * p).sin(),
                m.width,
            ));
            m.last = Some(cur);
        }
        if let Some((rx, ry, rw, rh)) = rect {
            if let Some((hx, hy, ux, uy, tail, glow, width)) = seg {
                let mut buf = vec![0u8; rw as usize * rh as usize * 4];
                for j in 0..rh as usize {
                    for i in 0..rw as usize {
                        let px = rx as f64 + i as f64 + 0.5 - hx;
                        let py = ry as f64 + j as f64 + 0.5 - hy;
                        // Distance le long de la traînée (0 : tête) et en travers.
                        let along = -(px * ux + py * uy);
                        let across = (px * uy - py * ux).abs();
                        if along < -width || along > tail {
                            continue;
                        }
                        let f = (1.0 - along.max(0.0) / tail).powf(1.6);
                        let a =
                            (glow * f * (1.0 - across / (width + 0.8)).max(0.0)).clamp(0.0, 1.0);
                        if a <= 0.0 {
                            continue;
                        }
                        let k = (j * rw as usize + i) * 4;
                        buf[k] = (0xFF as f64 * a) as u8;
                        buf[k + 1] = (0xF0 as f64 * a) as u8;
                        buf[k + 2] = (0xF4 as f64 * a) as u8;
                        buf[k + 3] = (255.0 * a) as u8;
                    }
                }
                self.put(rx, ry, rw, rh, &buf);
            } else {
                self.clear(rx, ry, rw, rh);
            }
            self.drawn = true;
        }
        if !alive {
            self.meteor = None;
            // Une étoile scintillante effacée par la traînée revient au prochain pas.
            for p in &mut self.placed {
                p.level = 0;
            }
        }
        alive
    }

    /// Tout effacer : couche transparente, rien à recomposer ensuite.
    fn blank(&mut self) {
        if self.drawn {
            self.clear(
                0,
                0,
                self.screen.width_in_pixels,
                self.screen.height_in_pixels,
            );
            self.drawn = false;
        }
        for p in &mut self.placed {
            p.level = 0;
        }
        if let Some(m) = self.meteor.take() {
            let _ = m;
        }
    }

    fn handle(&mut self, ev: Event) -> Verdict {
        let a = &self.atoms;
        match ev {
            Event::SelectionClear(e) if e.selection == a.selection => {
                return Verdict::Quit("remplacé ou désactivé")
            }
            Event::XfixesSelectionNotify(e) if e.selection == a.compositor => {
                if e.owner == x11rb::NONE {
                    return Verdict::Quit("plus de compositeur");
                }
            }
            Event::ScreensaverNotify(e) => {
                self.saver_on =
                    e.state == screensaver::State::ON || e.state == screensaver::State::CYCLE;
            }
            Event::RandrScreenChangeNotify(e) => {
                self.screen.width_in_pixels = e.width;
                self.screen.height_in_pixels = e.height;
                self.blank();
                if !self.refresh_layout() {
                    return Verdict::Quit("fond changé");
                }
                self.refresh_clients();
            }
            Event::PropertyNotify(e) if e.window == self.root => {
                if e.atom == a.root_pixmap || e.atom == a.current_desktop {
                    // Fond changé (xfdesktop repose sa pixmap) ou autre bureau virtuel.
                    self.blank();
                    if !self.refresh_layout() {
                        return Verdict::Quit("fond changé");
                    }
                    self.refresh_clients();
                } else if e.atom == a.client_list {
                    self.refresh_clients();
                }
            }
            Event::PropertyNotify(e) if e.atom == a.wm_state || e.atom == a.wm_desktop => {
                self.refresh_clients()
            }
            Event::Expose(e) if e.window == self.win => {
                // Zone repeinte transparente : on redessine au prochain pas.
                let (x0, y0, x1, y1) = (
                    e.x as i32,
                    e.y as i32,
                    e.x as i32 + e.width as i32,
                    e.y as i32 + e.height as i32,
                );
                for p in &mut self.placed {
                    if p.x + p.radius >= x0
                        && p.x - p.radius < x1
                        && p.y + p.radius >= y0
                        && p.y - p.radius < y1
                    {
                        p.level = 0;
                    }
                }
            }
            _ => {}
        }
        Verdict::Continue
    }

    fn run(mut self) -> Res<&'static str> {
        if owner(&self.conn, self.atoms.compositor) == x11rb::NONE {
            return Ok("aucun compositeur : la couche transparente serait noire");
        }
        if self.stars.is_empty() {
            return Ok("aucune étoile à animer");
        }
        if !self.refresh_layout() {
            return Ok("le fond affiché n'est pas Winlux Aurore");
        }
        // Instance unique : prendre la sélection, garder la main.
        self.conn
            .set_selection_owner(self.win, self.atoms.selection, CURRENT_TIME)?;
        if owner(&self.conn, self.atoms.selection) != self.win {
            return Ok("une autre animation tourne déjà");
        }
        self.setup_window()?;
        self.subscribe()?;
        self.conn.map_window(self.win)?;
        self.refresh_clients();
        self.check_dpms();
        self.battery_low = battery_low();
        if let Some(r) = self
            .conn
            .extension_information(screensaver::X11_EXTENSION_NAME)?
            .and_then(|_| self.conn.screensaver_query_info(self.root).ok())
            .and_then(|c| c.reply().ok())
        {
            self.saver_on = r.state == u8::from(screensaver::State::ON)
                || r.state == u8::from(screensaver::State::CYCLE);
        }
        self.conn.flush()?;
        let mut next_dpms = Instant::now() + DPMS_EVERY;
        let mut next_battery = Instant::now() + BATTERY_EVERY;
        let mut next_frame = Instant::now();
        let mut was_visible = true;
        loop {
            while let Some(ev) = self.conn.poll_for_event()? {
                if let Verdict::Quit(why) = self.handle(ev) {
                    self.conn.flush()?;
                    return Ok(why);
                }
            }
            let now = Instant::now();
            if now >= next_dpms {
                self.check_dpms();
                next_dpms = now + DPMS_EVERY;
            }
            if now >= next_battery {
                self.battery_low = battery_low();
                next_battery = now + BATTERY_EVERY;
            }
            let visible = self.any_visible();
            if !visible && was_visible {
                self.blank();
            }
            if visible && !was_visible {
                next_frame = now;
                if self.next_meteor < now {
                    self.next_meteor = now + Duration::from_secs(10);
                }
            }
            was_visible = visible;
            let mut wake = next_dpms.min(next_battery);
            if visible {
                if now >= next_frame {
                    let t = self.start.elapsed().as_secs_f64();
                    if self.meteor.is_none() && now >= self.next_meteor {
                        self.start_meteor();
                        self.next_meteor =
                            now + Duration::from_secs_f64(30.0 + 50.0 * self.rng.next());
                    }
                    let meteor = self.meteor_frame();
                    self.twinkle(t);
                    next_frame = now + if meteor { METEOR_TICK } else { TWINKLE_TICK };
                }
                wake = wake.min(next_frame);
            }
            self.conn.flush()?;
            let ms = wake
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(i32::MAX as u128) as i32;
            let mut pfd = libc::pollfd {
                fd: self.conn.stream().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            unsafe {
                libc::poll(&mut pfd, 1, ms);
            }
            if pfd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                return Ok("serveur X fermé");
            }
        }
    }
}

fn union(a: (i32, i32, u16, u16), b: (i32, i32, u16, u16)) -> (i32, i32, u16, u16) {
    let x0 = a.0.min(b.0);
    let y0 = a.1.min(b.1);
    let x1 = (a.0 + a.2 as i32).max(b.0 + b.2 as i32);
    let y1 = (a.1 + a.3 as i32).max(b.1 + b.3 as i32);
    (x0, y0, (x1 - x0) as u16, (y1 - y0) as u16)
}

// ── Commandes ──────────────────────────────────────────────────────────────

/// État vu de l'extérieur, sans rien lancer.
struct Probe {
    running: bool,
    reason: &'static str,
}

fn probe() -> Probe {
    let Ok((conn, screen)) = RustConnection::connect(None) else {
        return Probe {
            running: false,
            reason: "pas d'écran X",
        };
    };
    let Ok(atoms) = Atoms::new(&conn, screen) else {
        return Probe {
            running: false,
            reason: "pas d'écran X",
        };
    };
    let running = owner(&conn, atoms.selection) != x11rb::NONE;
    let root = conn.setup().roots[screen].clone();
    let reason = if !enabled() {
        "desactive"
    } else if owner(&conn, atoms.compositor) == x11rb::NONE {
        "sans-compositeur"
    } else {
        let listing = xfconf_desktop();
        let desktop = prop32(&conn, root.root, atoms.current_desktop)
            .first()
            .copied()
            .unwrap_or(0);
        let mons = monitors(&conn, root.root, &root);
        if mons
            .iter()
            .any(|m| wallpaper_of(&listing, &m.name, desktop).is_some_and(|w| is_aurora(&w)))
        {
            "aurore"
        } else {
            "autre-fond"
        }
    };
    Probe { running, reason }
}

/// Reprend la sélection : l'instance en place reçoit SelectionClear et s'arrête.
fn stop() -> Res<()> {
    let (conn, screen) = RustConnection::connect(None)?;
    let atoms = Atoms::new(&conn, screen)?;
    let previous = owner(&conn, atoms.selection);
    if previous == x11rb::NONE {
        return Ok(());
    }
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id()?;
    conn.create_window(
        0,
        win,
        root,
        -1,
        -1,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )?;
    conn.set_selection_owner(win, atoms.selection, CURRENT_TIME)?;
    conn.sync()?;
    // L'ancienne couche disparaît avec sa connexion ; deux secondes au plus.
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        let alive = conn
            .get_window_attributes(previous)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some();
        if !alive {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    conn.destroy_window(win)?;
    conn.sync()?;
    Ok(())
}

fn spawn_background() -> Res<()> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .arg("run")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    Ok(())
}

fn ensure() -> Res<()> {
    let p = probe();
    if p.reason == "aurore" {
        if !p.running {
            spawn_background()?;
        }
    } else if p.running {
        stop()?;
    }
    Ok(())
}

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    let result: Res<()> = match cmd.as_str() {
        "status" => {
            let p = probe();
            println!("enabled={}", if enabled() { "on" } else { "off" });
            println!("running={}", if p.running { "yes" } else { "no" });
            println!("reason={}", p.reason);
            Ok(())
        }
        "enable" => set_enabled(true).and_then(|_| ensure()),
        "disable" => set_enabled(false).and_then(|_| stop()),
        "ensure" => ensure(),
        "run" => {
            if !enabled() {
                eprintln!("winlux-fond-anime : désactivé dans Paramètres");
                std::process::exit(0);
            }
            Anim::connect()
                .and_then(|a| a.run())
                .map(|why| eprintln!("winlux-fond-anime : arrêt ({why})"))
        }
        _ => {
            eprintln!("usage : winlux-fond-anime status | enable | disable | ensure | run");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("winlux-fond-anime : {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aurore_sombre_seulement() {
        assert!(is_aurora(
            "/usr/share/winlux/wallpapers/winlux-prisme-aurore.png"
        ));
        assert!(is_aurora(
            "/usr/share/winlux/wallpapers/winlux-prisme-aurore-3840x2160.png"
        ));
        assert!(!is_aurora(
            "/usr/share/winlux/wallpapers/winlux-prisme-aurore-clair.png"
        ));
        assert!(!is_aurora("/home/a/winlux-prisme-nuit.png"));
    }

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/wallpapers")
    }

    /// La copie BMP posée par l'installation, et toute image renommée, sont
    /// reconnues par leur contenu ; les autres fonds Winlux ne le sont pas.
    #[test]
    fn aurore_reconnue_par_son_contenu_3079() {
        std::env::set_var(
            "WINLUX_FOND_ANIME_ETOILES",
            assets().join("winlux-prisme-aurore.etoiles"),
        );
        let empreinte = load_fingerprint();
        assert_eq!(empreinte.len(), 20);
        let ouvre = |n: &str| Picture::open(assets().join(n).to_str().unwrap()).unwrap();
        for n in [
            "winlux-prisme-aurore.png",
            "winlux-prisme-aurore-2560x1440.png",
            "winlux-prisme-aurore-3840x2160.png",
        ] {
            assert!(matches_fingerprint(&ouvre(n), &empreinte), "{n}");
        }
        for n in [
            "winlux-prisme-aurore-clair.png",
            "winlux-prisme-nuit.png",
            "winlux-prisme-aube.png",
        ] {
            assert!(!matches_fingerprint(&ouvre(n), &empreinte), "{n}");
        }
        // BMP 24 bits de bas en haut, comme convert BMP3 et ffmpeg bgr24.
        let p = ouvre("winlux-prisme-aurore.png");
        let stride = (p.w * 3).div_ceil(4) * 4;
        let mut bmp = Vec::new();
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&((54 + stride * p.h) as u32).to_le_bytes());
        bmp.extend_from_slice(&[0, 0, 0, 0]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(p.w as i32).to_le_bytes());
        bmp.extend_from_slice(&(p.h as i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[0u8; 24]);
        for y in (0..p.h).rev() {
            let mut row = vec![0u8; stride];
            for x in 0..p.w {
                let k = (y * p.w + x) * 3;
                row[x * 3..x * 3 + 3].copy_from_slice(&[p.rgb[k + 2], p.rgb[k + 1], p.rgb[k]]);
            }
            bmp.extend_from_slice(&row);
        }
        let dir = std::env::temp_dir().join(format!("fond-anime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wallpaper.bmp");
        std::fs::write(&path, &bmp).unwrap();
        assert!(is_aurora(path.to_str().unwrap()), "copie wallpaper.bmp");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn fond_par_ecran_et_bureau() {
        let l =
            "/backdrop/screen0/monitoreDP-1/workspace0/last-image   /a/winlux-prisme-aurore.png\n\
                 /backdrop/screen0/monitoreDP-1/workspace1/last-image   /a/photo.jpg\n\
                 /backdrop/screen0/monitorHDMI-1/workspace0/last-image  /a/photo.jpg\n";
        assert_eq!(
            wallpaper_of(l, "eDP-1", 1).as_deref(),
            Some("/a/winlux-prisme-aurore.png")
        );
        assert_eq!(
            wallpaper_of(l, "HDMI-1", 0).as_deref(),
            Some("/a/photo.jpg")
        );
        let l2 = format!("/backdrop/single-workspace-mode  false\n{l}");
        assert_eq!(
            wallpaper_of(&l2, "eDP-1", 1).as_deref(),
            Some("/a/photo.jpg")
        );
        // Écran inconnu d'xfconf : le premier fond du bureau.
        assert_eq!(
            wallpaper_of(l, "screen", 0).as_deref(),
            Some("/a/winlux-prisme-aurore.png")
        );
    }

    #[test]
    fn etoile_transparente_au_repos() {
        let s = Anim::star_sprite(3, 0);
        assert!(s.iter().all(|&b| b == 0));
        let s = Anim::star_sprite(3, LEVELS);
        let centre = (3 * 7 + 3) * 4;
        assert!(s[centre + 3] > 180);
        // Prémultiplié : aucune composante au-dessus de l'alpha.
        assert!(s
            .chunks(4)
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
    }
}
