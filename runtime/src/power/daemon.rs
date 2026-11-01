//! Démon d'alimentation lancé par l'init Winlux (pas de systemd).
//!
//! - publie l'état batterie/réglages dans `/run/winlux/power.txt` ;
//! - éteint l'écran puis met en veille après inactivité, sauf si un `.exe`
//!   l'a interdit par `SetThreadExecutionState`/`PowerSetRequest` ;
//! - réagit au capot, au bouton d'alimentation et à la touche veille
//!   (lecture directe des périphériques evdev du noyau) ;
//! - met en veille prolongée quand la batterie devient critique ;
//! - applique les touches de luminosité du clavier (sauf si le noyau ou le
//!   micrologiciel l'a déjà fait, pour ne jamais compter deux pas) ;
//! - capot fermé avec un écran externe branché : la machine reste éveillée
//!   (mode « station », comme Windows).

use super::{
    active_inhibits_at, last_activity_at, now_secs, read_status_at, read_trim,
    set_backlight_power_at, sleep, status_lines, touch_activity_at, Action, Charge, Config,
    ES_AWAYMODE_REQUIRED, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Évènement matériel reçu du noyau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hw {
    LidClosed,
    LidOpened,
    PowerButton,
    SleepButton,
    BrightnessUp,
    BrightnessDown,
}

const EV_KEY: u16 = 1;
const EV_SW: u16 = 5;
const KEY_POWER: u16 = 116;
const KEY_SLEEP: u16 = 142;
const KEY_SUSPEND: u16 = 205;
const SW_LID: u16 = 0;
const KEY_BRIGHTNESSDOWN: u16 = 224;
const KEY_BRIGHTNESSUP: u16 = 225;

/// Décode un `struct input_event` 64 bits (24 octets).
pub fn decode_event(buf: &[u8; 24]) -> Option<Hw> {
    let kind = u16::from_ne_bytes([buf[16], buf[17]]);
    let code = u16::from_ne_bytes([buf[18], buf[19]]);
    let value = i32::from_ne_bytes([buf[20], buf[21], buf[22], buf[23]]);
    match (kind, code, value) {
        (EV_KEY, KEY_POWER, 1) => Some(Hw::PowerButton),
        (EV_KEY, KEY_SLEEP | KEY_SUSPEND, 1) => Some(Hw::SleepButton),
        (EV_SW, SW_LID, 1) => Some(Hw::LidClosed),
        (EV_SW, SW_LID, 0) => Some(Hw::LidOpened),
        // 1 = appui, 2 = répétition quand la touche reste enfoncée.
        (EV_KEY, KEY_BRIGHTNESSUP, 1 | 2) => Some(Hw::BrightnessUp),
        (EV_KEY, KEY_BRIGHTNESSDOWN, 1 | 2) => Some(Hw::BrightnessDown),
        _ => None,
    }
}

/// Vrai si le masque `capabilities/key` du noyau (mots hexadécimaux, le
/// plus significatif d'abord) annonce la touche `code`.
pub fn has_key(caps: &str, code: u16) -> bool {
    let words: Vec<&str> = caps.split_whitespace().collect();
    let index = usize::from(code / 64);
    let Some(word) = words.len().checked_sub(index + 1).map(|i| words[i]) else {
        return false;
    };
    u64::from_str_radix(word, 16).is_ok_and(|w| w & (1u64 << (code % 64)) != 0)
}

/// Périphériques d'entrée utiles : bouton, capot, touche veille et tout
/// périphérique qui annonce les touches de luminosité (bus vidéo ACPI,
/// touches spéciales du constructeur, clavier interne).
pub fn acpi_inputs(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(dir) = fs::read_dir(root.join("sys/class/input")) else {
        return found;
    };
    for entry in dir.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("event") {
            continue;
        }
        let dev = read_trim(&entry.path().join("device/name")).unwrap_or_default();
        let keys = read_trim(&entry.path().join("device/capabilities/key")).unwrap_or_default();
        if matches!(dev.as_str(), "Power Button" | "Sleep Button" | "Lid Switch")
            || has_key(&keys, KEY_BRIGHTNESSUP)
            || has_key(&keys, KEY_BRIGHTNESSDOWN)
        {
            found.push(root.join("dev/input").join(name));
        }
    }
    found.sort();
    found
}

/// Écran externe branché (connecteur DRM qui n'est pas la dalle interne).
pub fn external_display_at(root: &Path) -> bool {
    let Ok(dir) = fs::read_dir(root.join("sys/class/drm")) else {
        return false;
    };
    dir.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        // card0-eDP-1 → eDP-1 ; « card0 » seul n'est pas un connecteur.
        let Some((_, connector)) = name.split_once('-') else {
            return false;
        };
        let internal = ["eDP", "LVDS", "DSI"]
            .iter()
            .any(|p| connector.starts_with(p));
        !internal && read_trim(&e.path().join("status")).as_deref() == Some("connected")
    })
}

/// Action du capot : rien quand un écran externe est branché.
pub fn lid_action(configured: Action, docked: bool) -> Action {
    if docked && matches!(configured, Action::Sleep | Action::Hibernate) {
        Action::Nothing
    } else {
        configured
    }
}

/// Pas de luminosité : 5 % sous 20 %, 10 % au-dessus (réglage fin dans le
/// bas de l'échelle, là où l'œil voit le plus la différence).
pub fn brightness_step(current: u32, up: bool) -> u32 {
    let step = if current < 20 || (!up && current <= 20) {
        5
    } else {
        10
    };
    if up {
        (current + step).min(100)
    } else {
        current.saturating_sub(step).max(5)
    }
}

/// Touche luminosité : si le rétroéclairage a déjà bougé depuis `before`
/// (le noyau ou le micrologiciel a traité la touche), on enregistre
/// seulement la nouvelle valeur ; sinon on fait le pas nous-mêmes.
/// Renvoie le pourcentage final.
pub fn brightness_key(root: &Path, up: bool, before: Option<u32>) -> Option<u32> {
    std::thread::sleep(Duration::from_millis(150));
    let now = super::brightness_at(root)?;
    let mut config = Config::load_at(root);
    let factor = config.profile.brightness_factor().max(1);
    let result = if before.is_some_and(|b| b != now) {
        now
    } else {
        let target = brightness_step(now, up);
        super::set_brightness_at(root, target)?
    };
    config.brightness = (result * 100 / factor).clamp(5, 100);
    let _ = config.save_at(root);
    Some(result)
}

fn spawn_readers(root: &Path, tx: &mpsc::Sender<Hw>) -> usize {
    let inputs = acpi_inputs(root);
    for path in &inputs {
        let tx = tx.clone();
        let path = path.clone();
        let _ = std::thread::Builder::new()
            .name("winlux-power-input".into())
            .spawn(move || {
                let Ok(mut f) = fs::File::open(&path) else {
                    return;
                };
                let mut buf = [0u8; 24];
                while f.read_exact(&mut buf).is_ok() {
                    if let Some(ev) = decode_event(&buf) {
                        if tx.send(ev).is_err() {
                            return;
                        }
                    }
                }
            });
    }
    inputs.len()
}

/// Capot fermé selon l'ACPI (repli si aucun périphérique evdev).
fn lid_closed_proc(root: &Path) -> Option<bool> {
    let dir = fs::read_dir(root.join("proc/acpi/button/lid")).ok()?;
    for e in dir.flatten() {
        if let Some(s) = read_trim(&e.path().join("state")) {
            return Some(s.contains("closed"));
        }
    }
    None
}

fn display_off(root: &Path) {
    set_backlight_power_at(root, false);
    let _ = std::process::Command::new("xset")
        .args(["dpms", "force", "off"])
        .env(
            "DISPLAY",
            std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn display_on(root: &Path) {
    set_backlight_power_at(root, true);
    let _ = std::process::Command::new("xset")
        .args(["dpms", "force", "on"])
        .env(
            "DISPLAY",
            std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Exécute une action d'alimentation ; renvoie le message de journal.
pub fn perform(root: &Path, action: Action) -> String {
    match action {
        Action::Nothing => "Aucune action".into(),
        Action::DisplayOff => {
            display_off(root);
            "\u{c9}cran \u{e9}teint".into()
        }
        Action::Sleep => match sleep::suspend_at(root) {
            Ok(()) => "Reprise apr\u{e8}s veille".into(),
            Err(e) => e,
        },
        Action::Hibernate => match sleep::hibernate_at(root) {
            Ok(()) => "Reprise apr\u{e8}s veille prolong\u{e9}e".into(),
            // Sans zone de reprise, la veille protège quand même la session.
            Err(e) => match sleep::suspend_at(root) {
                Ok(()) => format!("{e} ; veille utilis\u{e9}e \u{e0} la place"),
                Err(e2) => format!("{e} ; {e2}"),
            },
        },
        Action::Shutdown => {
            // Même chemin que le menu Démarrer : écran d'arrêt, fermeture
            // propre de la session puis signal à l'init.
            match std::process::Command::new("winlux-control")
                .arg("poweroff")
                .spawn()
            {
                Ok(_) => "Arr\u{ea}t demand\u{e9}".into(),
                Err(e) => format!("Arr\u{ea}t impossible : {e}"),
            }
        }
    }
}

/// Décision d'inactivité, isolée pour les tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleStep {
    None,
    ScreenOff,
    ScreenOn,
    Sleep,
}

pub fn idle_step(
    idle: u64,
    timeouts: (u32, u32),
    inhibits: u32,
    screen_is_off: bool,
    can_sleep: bool,
) -> IdleStep {
    let (screen, sleep_after) = timeouts;
    let keep_display = inhibits & ES_DISPLAY_REQUIRED != 0;
    let keep_system =
        inhibits & (ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED | ES_DISPLAY_REQUIRED) != 0;
    if sleep_after > 0 && idle >= u64::from(sleep_after) && !keep_system && can_sleep {
        return IdleStep::Sleep;
    }
    let want_off = screen > 0 && idle >= u64::from(screen) && !keep_display;
    match (want_off, screen_is_off) {
        (true, false) => IdleStep::ScreenOff,
        (false, true) => IdleStep::ScreenOn,
        _ => IdleStep::None,
    }
}

/// Inactivité vue par le serveur X (extension MIT-SCREEN-SAVER) : couvre
/// aussi les applications natives (navigateur, terminal) qui ne passent pas
/// par le runtime Winlux.
struct XIdle {
    conn: Option<(x11rb::rust_connection::RustConnection, u32)>,
    retry_at: u64,
}

impl XIdle {
    fn secs(&mut self, now: u64) -> Option<u64> {
        use x11rb::connection::Connection as _;
        use x11rb::protocol::screensaver::ConnectionExt as _;
        if self.conn.is_none() && now >= self.retry_at {
            self.retry_at = now + 30;
            let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
            if let Ok((c, screen)) = x11rb::connect(Some(&display)) {
                let root = c.setup().roots.get(screen).map(|s| s.root);
                if let Some(root) = root {
                    self.conn = Some((c, root));
                }
            }
        }
        let (c, root) = self.conn.as_ref()?;
        match c
            .screensaver_query_info(*root)
            .ok()
            .and_then(|r| r.reply().ok())
        {
            Some(info) => Some(u64::from(info.ms_since_user_input) / 1000),
            None => {
                self.conn = None;
                None
            }
        }
    }
}

fn log(root: &Path, msg: &str) {
    let path = super::run_dir_at(root).join("power.log");
    let line = format!("{} {msg}\n", now_secs());
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
}

fn write_status(root: &Path, text: &str) {
    let dir = super::run_dir_at(root);
    let tmp = dir.join(format!("power.txt.{}", std::process::id()));
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, dir.join("power.txt"));
    }
}

/// Une seule instance : le PID est noté dans /run/winlux/power-daemon.pid.
fn already_running(root: &Path) -> bool {
    let pid_file = super::run_dir_at(root).join("power-daemon.pid");
    if let Some(pid) = read_trim(&pid_file) {
        if pid != std::process::id().to_string() && root.join("proc").join(&pid).exists() {
            return true;
        }
    }
    let _ = fs::write(pid_file, format!("{}\n", std::process::id()));
    false
}

pub fn run(root: &Path) -> i32 {
    let _ = fs::create_dir_all(super::run_dir_at(root));
    if already_running(root) {
        eprintln!("winlux-power : d\u{e9}j\u{e0} actif");
        return 0;
    }
    let config = Config::load_at(root);
    if let Some(file) = &config.hibernate_file {
        let path = root.join(file.trim_start_matches('/'));
        if let Err(e) = sleep::activate_at(root, &path) {
            log(root, &e);
        }
    }
    let applied = super::apply_profile_at(root, &config);
    log(
        root,
        &format!(
            "profil {} : gouverneur {} sur {} c\u{153}ur(s), {} p\u{e9}riph\u{e9}rique(s)",
            config.profile.key(),
            applied.governor.unwrap_or("inchang\u{e9}"),
            applied.cpus,
            applied.devices
        ),
    );
    touch_activity_at(root);
    let (tx, rx) = mpsc::channel();
    let readers = spawn_readers(root, &tx);
    log(
        root,
        &format!("{readers} p\u{e9}riph\u{e9}rique(s) ACPI surveill\u{e9}(s)"),
    );
    let mut screen_off = false;
    let mut last_text = String::new();
    let mut last_ac = None;
    let mut lid_was_closed = lid_closed_proc(root).unwrap_or(false);
    let mut quiet_until = 0u64;
    let mut critical_done = false;
    let mut last_brightness = super::brightness_at(root);
    // En VM sans batterie, la veille sur inactivité figerait la machine de
    // test : seule la veille demandée explicitement reste possible.
    let virtual_machine = super::is_virtual_at(root);
    let mut x_idle = XIdle {
        conn: None,
        retry_at: 0,
    };
    loop {
        let event = rx.recv_timeout(Duration::from_secs(2)).ok();
        let config = Config::load_at(root);
        let status = read_status_at(root);
        let now = now_secs();
        let has_battery = status.battery.is_some();
        // Évènements matériels (ignorés juste après une reprise : l'ouverture
        // du capot qui réveille ne doit pas relancer une action).
        let mut hw = event;
        if hw.is_none() && readers == 0 {
            if let Some(closed) = lid_closed_proc(root) {
                if closed != lid_was_closed {
                    hw = Some(if closed { Hw::LidClosed } else { Hw::LidOpened });
                }
                lid_was_closed = closed;
            }
        }
        if let Some(ev @ (Hw::BrightnessUp | Hw::BrightnessDown)) = hw {
            hw = None;
            if let Some(p) = brightness_key(root, ev == Hw::BrightnessUp, last_brightness) {
                sleep::event(root, &format!("brightness {p}"));
            }
            touch_activity_at(root);
        }
        if let Some(ev) = hw.filter(|_| now >= quiet_until) {
            let action = match ev {
                Hw::LidClosed => {
                    let docked = external_display_at(root);
                    let action = lid_action(config.lid, docked);
                    if docked && action != config.lid {
                        log(
                            root,
                            "capot ferm\u{e9} avec un \u{e9}cran externe : veille ignor\u{e9}e",
                        );
                    }
                    Some(action)
                }
                Hw::PowerButton => Some(config.button_action(has_battery)),
                Hw::SleepButton => Some(Action::Sleep),
                Hw::BrightnessUp | Hw::BrightnessDown => None,
                Hw::LidOpened => {
                    display_on(root);
                    screen_off = false;
                    touch_activity_at(root);
                    None
                }
            };
            if let Some(action) = action {
                let msg = perform(root, action);
                log(root, &format!("{ev:?} -> {} : {msg}", action.key()));
                if matches!(action, Action::Sleep | Action::Hibernate) {
                    quiet_until = now_secs() + 3;
                    screen_off = false;
                }
            }
        }
        // Branchement/débranchement : l'utilisateur est là.
        if last_ac.is_some_and(|ac| ac != status.ac_online) {
            touch_activity_at(root);
        }
        last_ac = Some(status.ac_online);
        // Batterie critique : veille prolongée (ou veille) pour ne rien perdre.
        if let Some(b) = &status.battery {
            if b.state == Charge::Discharging && b.percent <= 3 && !status.ac_online {
                if !critical_done {
                    critical_done = true;
                    let msg = perform(root, Action::Hibernate);
                    log(
                        root,
                        &format!(
                            "batterie critique ({} %, {}, {}) : {msg}",
                            b.percent,
                            b.state.label(),
                            b.seconds_left.map_or_else(
                                || "autonomie inconnue".into(),
                                super::format_duration
                            )
                        ),
                    );
                }
            } else if b.percent > 5 {
                critical_done = false;
            }
        }
        // Inactivité.
        let last = last_activity_at(root).unwrap_or(now);
        let mut idle = now.saturating_sub(last);
        if let Some(x) = x_idle.secs(now) {
            idle = idle.min(x);
        }
        let caps = sleep::capabilities_at(root);
        match idle_step(
            idle,
            config.effective_timeouts(status.ac_online),
            active_inhibits_at(root),
            screen_off,
            caps.suspend && !(virtual_machine && !has_battery),
        ) {
            IdleStep::ScreenOff => {
                display_off(root);
                screen_off = true;
            }
            IdleStep::ScreenOn => {
                display_on(root);
                screen_off = false;
            }
            IdleStep::Sleep => {
                let msg = perform(root, Action::Sleep);
                log(root, &format!("inactivit\u{e9} {idle} s : {msg}"));
                quiet_until = now_secs() + 3;
                screen_off = false;
            }
            IdleStep::None => {}
        }
        // Valeur de référence pour reconnaître un pas déjà fait par le noyau.
        last_brightness = super::brightness_at(root);
        let text = status_lines(&status, &config, root);
        if text != last_text {
            write_status(root, &text);
            last_text = text;
        }
    }
}
