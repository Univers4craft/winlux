//! Écrans multiples, écrit de zéro : `winlux --display ...`.
//!
//! Le serveur X est piloté par l'outil standard `xrandr` (la plomberie) ;
//! toute la politique est ici et testée sans serveur :
//! - liste des écrans (`xrandr --query`), dalle interne reconnue (eDP, LVDS,
//!   DSI) ;
//! - disposition « étendre », « dupliquer » ou « un seul écran », écran
//!   principal, définition et mise à l'échelle par écran ;
//! - une configuration mémorisée par ensemble d'écrans branchés : on
//!   retrouve son installation de bureau en rebranchant le même écran ;
//! - branchement à chaud : `watch` surveille `/sys/class/drm` (sans sonder
//!   les écrans en boucle) et le capot ; capot fermé avec un écran externe,
//!   la dalle interne est éteinte.
//!
//! Mise à l'échelle par écran sous X11 : le bureau dessine à l'échelle de la
//! session (la plus grande demandée) et chaque écran moins dense est réduit
//! par une transformation RandR (`--scale`), comme les bureaux Linux en X11.
//!
//! Les fichiers sont relatifs à `WINLUX_CONTROL_ROOT` (tests).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// Échelles proposées dans les Paramètres (comme Windows).
pub const SCALES: [&str; 7] = ["1", "1.25", "1.5", "1.75", "2", "2.5", "3"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mode {
    pub w: u32,
    pub h: u32,
    pub current: bool,
    pub preferred: bool,
}

impl Mode {
    pub fn name(&self) -> String {
        format!("{}x{}", self.w, self.h)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Output {
    pub name: String,
    pub connected: bool,
    pub primary: bool,
    /// Zone occupée dans l'écran X (largeur, hauteur, x, y), si allumé.
    pub geometry: Option<(u32, u32, i32, i32)>,
    /// Taille physique en millimètres, si l'EDID la donne.
    pub mm: Option<(u32, u32)>,
    pub modes: Vec<Mode>,
}

impl Output {
    /// Dalle intégrée d'un portable.
    pub fn internal(&self) -> bool {
        ["eDP", "LVDS", "DSI"]
            .iter()
            .any(|p| self.name.starts_with(p))
    }
    pub fn current_mode(&self) -> Option<&Mode> {
        self.modes.iter().find(|m| m.current)
    }
    pub fn preferred_mode(&self) -> Option<&Mode> {
        self.modes
            .iter()
            .find(|m| m.preferred)
            .or_else(|| self.modes.first())
    }
    pub fn find_mode(&self, name: &str) -> Option<&Mode> {
        self.modes.iter().find(|m| m.name() == name)
    }
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// `1920x1080+1920+0` → (1920, 1080, 1920, 0).
fn parse_geometry(s: &str) -> Option<(u32, u32, i32, i32)> {
    let (size, rest) = s.split_once('+')?;
    let (x, y) = rest.split_once('+')?;
    let (w, h) = parse_size(size)?;
    Some((w, h, x.parse().ok()?, y.parse().ok()?))
}

/// Analyse la sortie de `xrandr --query`.
pub fn parse_query(text: &str) -> Vec<Output> {
    let mut outputs: Vec<Output> = Vec::new();
    for line in text.lines() {
        if line.starts_with("Screen ") {
            continue;
        }
        if !line.starts_with(char::is_whitespace) {
            let mut words = line.split_whitespace();
            let Some(name) = words.next() else { continue };
            let state = words.next().unwrap_or("");
            if state != "connected" && state != "disconnected" {
                continue;
            }
            let rest: Vec<&str> = words.collect();
            let mut out = Output {
                name: name.to_string(),
                connected: state == "connected",
                primary: rest.first() == Some(&"primary"),
                ..Output::default()
            };
            out.geometry = rest.iter().find_map(|w| parse_geometry(w));
            // « ... 344mm x 194mm » à la fin de la ligne.
            if let Some(i) = rest.iter().rposition(|w| *w == "x") {
                let mm = |w: Option<&&str>| w?.strip_suffix("mm")?.parse::<u32>().ok();
                if let (Some(w), Some(h)) = (mm(rest.get(i.wrapping_sub(1))), mm(rest.get(i + 1))) {
                    if w > 0 && h > 0 {
                        out.mm = Some((w, h));
                    }
                }
            }
            outputs.push(out);
            continue;
        }
        let Some(out) = outputs.last_mut() else {
            continue;
        };
        let mut words = line.split_whitespace();
        let Some(first) = words.next() else { continue };
        // Lignes de mode : « 1920x1080  60.02*+  59.97 » ; on ignore
        // l'entrelacé et les lignes détaillées de --verbose.
        let Some((w, h)) = parse_size(first) else {
            continue;
        };
        let rates: Vec<&str> = words.collect();
        let current = rates.iter().any(|r| r.contains('*'));
        let preferred = rates.iter().any(|r| r.contains('+'));
        if let Some(m) = out.modes.iter_mut().find(|m| m.w == w && m.h == h) {
            m.current |= current;
            m.preferred |= preferred;
        } else {
            out.modes.push(Mode {
                w,
                h,
                current,
                preferred,
            });
        }
    }
    outputs
}

// ───────────────────────────── Échelle ─────────────────────────────

pub fn parse_scale(s: &str) -> Option<f64> {
    SCALES.contains(&s).then(|| s.parse().ok()).flatten()
}

pub fn scale_label(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Échelle recommandée d'après la densité réelle (comme Windows : un
/// portable 14 pouces Full HD est proposé à 150 %). Sans taille physique
/// (VM), on garde l'ancienne règle d'après la hauteur.
pub fn auto_scale(mode: (u32, u32), mm: Option<(u32, u32)>) -> f64 {
    match mm {
        // Les EDID farfelus (projecteurs, 160x90 mm pour un téléviseur...)
        // sont écartés : moins de 10 cm de large n'est pas un écran.
        Some((w_mm, _)) if w_mm >= 100 => {
            let dpi = f64::from(mode.0) * 25.4 / f64::from(w_mm);
            match dpi {
                d if d < 120.0 => 1.0,
                d if d < 144.0 => 1.25,
                d if d < 168.0 => 1.5,
                d if d < 192.0 => 1.75,
                d if d < 240.0 => 2.0,
                d if d < 288.0 => 2.5,
                _ => 3.0,
            }
        }
        _ => match mode.1 {
            h if h >= 2000 => 2.0,
            h if h >= 1400 => 1.5,
            h if h >= 1000 => 1.25,
            _ => 1.0,
        },
    }
}

// ───────────────────────────── Réglages ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Right,
    Left,
    Above,
    Below,
}

impl Place {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "right" => Self::Right,
            "left" => Self::Left,
            "above" => Self::Above,
            "below" => Self::Below,
            _ => return None,
        })
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
            Self::Above => "above",
            Self::Below => "below",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    Extend,
    Duplicate,
    Only(String),
}

impl Layout {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "extend" => Some(Self::Extend),
            "duplicate" => Some(Self::Duplicate),
            _ => s
                .strip_prefix("only:")
                .filter(|n| valid_name(n))
                .map(|n| Self::Only(n.to_string())),
        }
    }
    pub fn key(&self) -> String {
        match self {
            Self::Extend => "extend".into(),
            Self::Duplicate => "duplicate".into(),
            Self::Only(n) => format!("only:{n}"),
        }
    }
}

/// Noms de sortie RandR : lettres, chiffres, tirets (HDMI-A-1, DP-1-2...).
pub fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 32
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Fichier `display.conf` : clés simples conservées telles quelles
/// (`POLICY`, `MODE`, `UI_SCALE` des versions précédentes) et une
/// configuration par ensemble d'écrans (`display.<écrans>.<champ>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub values: BTreeMap<String, String>,
}

impl Config {
    pub fn parse(text: &str) -> Self {
        let values = text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, _)| !k.is_empty())
            .collect();
        Self { values }
    }
    pub fn render(&self) -> String {
        self.values
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect()
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_string(), value.to_string());
    }
    fn pkey(profile: &str, field: &str, output: Option<&str>) -> String {
        match output {
            Some(o) => format!("display.{profile}.{field}.{o}"),
            None => format!("display.{profile}.{field}"),
        }
    }
    pub fn pget(&self, profile: &str, field: &str, output: Option<&str>) -> Option<&str> {
        self.get(&Self::pkey(profile, field, output))
    }
    pub fn pset(&mut self, profile: &str, field: &str, output: Option<&str>, value: &str) {
        self.set(&Self::pkey(profile, field, output), value);
    }
}

/// Identifiant d'un ensemble d'écrans : noms branchés, triés.
pub fn profile_id(outputs: &[Output]) -> String {
    let mut names: Vec<&str> = outputs
        .iter()
        .filter(|o| o.connected)
        .map(|o| o.name.as_str())
        .collect();
    names.sort_unstable();
    names.join("+")
}

/// Ce qui est voulu pour l'ensemble d'écrans branché.
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    pub layout: Layout,
    pub primary: Option<String>,
    pub modes: BTreeMap<String, String>,
    pub places: BTreeMap<String, Place>,
    pub scales: BTreeMap<String, f64>,
    /// Une seule sortie, politique « suivre la fenêtre VM » (mode préféré).
    pub follow: bool,
}

pub fn wanted(config: &Config, outputs: &[Output]) -> Wanted {
    let id = profile_id(outputs);
    let connected: Vec<&Output> = outputs.iter().filter(|o| o.connected).collect();
    let single = connected.len() == 1;
    let mut w = Wanted {
        layout: config
            .pget(&id, "layout", None)
            .and_then(Layout::parse)
            .unwrap_or(Layout::Extend),
        primary: config.pget(&id, "primary", None).map(str::to_string),
        modes: BTreeMap::new(),
        places: BTreeMap::new(),
        scales: BTreeMap::new(),
        follow: single && config.get("POLICY") != Some("fixed"),
    };
    let global_scale = config.get("UI_SCALE").and_then(parse_scale);
    for o in &connected {
        let name = o.name.as_str();
        let mode = config.pget(&id, "mode", Some(name)).or_else(|| {
            // Ancien réglage « définition fixe » d'un seul écran.
            (single && config.get("POLICY") == Some("fixed"))
                .then(|| config.get("MODE"))
                .flatten()
        });
        if let Some(m) = mode.filter(|m| o.find_mode(m).is_some()) {
            w.modes.insert(name.to_string(), m.to_string());
        }
        if let Some(p) = config.pget(&id, "place", Some(name)).and_then(Place::parse) {
            w.places.insert(name.to_string(), p);
        }
        // Sinon l'échelle choisie pour cet écran dans un autre ensemble
        // (l'interne garde ses 150 % quand on branche un écran).
        let suffix = format!(".scale.{name}");
        let scale = config
            .pget(&id, "scale", Some(name))
            .and_then(parse_scale)
            .or_else(|| {
                config
                    .values
                    .iter()
                    .find(|(k, _)| k.starts_with("display.") && k.ends_with(&suffix))
                    .and_then(|(_, v)| parse_scale(v))
            })
            .or(if single { global_scale } else { None });
        if let Some(s) = scale {
            w.scales.insert(name.to_string(), s);
        }
    }
    w
}

/// Sorties réellement utilisables : capot fermé avec un écran externe, la
/// dalle interne est mise de côté.
pub fn usable(outputs: &[Output], lid_closed: bool) -> Vec<&Output> {
    let connected: Vec<&Output> = outputs.iter().filter(|o| o.connected).collect();
    let external = connected.iter().any(|o| !o.internal());
    connected
        .into_iter()
        .filter(|o| !(lid_closed && external && o.internal()))
        .collect()
}

/// Échelle voulue pour une sortie (réglage ou recommandation).
pub fn scale_for(w: &Wanted, o: &Output) -> f64 {
    if let Some(s) = w.scales.get(&o.name) {
        return *s;
    }
    let mode = chosen_mode(w, o).map_or((0, 0), |m| (m.w, m.h));
    auto_scale(mode, o.mm)
}

fn chosen_mode<'a>(w: &Wanted, o: &'a Output) -> Option<&'a Mode> {
    if w.follow {
        return o.preferred_mode();
    }
    w.modes
        .get(&o.name)
        .and_then(|m| o.find_mode(m))
        .or_else(|| o.preferred_mode())
}

/// Échelle de la session : la plus grande demandée parmi les écrans
/// allumés (les autres sont réduits par RandR, jamais agrandis en flou).
pub fn session_scale(w: &Wanted, outputs: &[Output], lid_closed: bool) -> f64 {
    let active = active_outputs(w, outputs, lid_closed);
    active
        .iter()
        .map(|o| scale_for(w, o))
        .fold(0.0, f64::max)
        .max(1.0)
}

fn active_outputs<'a>(w: &Wanted, outputs: &'a [Output], lid_closed: bool) -> Vec<&'a Output> {
    let list = usable(outputs, lid_closed);
    if let Layout::Only(name) = &w.layout {
        if let Some(o) = list.iter().find(|o| &o.name == name) {
            return vec![*o];
        }
    }
    list
}

/// Ce qu'une sortie doit devenir.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub name: String,
    /// `None` : éteinte.
    pub on: Option<On>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct On {
    pub mode: (u32, u32),
    /// Facteur RandR (`--scale f x f`) : > 1 réduit l'image.
    pub factor: f64,
    pub x: i32,
    pub y: i32,
    pub primary: bool,
    /// Duplication d'une définition différente : `--scale-from`.
    pub scale_from: Option<(u32, u32)>,
}

impl On {
    /// Taille occupée dans l'écran X.
    pub fn size(&self) -> (u32, u32) {
        match self.scale_from {
            Some(s) => s,
            None => (
                (f64::from(self.mode.0) * self.factor).round() as u32,
                (f64::from(self.mode.1) * self.factor).round() as u32,
            ),
        }
    }
}

/// Calcule la disposition complète pour l'échelle de session `g`.
pub fn plan(w: &Wanted, outputs: &[Output], lid_closed: bool, g: f64) -> Vec<Target> {
    let active = active_outputs(w, outputs, lid_closed);
    let primary = w
        .primary
        .as_deref()
        .and_then(|p| active.iter().find(|o| o.name == p))
        .or_else(|| active.iter().find(|o| o.internal()))
        .or_else(|| active.first())
        .map(|o| o.name.clone());
    let mut on: BTreeMap<String, On> = BTreeMap::new();
    let duplicate = w.layout == Layout::Duplicate && active.len() > 1;
    if duplicate {
        // Plus grande définition commune, sinon celle du principal et
        // les autres adaptés par --scale-from.
        let common = active[0]
            .modes
            .iter()
            .filter(|m| {
                active
                    .iter()
                    .all(|o| o.modes.iter().any(|n| n.w == m.w && n.h == m.h))
            })
            .max_by_key(|m| m.w * m.h)
            .map(|m| (m.w, m.h));
        let pri = active
            .iter()
            .find(|o| Some(&o.name) == primary.as_ref())
            .copied()
            .unwrap_or(active[0]);
        let pri_mode = common
            .or_else(|| chosen_mode(w, pri).map(|m| (m.w, m.h)))
            .unwrap_or((1024, 768));
        let factor = g / scale_for(w, pri);
        let area = (
            (f64::from(pri_mode.0) * factor).round() as u32,
            (f64::from(pri_mode.1) * factor).round() as u32,
        );
        for o in &active {
            let mode = if common.is_some() {
                pri_mode
            } else {
                chosen_mode(w, o).map_or(pri_mode, |m| (m.w, m.h))
            };
            on.insert(
                o.name.clone(),
                On {
                    mode,
                    factor,
                    x: 0,
                    y: 0,
                    primary: Some(&o.name) == primary.as_ref(),
                    scale_from: (mode != pri_mode || factor != 1.0).then_some(area),
                },
            );
        }
    } else {
        for o in &active {
            let Some(m) = chosen_mode(w, o) else { continue };
            on.insert(
                o.name.clone(),
                On {
                    mode: (m.w, m.h),
                    factor: g / scale_for(w, o),
                    x: 0,
                    y: 0,
                    primary: Some(&o.name) == primary.as_ref(),
                    scale_from: None,
                },
            );
        }
        place(&mut on, primary.as_deref(), &w.places, &active);
    }
    outputs
        .iter()
        .filter(|o| o.connected || o.geometry.is_some())
        .map(|o| Target {
            name: o.name.clone(),
            on: on.remove(&o.name),
        })
        .collect()
}

/// Place les écrans autour du principal (par défaut à sa droite, alignés en
/// haut comme Windows), puis ramène l'ensemble en coordonnées positives.
fn place(
    on: &mut BTreeMap<String, On>,
    primary: Option<&str>,
    places: &BTreeMap<String, Place>,
    order: &[&Output],
) {
    let Some(primary) = primary else { return };
    let Some((pw, ph)) = on.get(primary).map(On::size) else {
        return;
    };
    let (mut right, mut left, mut top, mut bottom) = (pw as i32, 0i32, 0i32, ph as i32);
    for o in order {
        if o.name == primary {
            continue;
        }
        let Some(t) = on.get_mut(&o.name) else {
            continue;
        };
        let (w, h) = t.size();
        let (w, h) = (w as i32, h as i32);
        match places.get(&o.name).copied().unwrap_or(Place::Right) {
            Place::Right => {
                (t.x, t.y) = (right, 0);
                right += w;
            }
            Place::Left => {
                left -= w;
                (t.x, t.y) = (left, 0);
            }
            Place::Above => {
                top -= h;
                (t.x, t.y) = (0, top);
            }
            Place::Below => {
                (t.x, t.y) = (0, bottom);
                bottom += h;
            }
        }
    }
    let min_x = on.values().map(|t| t.x).min().unwrap_or(0);
    let min_y = on.values().map(|t| t.y).min().unwrap_or(0);
    for t in on.values_mut() {
        t.x -= min_x;
        t.y -= min_y;
    }
}

fn factor_arg(f: f64) -> String {
    let s = format!("{f:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}x{s}")
}

/// Vrai si l'état actuel correspond déjà au plan (aucun appel à xrandr :
/// `sync` est appelé souvent et un changement de mode fait clignoter).
pub fn matches(outputs: &[Output], plan: &[Target]) -> bool {
    plan.iter().all(|t| {
        let Some(o) = outputs.iter().find(|o| o.name == t.name) else {
            return true;
        };
        match &t.on {
            None => o.geometry.is_none(),
            Some(on) => {
                let (w, h) = on.size();
                o.geometry == Some((w, h, on.x, on.y))
                    && o.current_mode().map(|m| (m.w, m.h)) == Some(on.mode)
                    && o.primary == on.primary
            }
        }
    })
}

/// Arguments d'un seul appel `xrandr` (atomique côté serveur).
pub fn xrandr_args(plan: &[Target]) -> Vec<String> {
    let mut args = Vec::new();
    // Éteindre d'abord libère les contrôleurs (CRTC) pour les autres.
    for t in plan.iter().filter(|t| t.on.is_none()) {
        args.extend(["--output".into(), t.name.clone(), "--off".into()]);
    }
    for t in plan {
        let Some(on) = &t.on else { continue };
        args.extend([
            "--output".into(),
            t.name.clone(),
            "--mode".into(),
            format!("{}x{}", on.mode.0, on.mode.1),
            "--pos".into(),
            format!("{}x{}", on.x, on.y),
            "--rotate".into(),
            "normal".into(),
        ]);
        match on.scale_from {
            Some((w, h)) => args.extend(["--scale-from".into(), format!("{w}x{h}")]),
            None => args.extend(["--scale".into(), factor_arg(on.factor)]),
        }
        if on.primary {
            args.push("--primary".into());
        }
    }
    args
}

// ───────────────────────────── Système ─────────────────────────────

pub fn root() -> PathBuf {
    std::env::var_os("WINLUX_CONTROL_ROOT")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn config_path(root: &Path) -> PathBuf {
    root.join("etc/winlux/display.conf")
}

fn run_dir(root: &Path) -> PathBuf {
    root.join("run/winlux")
}

pub fn load(root: &Path) -> Config {
    Config::parse(&fs::read_to_string(config_path(root)).unwrap_or_default())
}

pub fn save(root: &Path, config: &Config) -> Result<(), String> {
    let path = config_path(root);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension(format!("new.{}", std::process::id()));
    fs::write(&tmp, config.render()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Capot fermé selon l'ACPI.
pub fn lid_closed(root: &Path) -> bool {
    let Ok(dir) = fs::read_dir(root.join("proc/acpi/button/lid")) else {
        return false;
    };
    dir.flatten()
        .any(|e| fs::read_to_string(e.path().join("state")).is_ok_and(|s| s.contains("closed")))
}

/// Signature des connecteurs DRM et du capot : change au branchement, au
/// débranchement et quand une VM annonce une nouvelle taille de fenêtre.
pub fn hardware_signature(root: &Path) -> String {
    let mut parts = Vec::new();
    if let Ok(dir) = fs::read_dir(root.join("sys/class/drm")) {
        let mut entries: Vec<PathBuf> = dir.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let status = fs::read_to_string(p.join("status")).unwrap_or_default();
            if status.is_empty() {
                continue;
            }
            let modes = fs::read_to_string(p.join("modes")).unwrap_or_default();
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
            parts.push(format!(
                "{}={}:{}",
                name.unwrap_or_default(),
                status.trim(),
                modes.lines().next().unwrap_or("")
            ));
        }
    }
    parts.push(format!("lid={}", lid_closed(root)));
    parts.join(";")
}

fn xrandr(args: &[String]) -> Result<String, String> {
    let out = std::process::Command::new("xrandr")
        .args(args)
        .env(
            "DISPLAY",
            std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
        )
        .output()
        .map_err(|e| format!("xrandr indisponible ({e})"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn query() -> Result<Vec<Output>, String> {
    let text =
        xrandr(&["--query".into()]).map_err(|_| "Affichage Xorg indisponible.".to_string())?;
    let outputs = parse_query(&text);
    if !outputs.iter().any(|o| o.connected) {
        return Err("Aucun \u{e9}cran connect\u{e9}.".into());
    }
    Ok(outputs)
}

/// Échelle avec laquelle le bureau de la session a été lancé.
pub fn current_session_scale(root: &Path) -> Option<f64> {
    fs::read_to_string(run_dir(root).join("display-session-scale"))
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .or_else(|| {
            std::env::var("WINLUX_UI_SCALE")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .filter(|v: &f64| v.is_finite() && *v >= 1.0)
}

/// Applique la configuration mémorisée ; `Ok(false)` si rien ne change.
pub fn sync(root: &Path) -> Result<bool, String> {
    let outputs = query()?;
    let config = load(root);
    let w = wanted(&config, &outputs);
    let lid = lid_closed(root);
    let g = current_session_scale(root).unwrap_or_else(|| session_scale(&w, &outputs, lid));
    let p = plan(&w, &outputs, lid, g);
    if p.iter().all(|t| t.on.is_none()) || matches(&outputs, &p) {
        return Ok(false);
    }
    xrandr(&xrandr_args(&p))?;
    let dir = run_dir(root);
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(
            dir.join("display-changed"),
            format!("{}\n", profile_id(&outputs)),
        );
    }
    Ok(true)
}

/// Lignes `display_*` lues par les Paramètres (ASCII uniquement).
pub fn status_lines(root: &Path, outputs: &[Output], config: &Config) -> String {
    let w = wanted(config, outputs);
    let lid = lid_closed(root);
    let g = current_session_scale(root).unwrap_or_else(|| session_scale(&w, outputs, lid));
    let wanted_g = session_scale(&w, outputs, lid);
    let p = plan(&w, outputs, lid, g);
    let connected: Vec<&Output> = outputs.iter().filter(|o| o.connected).collect();
    let primary = p
        .iter()
        .find(|t| t.on.as_ref().is_some_and(|o| o.primary))
        .and_then(|t| connected.iter().find(|o| o.name == t.name))
        .or(connected.first())
        .copied();
    let mut s = String::new();
    if let Some(o) = primary {
        let modes: Vec<String> = o.modes.iter().map(Mode::name).collect();
        s.push_str(&format!(
            "display_output={}\ndisplay_current={}\ndisplay_modes={}\n",
            o.name,
            o.current_mode().map(Mode::name).unwrap_or_default(),
            modes.join(",")
        ));
    }
    s.push_str(&format!(
        "display_policy={}\ndisplay_scale={}\n",
        config.get("POLICY").unwrap_or(""),
        config.get("UI_SCALE").unwrap_or("")
    ));
    s.push_str(&format!(
        "display_virtual={}\n",
        u8::from(crate::power::is_virtual_at(&crate::power::root()))
    ));
    s.push_str(&format!(
        "display_count={}\ndisplay_layout={}\ndisplay_lid_closed={}\ndisplay_session_scale={}\ndisplay_restart_hint={}\n",
        connected.len(),
        w.layout.key(),
        u8::from(lid),
        scale_label(g),
        u8::from((wanted_g - g).abs() > 0.01)
    ));
    for (i, o) in connected.iter().enumerate().take(4) {
        let t = p.iter().find(|t| t.name == o.name);
        let on = t.and_then(|t| t.on.as_ref());
        let modes: Vec<String> = o.modes.iter().map(Mode::name).collect();
        let n = i + 1;
        s.push_str(&format!(
            "display_{n}_name={}\ndisplay_{n}_internal={}\ndisplay_{n}_active={}\ndisplay_{n}_primary={}\n",
            o.name,
            u8::from(o.internal()),
            u8::from(on.is_some()),
            u8::from(on.is_some_and(|o| o.primary)),
        ));
        s.push_str(&format!(
            "display_{n}_mode={}\ndisplay_{n}_modes={}\ndisplay_{n}_scale={}\ndisplay_{n}_scale_auto={}\ndisplay_{n}_place={}\n",
            chosen_mode(&w, o).map(Mode::name).unwrap_or_default(),
            modes.join(","),
            scale_label(scale_for(&w, o)),
            u8::from(!w.scales.contains_key(&o.name)),
            w.places.get(&o.name).copied().unwrap_or(Place::Right).key(),
        ));
        if let Some(on) = on {
            let (ww, hh) = on.size();
            s.push_str(&format!(
                "display_{n}_rect={},{},{},{}\n",
                on.x, on.y, ww, hh
            ));
        }
    }
    s
}

fn connected_name<'a>(outputs: &'a [Output], name: &str) -> Result<&'a Output, String> {
    outputs
        .iter()
        .find(|o| o.connected && o.name == name)
        .ok_or_else(|| format!("\u{c9}cran inconnu : {name}"))
}

/// Applique une commande de réglage à la configuration (sans X).
pub fn apply_command(
    config: &mut Config,
    outputs: &[Output],
    args: &[&str],
) -> Result<String, String> {
    let id = profile_id(outputs);
    let single = outputs.iter().filter(|o| o.connected).count() == 1;
    let primary = outputs
        .iter()
        .filter(|o| o.connected)
        .find(|o| o.primary)
        .or_else(|| outputs.iter().find(|o| o.connected))
        .map(|o| o.name.clone())
        .unwrap_or_default();
    match args {
        ["follow"] => {
            config.set("POLICY", "follow");
            Ok("La d\u{e9}finition suit l'\u{e9}cran.".into())
        }
        ["fixed"] => {
            if let Some(m) = connected_name(outputs, &primary)?.current_mode() {
                config.set("MODE", &m.name());
                config.pset(&id, "mode", Some(&primary), &m.name());
            }
            config.set("POLICY", "fixed");
            Ok("D\u{e9}finition fix\u{e9}e.".into())
        }
        ["mode", mode] => apply_command(config, outputs, &["mode", &primary, mode]),
        ["mode", name, mode] => {
            let o = connected_name(outputs, name)?;
            if o.find_mode(mode).is_none() {
                return Err("D\u{e9}finition non propos\u{e9}e par cet \u{e9}cran.".into());
            }
            config.pset(&id, "mode", Some(name), mode);
            if single {
                config.set("MODE", mode);
                config.set("POLICY", "fixed");
            }
            Ok(format!("{name} : {mode}"))
        }
        ["scale", value] if single => apply_command(config, outputs, &["scale", &primary, value]),
        ["scale", value] => {
            // Ancienne commande globale : tous les écrans branchés.
            let names: Vec<String> = outputs
                .iter()
                .filter(|o| o.connected)
                .map(|o| o.name.clone())
                .collect();
            for n in names {
                apply_command(config, outputs, &["scale", &n, value])?;
            }
            Ok(format!("\u{c9}chelle : {value}"))
        }
        ["scale", name, value] => {
            connected_name(outputs, name)?;
            let key = format!("display.{id}.scale.{name}");
            if *value == "auto" {
                config.values.remove(&key);
            } else if parse_scale(value).is_some() {
                config.set(&key, value);
            } else {
                return Err(format!("\u{e9}chelle invalide : {value}"));
            }
            if single {
                config.set("UI_SCALE", value);
            }
            Ok(format!("{name} : \u{e9}chelle {value}"))
        }
        ["scale-step", name, dir] => {
            let o = connected_name(outputs, name)?;
            let w = wanted(config, outputs);
            let cur = scale_label(scale_for(&w, o));
            let i = SCALES.iter().position(|s| *s == cur).unwrap_or(0);
            let j = match *dir {
                "up" => (i + 1).min(SCALES.len() - 1),
                "down" => i.saturating_sub(1),
                _ => return Err("sens invalide".into()),
            };
            apply_command(config, outputs, &["scale", name, SCALES[j]])
        }
        ["layout", "only", name] => {
            connected_name(outputs, name)?;
            config.pset(&id, "layout", None, &format!("only:{name}"));
            Ok(format!("Seulement {name}"))
        }
        ["layout", l @ ("extend" | "duplicate")] => {
            config.pset(&id, "layout", None, l);
            Ok(format!("Disposition : {l}"))
        }
        ["primary", name] => {
            connected_name(outputs, name)?;
            config.pset(&id, "primary", None, name);
            Ok(format!("\u{c9}cran principal : {name}"))
        }
        ["place", name, side] => {
            connected_name(outputs, name)?;
            let p = Place::parse(side).ok_or(format!("position invalide : {side}"))?;
            config.pset(&id, "place", Some(name), p.key());
            Ok(format!("{name} : {}", p.key()))
        }
        _ => Err(USAGE.into()),
    }
}

const USAGE: &str = "usage: winlux --display status | sync | watch | session-scale\n\
  | layout extend|duplicate | layout only ECRAN | primary ECRAN\n\
  | mode [ECRAN] LxH | place ECRAN right|left|above|below\n\
  | scale [ECRAN] 1|1.25|1.5|1.75|2|2.5|3|auto | scale-step ECRAN up|down\n\
  | follow | fixed";

/// Boucle de surveillance (lancée par la session) : réapplique la
/// configuration au branchement, au débranchement, à la fermeture du capot
/// et, en VM, quand la fenêtre change de taille.
pub fn watch(root: &Path) -> i32 {
    let mut last = String::new();
    let mut ticks = 0u32;
    let mut lost = 0u32;
    // En VM, certains pilotes ne publient pas la nouvelle taille de fenêtre
    // dans sysfs : repli toutes les 2 s. Sur une vraie machine, on ne sonde
    // jamais les écrans en boucle (lecture EDID, clignotements).
    let virtual_machine = crate::power::is_virtual_at(&crate::power::root());
    loop {
        let sig = hardware_signature(root);
        if sig != last || ticks == 0 || virtual_machine && ticks.is_multiple_of(2) {
            match sync(root) {
                Ok(_) => lost = 0,
                // Serveur X parti (fin de session) : on s'arrête, la session
                // suivante relance la surveillance.
                Err(e) if e.starts_with("Affichage Xorg") => {
                    lost += 1;
                    if lost >= 3 {
                        return 1;
                    }
                }
                Err(e) => {
                    if sig != last {
                        eprintln!("winlux-display : {e}");
                    }
                }
            }
            last = sig;
        }
        if ticks % 5 == 4 && x11rb::connect(None).is_err() {
            return 1;
        }
        ticks = ticks.wrapping_add(1);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

pub fn main(args: &[String]) -> i32 {
    let root = root();
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    match argv.as_slice() {
        [] | ["status"] => match query() {
            Ok(outputs) => {
                print!("{}", status_lines(&root, &outputs, &load(&root)));
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        ["sync"] => match sync(&root) {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        ["watch"] => watch(&root),
        ["session-scale"] => {
            // Au lancement du bureau : la plus grande échelle demandée.
            let g = query().map_or(1.0, |outputs| {
                let w = wanted(&load(&root), &outputs);
                session_scale(&w, &outputs, lid_closed(&root))
            });
            let dir = run_dir(&root);
            if fs::create_dir_all(&dir).is_ok() {
                let _ = fs::write(
                    dir.join("display-session-scale"),
                    format!("{}\n", scale_label(g)),
                );
            }
            println!("{}", scale_label(g));
            0
        }
        _ => {
            let outputs = match query() {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            };
            let mut config = load(&root);
            match apply_command(&mut config, &outputs, &argv) {
                Ok(msg) => {
                    if let Err(e) = save(&root, &config) {
                        eprintln!("Enregistrement impossible : {e}");
                        return 1;
                    }
                    if let Err(e) = sync(&root) {
                        eprintln!("{e}");
                        return 1;
                    }
                    println!("{msg}");
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    2
                }
            }
        }
    }
}
