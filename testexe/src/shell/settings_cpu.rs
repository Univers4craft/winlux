//! Paramètres > Processeur : fréquences, mode d'alimentation, P/E-cores,
//! températures, ventilateurs, NUMA et réglages processeur par programme.
//!
//! Garde-fous : tout vient de `/run/winlux/cpu.txt`, écrit par
//! `winlux --cpu-report` à partir du matériel réel. Une section n'est
//! dessinée (et cliquable) que si son `cap_*` vaut 1 ; un bouton n'existe que
//! pour une option applicable. Rien n'est jamais grisé : soit cela marche,
//! soit ce n'est pas proposé.

use crate::gfx::*;
use crate::wm::Gfx;
use winlux_testexe::*;

const INFO_CAP: usize = 16384;
static mut INFO: [u8; INFO_CAP] = [0; INFO_CAP];
static mut INFO_LEN: usize = 0;
static mut LAST_READ: u32 = 0;
/// Fréquences relues en direct (MHz) pour les 64 premiers processeurs.
static mut LIVE_MHZ: [u32; 64] = [0; 64];
static mut LIVE_TEMP: i32 = -1;
static mut SCROLL_PX: i32 = 0;
static mut CONTENT_H: i32 = 0;
static mut HOVER: i32 = -1;
static mut THROTTLE_LAST: u32 = u32::MAX;
static mut THROTTLE_AT: u32 = 0;
static mut FOCUS_PROGRAM: bool = false;
static mut PROGRAM_Y: i32 = 0;

/// Réglages processeur du programme ouvert dans « Réglages de l'exécutable ».
pub static mut EXE_PRIORITY: u8 = 0; // 0 = par défaut, sinon index de PRIORITIES + 1
pub static mut EXE_CORES: u8 = 0; // 0 auto, 1 P-cores, 2 E-cores
pub static mut EXE_AFFINITY: [u64; 4] = [0; 4]; // vide = tous les processeurs
pub static mut EXE_NUMA: i32 = -1;
pub static mut NOTICE: &[u8] = b"";

const PRIORITIES: [(&[u8], &[u8], &[u8]); 4] = [
    (b"low", b"Basse", b"Low"),
    (b"normal", b"Normale", b"Normal"),
    (b"high", b"Haute", b"High"),
    (b"realtime", b"Temps r\xE9el", b"Realtime"),
];

// Identifiants des zones cliquables.
const ID_PROFILE: i32 = 100;
const ID_HYBRID: i32 = 110;
const ID_FAN: i32 = 120;
const ID_PRIORITY: i32 = 130;
const ID_CORES: i32 = 140;
const ID_ALL_CPUS: i32 = 150;
const ID_NUMA: i32 = 160;
const ID_SAVE: i32 = 180;
const ID_LAUNCH: i32 = 181;
const ID_EXPLORER: i32 = 182;
const ID_RESET: i32 = 183;
const ID_CHIP: i32 = 1000;

fn en() -> bool {
    unsafe { crate::settings::status_value(b"locale_lang") == b"en" }
}
fn t(fr: &'static [u8], en_text: &'static [u8]) -> &'static [u8] {
    if en() {
        en_text
    } else {
        fr
    }
}

fn info(key: &[u8]) -> &'static [u8] {
    unsafe { crate::settings::field(core::slice::from_raw_parts(INFO.as_ptr(), INFO_LEN), key) }
}
fn cap(key: &[u8]) -> bool {
    info(key) == b"1"
}
/// Les données processeur sont présentes (service et runtime disponibles).
pub fn available() -> bool {
    unsafe { INFO_LEN > 0 }
}

fn num(s: &[u8]) -> u32 {
    crate::settings::number(s)
}

/// Petite chaîne à pile fixe pour composer les libellés.
struct Line {
    b: [u8; 200],
    n: usize,
}
impl Line {
    fn new() -> Self {
        Line { b: [0; 200], n: 0 }
    }
    fn push(&mut self, s: &[u8]) -> &mut Self {
        let k = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s[..k]);
        self.n += k;
        self
    }
    fn num(&mut self, mut v: u32) -> &mut Self {
        let mut tmp = [0u8; 10];
        let mut i = tmp.len();
        loop {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        self.push(&tmp[i..])
    }
    fn get(&self) -> &[u8] {
        &self.b[..self.n]
    }
}

/// Itère sur une liste Linux `0-3,8` (bornée à 4096).
fn cpulist_contains(list: &[u8], cpu: u32) -> bool {
    for part in list.split(|&c| c == b',') {
        let mut it = part.split(|&c| c == b'-');
        let a = it.next().map(num).unwrap_or(u32::MAX);
        let b = it.next().map(num).unwrap_or(a);
        if !part.is_empty() && a <= cpu && cpu <= b {
            return true;
        }
    }
    false
}

/// Processeurs listés dans `freq=` ou `cpu_list=` (dans l'ordre).
fn for_each_cpu(mut f: impl FnMut(usize, u32, u32)) {
    let freq = info(b"freq");
    if !freq.is_empty() {
        for (i, entry) in freq.split(|&c| c == b',').enumerate() {
            let mut it = entry.split(|&c| c == b':');
            let id = num(it.next().unwrap_or(b""));
            let mhz = num(it.next().unwrap_or(b""));
            f(i, id, mhz);
        }
        return;
    }
    let mut i = 0;
    for part in info(b"cpu_list").split(|&c| c == b',') {
        let mut it = part.split(|&c| c == b'-');
        let Some(a) = it.next().filter(|a| !a.is_empty()).map(num) else {
            continue;
        };
        let b = it.next().map(num).unwrap_or(a).min(a + 4096);
        for id in a..=b {
            f(i, id, 0);
            i += 1;
        }
    }
}

unsafe fn read_file(path: &[u8], out: &mut [u8]) -> usize {
    let mut z = [0u8; 160];
    if path.len() + 3 > z.len() {
        return 0;
    }
    z[0] = b'Z';
    z[1] = b':';
    for (i, &c) in path.iter().enumerate() {
        z[2 + i] = if c == b'/' { b'\\' } else { c };
    }
    let h = CreateFileA(
        z.as_ptr().cast(),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if h.is_null() || h as isize == -1 {
        return 0;
    }
    let mut n = 0u32;
    ReadFile(
        h,
        out.as_mut_ptr(),
        out.len() as u32,
        &mut n,
        core::ptr::null_mut(),
    );
    CloseHandle(h);
    (n as usize).min(out.len())
}

/// Relit le rapport (toutes les secondes) et les valeurs vivantes.
pub unsafe fn tick(active: bool) -> bool {
    let now = GetTickCount();
    if LAST_READ != 0 && now.wrapping_sub(LAST_READ) < 1000 {
        return false;
    }
    LAST_READ = now;
    let mut fresh = [0u8; INFO_CAP];
    let n = read_file(b"/run/winlux/cpu.txt", &mut fresh[..INFO_CAP - 1]);
    let mut changed = n != INFO_LEN || fresh[..n] != INFO[..n];
    if changed {
        INFO[..n].copy_from_slice(&fresh[..n]);
        INFO_LEN = n;
    }
    if !active {
        return changed;
    }
    if cap(b"cap_freq") {
        let mut live = [0u32; 64];
        for_each_cpu(|i, id, mhz| {
            if i < 64 {
                let mut path = Line::new();
                path.push(b"/sys/devices/system/cpu/cpu")
                    .num(id)
                    .push(b"/cpufreq/scaling_cur_freq");
                let mut buf = [0u8; 16];
                let k = read_file(path.get(), &mut buf);
                let khz = num(&buf[..k]);
                live[i] = if khz > 0 { khz / 1000 } else { mhz };
            }
        });
        if live != LIVE_MHZ {
            LIVE_MHZ = live;
            changed = true;
        }
    }
    if cap(b"cap_thermal") {
        let mut buf = [0u8; 16];
        let k = read_file(info(b"temp_path"), &mut buf);
        let milli = num(&buf[..k]);
        let temp = if (1000..=125_000).contains(&milli) {
            (milli / 1000) as i32
        } else {
            num(info(b"temp")) as i32
        };
        if temp != LIVE_TEMP {
            LIVE_TEMP = temp;
            changed = true;
        }
    }
    if cap(b"cap_throttle") {
        let count = num(info(b"throttle"));
        if THROTTLE_LAST != u32::MAX && count > THROTTLE_LAST {
            THROTTLE_AT = now;
        }
        THROTTLE_LAST = count;
    }
    changed
}

// --- Mise en page partagée entre le dessin et le clic ----------------------

struct Walk<'a, 'b> {
    g: Option<&'a mut Gfx<'b>>,
    c: Rect,
    y: i32,
    clip_top: i32,
    probe: Option<(i32, i32)>,
    hit: i32,
}

impl Walk<'_, '_> {
    fn screen(&self, y: i32) -> i32 {
        self.c.top + y - unsafe { SCROLL_PX }
    }
    fn visible(&self, y: i32, h: i32) -> bool {
        let s = self.screen(y);
        s >= self.clip_top && s + h <= self.c.bottom - 24
    }
    fn header(&mut self, label: &[u8]) {
        self.y += 14;
        if self.visible(self.y, 26) {
            let y = self.screen(self.y);
            if let Some(g) = self.g.as_deref_mut() {
                unsafe {
                    text(g.hdc, g.f.semibold, pal().text, self.c.left, y, label);
                    fill(
                        g.hdc,
                        g.b,
                        self.c.left,
                        y + 22,
                        self.c.left + 28,
                        y + 24,
                        pal().accent,
                    );
                }
            }
        }
        self.y += 34;
    }
    fn row(&mut self, label: &[u8], value: &[u8]) {
        if self.visible(self.y - 7, 52) {
            let y = self.screen(self.y);
            if let Some(g) = self.g.as_deref_mut() {
                let p = pal();
                let c = &self.c;
                unsafe {
                    round_rect(
                        g.hdc,
                        g.b,
                        c.left - 8,
                        y - 7,
                        c.right + 8,
                        y + 45,
                        8,
                        p.border,
                    );
                    round_rect(
                        g.hdc,
                        g.b,
                        c.left - 7,
                        y - 6,
                        c.right + 7,
                        y + 44,
                        7,
                        p.surface,
                    );
                    text_fit(
                        g.hdc,
                        g.f.semibold,
                        13,
                        p.text,
                        c.left,
                        y,
                        label,
                        c.right - c.left,
                    );
                    text_fit(
                        g.hdc,
                        g.f.body,
                        13,
                        p.text2,
                        c.left,
                        y + 22,
                        value,
                        c.right - c.left,
                    );
                }
            }
        }
        self.y += 60;
    }
    fn note(&mut self, s: &[u8]) {
        if self.visible(self.y, 18) {
            let y = self.screen(self.y);
            if let Some(g) = self.g.as_deref_mut() {
                unsafe {
                    text_fit(
                        g.hdc,
                        g.f.small,
                        11,
                        pal().text3,
                        self.c.left,
                        y,
                        s,
                        self.c.right - self.c.left,
                    )
                };
            }
        }
        self.y += 24;
    }
    /// Boutons d'options, quatre par ligne au plus.
    fn options(&mut self, opts: &[(&[u8], i32, bool)]) {
        let per_line = 4.min(opts.len().max(1)) as i32;
        // Au moins trois colonnes : un bouton seul garde une taille normale.
        let cols = per_line.max(3);
        let w = ((self.c.right - self.c.left - 8 * (cols - 1)) / cols).max(60);
        for (i, &(label, id, selected)) in opts.iter().enumerate() {
            let col = i as i32 % per_line;
            if col == 0 && i > 0 {
                self.y += 44;
            }
            let x = self.c.left + col * (w + 8);
            self.button(x, w, label, id, selected, false);
        }
        self.y += 48;
    }
    fn button(&mut self, x: i32, w: i32, label: &[u8], id: i32, selected: bool, primary: bool) {
        if !self.visible(self.y, 36) {
            return;
        }
        let y = self.screen(self.y);
        let r = rect(x, y, x + w, y + 36);
        if let Some((px, py)) = self.probe {
            if inside(&r, px, py) {
                self.hit = id;
            }
        }
        if let Some(g) = self.g.as_deref_mut() {
            let p = pal();
            unsafe {
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    7,
                    if selected { p.accent } else { p.border },
                );
                round_rect(
                    g.hdc,
                    g.b,
                    r.left + 1,
                    r.top + 1,
                    r.right - 1,
                    r.bottom - 1,
                    6,
                    if primary {
                        p.accent
                    } else if selected {
                        p.selected
                    } else if HOVER == id {
                        p.hover
                    } else {
                        p.surface2
                    },
                );
                let mut end = label.len();
                while end > 0 && text_width(&label[..end], 11) > w - 12 {
                    end -= 1;
                }
                text_centered(
                    g.hdc,
                    g.f.small,
                    11,
                    if primary { p.on_accent } else { p.text },
                    (r.left + r.right) / 2,
                    r.top + 12,
                    &label[..end],
                );
            }
        }
    }
    /// Tuiles par processeur logique : fréquence en direct ou sélection.
    fn tiles(&mut self, mode_select: bool) {
        let hybrid = cap(b"cap_hybrid");
        let pcores = info(b"pcores");
        let max = num(info(b"freq_max")).max(1);
        let tile_w = 96;
        let per_line = ((self.c.right - self.c.left + 8) / (tile_w + 8)).max(1);
        let mut last_line = -1;
        let base = self.y;
        for_each_cpu(|i, id, mhz| {
            let line = i as i32 / per_line;
            let col = i as i32 % per_line;
            last_line = line;
            let y_rel = base + line * 54;
            if !self.visible(y_rel, 46) {
                return;
            }
            let x = self.c.left + col * (tile_w + 8);
            let y = self.screen(y_rel);
            let r = rect(x, y, x + tile_w, y + 46);
            let chip = ID_CHIP + id as i32;
            if mode_select {
                if let Some((px, py)) = self.probe {
                    if inside(&r, px, py) {
                        self.hit = chip;
                    }
                }
            }
            let Some(g) = self.g.as_deref_mut() else {
                return;
            };
            let p = pal();
            let selected = mode_select && exe_cpu_selected(id);
            let mut label = Line::new();
            label.push(b"CPU ").num(id);
            if hybrid {
                label.push(if cpulist_contains(pcores, id) {
                    b" \x95 P"
                } else {
                    b" \x95 E"
                });
            }
            let live = if i < 64 {
                unsafe { LIVE_MHZ[i] }
            } else {
                mhz
            };
            let live = if live == 0 { mhz } else { live };
            unsafe {
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    6,
                    if selected { p.accent } else { p.border },
                );
                round_rect(
                    g.hdc,
                    g.b,
                    r.left + 1,
                    r.top + 1,
                    r.right - 1,
                    r.bottom - 1,
                    5,
                    if selected {
                        p.selected
                    } else if mode_select && HOVER == chip {
                        p.hover
                    } else {
                        p.surface
                    },
                );
                text_fit(
                    g.hdc,
                    g.f.small,
                    11,
                    p.text2,
                    r.left + 8,
                    r.top + 6,
                    label.get(),
                    tile_w - 12,
                );
                if mode_select {
                    text(
                        g.hdc,
                        g.f.semibold,
                        p.text,
                        r.left + 8,
                        r.top + 22,
                        if selected {
                            t(b"Autoris\xE9", b"Allowed")
                        } else {
                            t(b"Exclu", b"Excluded")
                        },
                    );
                } else {
                    let mut v = Line::new();
                    v.num(live).push(b" MHz");
                    text(g.hdc, g.f.semibold, p.text, r.left + 8, r.top + 20, v.get());
                    let bar = (tile_w - 16) * live.min(max) as i32 / max as i32;
                    fill(
                        g.hdc,
                        g.b,
                        r.left + 8,
                        r.bottom - 7,
                        r.right - 8,
                        r.bottom - 4,
                        p.surface2,
                    );
                    fill(
                        g.hdc,
                        g.b,
                        r.left + 8,
                        r.bottom - 7,
                        r.left + 8 + bar.max(2),
                        r.bottom - 4,
                        p.accent,
                    );
                }
            }
        });
        self.y = base + (last_line + 1) * 54 + 6;
    }
}

fn exe_cpu_selected(id: u32) -> bool {
    unsafe {
        let mask = EXE_AFFINITY;
        if mask == [0; 4] {
            return true;
        }
        id < 256 && mask[id as usize / 64] & (1 << (id % 64)) != 0
    }
}

fn profile_label(key: &[u8]) -> &'static [u8] {
    match key {
        b"performance" => t(b"Performances maximales", b"Best performance"),
        b"balanced" => t(b"\xC9quilibr\xE9", b"Balanced"),
        b"powersave" => t(b"\xC9conomie d'\xE9nergie", b"Power saver"),
        _ => t(b"Personnalis\xE9", b"Custom"),
    }
}

unsafe fn walk(w: &mut Walk) {
    if !available() {
        w.row(
            t(b"Informations processeur indisponibles", b"Processor information unavailable"),
            t(
                b"Le service Winlux n'a pas encore publi\xE9 l'\xE9tat du processeur.",
                b"The Winlux service has not published the processor state yet.",
            ),
        );
        return;
    }
    // Résumé, toujours affiché : ce sont des informations, pas des commandes.
    let mut summary = Line::new();
    summary
        .num(num(info(b"cpu_count")))
        .push(t(b" processeurs logiques", b" logical processors"));
    if num(info(b"packages")) > 1 {
        summary
            .push(b", ")
            .num(num(info(b"packages")))
            .push(t(b" processeurs physiques", b" sockets"));
    }
    let model = info(b"cpu_model");
    w.row(
        if model.is_empty() {
            t(b"Processeur", b"Processor")
        } else {
            model
        },
        summary.get(),
    );

    if cap(b"cap_freq") {
        w.header(t(b"Fr\xE9quences en temps r\xE9el", b"Live frequencies"));
        w.tiles(false);
        let mut max = Line::new();
        max.push(t(b"Fr\xE9quence maximale : ", b"Maximum frequency: "))
            .num(num(info(b"freq_max")))
            .push(b" MHz");
        w.note(max.get());
    }

    if cap(b"cap_governor") {
        w.header(t(b"Mode d'alimentation", b"Power mode"));
        let current = info(b"profile");
        w.note(t(
            b"Performances : vitesse maximale. \xC9conomie : moins de chaleur et de bruit.",
            b"Performance: top speed. Power saver: less heat and noise.",
        ));
        let mut opts: [(&[u8], i32, bool); 3] = [(b"", 0, false); 3];
        let mut n = 0;
        for (i, key) in [b"performance".as_slice(), b"balanced", b"powersave"]
            .iter()
            .enumerate()
        {
            if info(b"profiles").split(|&c| c == b',').any(|p| p == *key) {
                opts[n] = (profile_label(key), ID_PROFILE + i as i32, current == *key);
                n += 1;
            }
        }
        w.options(&opts[..n]);
    }

    if cap(b"cap_hybrid") {
        w.header(t(
            b"C\x9Curs performants (P) et c\x9Curs \xE9conomes (E)",
            b"Performance (P) and efficient (E) cores",
        ));
        let mut l = Line::new();
        l.push(t(b"P-cores : ", b"P-cores: "))
            .push(info(b"pcores"))
            .push(t(b"   E-cores : ", b"   E-cores: "))
            .push(info(b"ecores"));
        w.row(
            t(
                b"Placement des programmes Windows",
                b"Placement of Windows programs",
            ),
            l.get(),
        );
        let policy = info(b"hybrid_policy");
        w.options(&[
            (
                t(b"Automatique (recommand\xE9)", b"Automatic (recommended)"),
                ID_HYBRID,
                policy != b"heavy" && policy != b"performance" && policy != b"efficiency",
            ),
            (
                t(b"Jeux et 3D sur P-cores", b"Games and 3D on P-cores"),
                ID_HYBRID + 1,
                policy == b"heavy",
            ),
            (
                t(b"Tout sur P-cores", b"Everything on P-cores"),
                ID_HYBRID + 2,
                policy == b"performance",
            ),
            (
                t(b"Tout sur E-cores", b"Everything on E-cores"),
                ID_HYBRID + 3,
                policy == b"efficiency",
            ),
        ]);
        w.note(t(
            b"S'applique aux programmes lanc\xE9s ensuite.",
            b"Applies to programs started afterwards.",
        ));
    }

    let thermal = cap(b"cap_thermal");
    let throttle = cap(b"cap_throttle");
    let fans = cap(b"cap_fans");
    if thermal || throttle || fans {
        w.header(t(b"Temp\xE9ratures et ventilation", b"Temperatures and cooling"));
    }
    if thermal {
        let mut l = Line::new();
        l.num(LIVE_TEMP.max(0) as u32).push(b" \xB0C");
        let crit = num(info(b"temp_crit"));
        if crit > 0 {
            l.push(t(b"  (limite ", b"  (limit "))
                .num(crit)
                .push(b" \xB0C)");
        }
        w.row(t(b"Temp\xE9rature du processeur", b"Processor temperature"), l.get());
    }
    if throttle {
        let count = num(info(b"throttle"));
        let recent = THROTTLE_AT != 0 && GetTickCount().wrapping_sub(THROTTLE_AT) < 10_000;
        let mut l = Line::new();
        if recent {
            l.push(t(
                b"Ralentissement en cours : le processeur chauffe.",
                b"Throttling now: the processor is hot.",
            ));
        } else if count == 0 {
            l.push(t(
                b"Aucun ralentissement d\xE9tect\xE9.",
                b"No throttling detected.",
            ));
        } else {
            l.num(count).push(t(
                b" ralentissements thermiques depuis le d\xE9marrage.",
                b" thermal throttling events since boot.",
            ));
        }
        w.row(t(b"Bridage thermique", b"Thermal throttling"), l.get());
    }
    if fans {
        for entry in info(b"fans").split(|&c| c == b',').take(8) {
            let mut it = entry.split(|&c| c == b':');
            let label = it.next().unwrap_or(b"");
            let rpm = num(it.next().unwrap_or(b""));
            let mut l = Line::new();
            l.num(rpm).push(t(b" tr/min", b" RPM"));
            w.row(label, l.get());
        }
    }
    if cap(b"cap_fan_control") {
        let curve = info(b"fan_curve");
        w.note(t(
            b"Courbe des ventilateurs (100 % automatique au-del\xE0 de 85 \xB0C) :",
            b"Fan curve (always 100% above 85 \xB0C):",
        ));
        w.options(&[
            (
                t(b"Automatique", b"Automatic"),
                ID_FAN,
                !matches!(curve, b"quiet" | b"balanced" | b"performance"),
            ),
            (t(b"Silencieux", b"Quiet"), ID_FAN + 1, curve == b"quiet"),
            (
                t(b"\xC9quilibr\xE9", b"Balanced"),
                ID_FAN + 2,
                curve == b"balanced",
            ),
            (
                t(b"Refroidissement max.", b"Max cooling"),
                ID_FAN + 3,
                curve == b"performance",
            ),
        ]);
    }

    if cap(b"cap_numa") {
        w.header(t(
            b"M\xE9moire NUMA (plusieurs processeurs physiques)",
            b"NUMA memory (multiple sockets)",
        ));
        for node in info(b"numa_nodes").split(|&c| c == b';').take(16) {
            let mut it = node.split(|&c| c == b':');
            let id = num(it.next().unwrap_or(b""));
            let mut label = Line::new();
            label.push(t(b"N\x9Cud ", b"Node ")).num(id);
            let mut l = Line::new();
            l.push(t(b"Processeurs ", b"Processors "))
                .push(it.next().unwrap_or(b""));
            w.row(label.get(), l.get());
        }
    }

    program_section(w);
}

unsafe fn program_section(w: &mut Walk) {
    PROGRAM_Y = w.y;
    w.header(t(
        b"Priorit\xE9 et processeurs par programme",
        b"Priority and processors per program",
    ));
    let path = crate::settings::exe_path();
    if path.is_empty() {
        w.row(
            t(b"Choisir un programme", b"Choose a program"),
            t(
                b"Clic droit sur un .exe dans l'Explorateur, puis R\xE9glages de l'ex\xE9cutable.",
                b"Right-click an .exe in Explorer, then Executable settings.",
            ),
        );
        w.options(&[(
            t(b"Ouvrir l'Explorateur", b"Open Explorer"),
            ID_EXPLORER,
            false,
        )]);
        return;
    }
    let mut name = [b'?'; 160];
    let start = path
        .iter()
        .rposition(|&c| c == b'\\' as u16 || c == b'/' as u16)
        .map(|i| i + 1)
        .unwrap_or(0);
    let n = (path.len() - start).min(name.len());
    for i in 0..n {
        let c = path[start + i];
        name[i] = if c <= 255 { c as u8 } else { b'?' };
    }
    w.row(
        &name[..n],
        t(
            b"R\xE9glages appliqu\xE9s au prochain lancement de ce programme.",
            b"Settings apply the next time this program starts.",
        ),
    );

    w.note(t(b"Priorit\xE9 :", b"Priority:"));
    let mut opts: [(&[u8], i32, bool); 4] = [(b"", 0, false); 4];
    let mut k = 0;
    for (i, (_, fr, en_label)) in PRIORITIES.iter().enumerate() {
        // Garde-fous : « Haute » et « Temps réel » seulement si Linux les
        // autorise vraiment pour cet utilisateur.
        if i == 2 && !cap(b"cap_prio_high") || i == 3 && !cap(b"cap_prio_realtime") {
            continue;
        }
        let selected = EXE_PRIORITY as usize == i + 1 || EXE_PRIORITY == 0 && i == 1;
        opts[k] = (
            if en() { en_label } else { fr },
            ID_PRIORITY + i as i32,
            selected,
        );
        k += 1;
    }
    w.options(&opts[..k]);

    if cap(b"cap_hybrid") {
        w.note(t(b"Type de c\x9Curs :", b"Core type:"));
        w.options(&[
            (t(b"Automatique", b"Automatic"), ID_CORES, EXE_CORES == 0),
            (
                t(b"P-cores (rapides)", b"P-cores (fast)"),
                ID_CORES + 1,
                EXE_CORES == 1,
            ),
            (
                t(b"E-cores (\xE9conomes)", b"E-cores (efficient)"),
                ID_CORES + 2,
                EXE_CORES == 2,
            ),
        ]);
    }

    if cap(b"cap_numa") {
        w.note(t(b"N\x9Cud NUMA :", b"NUMA node:"));
        let mut labels = [Line::new(), Line::new(), Line::new()];
        let mut ids = [0i32; 3];
        let mut k = 0;
        for node in info(b"numa_nodes").split(|&c| c == b';').take(3) {
            let id = num(node.split(|&c| c == b':').next().unwrap_or(b""));
            labels[k].push(t(b"N\x9Cud ", b"Node ")).num(id);
            ids[k] = id as i32;
            k += 1;
        }
        let mut opts: [(&[u8], i32, bool); 4] = [(b"", 0, false); 4];
        opts[0] = (t(b"Automatique", b"Automatic"), ID_NUMA, EXE_NUMA < 0);
        for i in 0..k {
            opts[i + 1] = (labels[i].get(), ID_NUMA + 1 + ids[i], EXE_NUMA == ids[i]);
        }
        let k = k + 1;
        w.options(&opts[..k]);
    }

    if cap(b"cap_affinity") {
        w.note(t(
            b"Processeurs autoris\xE9s (cliquez pour inclure ou exclure) :",
            b"Allowed processors (click to include or exclude):",
        ));
        w.tiles(true);
        w.options(&[(
            t(b"Tous les processeurs", b"All processors"),
            ID_ALL_CPUS,
            EXE_AFFINITY == [0; 4],
        )]);
    }
    let width = (w.c.right - w.c.left - 16) / 3;
    let left = w.c.left;
    w.button(
        left,
        width,
        t(b"Enregistrer", b"Save"),
        ID_SAVE,
        false,
        true,
    );
    w.button(
        left + width + 8,
        width,
        t(b"Lancer le programme", b"Start program"),
        ID_LAUNCH,
        false,
        false,
    );
    w.button(
        left + 2 * (width + 8),
        width,
        t(b"Valeurs par d\xE9faut", b"Defaults"),
        ID_RESET,
        false,
        false,
    );
    w.y += 48;
    w.note(NOTICE);
}

fn content_top(c: &Rect) -> i32 {
    c.top + 44
}

pub unsafe fn draw(g: &mut Gfx, c: &Rect) {
    if FOCUS_PROGRAM && available() {
        // Premier dessin après « Processeur… » : mesurer puis défiler.
        let mut probe = Walk {
            g: None,
            c: *c,
            y: 58,
            clip_top: i32::MIN,
            probe: None,
            hit: -1,
        };
        SCROLL_PX = 0;
        walk(&mut probe);
        CONTENT_H = probe.y;
        // Le titre de la section arrive juste sous l'en-tête de la page.
        SCROLL_PX = (PROGRAM_Y - 40).max(0);
        FOCUS_PROGRAM = false;
    }
    clamp_scroll(c);
    let mut w = Walk {
        g: Some(g),
        c: *c,
        y: 58,
        clip_top: content_top(c),
        probe: None,
        hit: -1,
    };
    walk(&mut w);
    CONTENT_H = w.y;
    clamp_scroll(c);
}

fn clamp_scroll(c: &Rect) {
    unsafe {
        let view = c.bottom - 24 - content_top(c);
        SCROLL_PX = SCROLL_PX.clamp(0, (CONTENT_H - view + 30).max(0));
    }
}

unsafe fn hit(c: &Rect, x: i32, y: i32) -> i32 {
    let mut w = Walk {
        g: None,
        c: *c,
        y: 58,
        clip_top: content_top(c),
        probe: Some((x, y)),
        hit: -1,
    };
    walk(&mut w);
    w.hit
}

pub unsafe fn hover(c: &Rect, x: i32, y: i32) -> bool {
    let h = hit(c, x, y);
    let changed = h != HOVER;
    HOVER = h;
    changed
}

pub unsafe fn scroll(lines: i32) -> bool {
    SCROLL_PX = SCROLL_PX.saturating_add(lines * 24).max(0);
    true
}

pub unsafe fn focus_program() {
    FOCUS_PROGRAM = true;
    LAST_READ = 0;
}

pub unsafe fn reset_program() {
    EXE_PRIORITY = 0;
    EXE_CORES = 0;
    EXE_AFFINITY = [0; 4];
    EXE_NUMA = -1;
    NOTICE = b"";
}

/// Lit les clés processeur d'un profil `.winlux.ini`.
pub unsafe fn load_program(data: &[u8]) {
    reset_program();
    let field = |k: &[u8]| crate::settings::field(data, k);
    EXE_PRIORITY = PRIORITIES
        .iter()
        .position(|(key, _, _)| *key == field(b"cpu_priority"))
        .map(|i| i as u8 + 1)
        .unwrap_or(0);
    EXE_CORES = match field(b"cpu_cores") {
        b"performance" => 1,
        b"efficiency" => 2,
        _ => 0,
    };
    let list = field(b"cpu_affinity");
    for id in 0..256u32 {
        if !list.is_empty() && cpulist_contains(list, id) {
            EXE_AFFINITY[id as usize / 64] |= 1 << (id % 64);
        }
    }
    let numa = field(b"numa_node");
    EXE_NUMA = if numa.first().is_some_and(u8::is_ascii_digit) {
        num(numa) as i32
    } else {
        -1
    };
}

/// Ajoute les clés processeur au profil ; renvoie la longueur écrite.
pub unsafe fn write_program(out: &mut [u8]) -> usize {
    let mut l = Line::new();
    if EXE_PRIORITY > 0 {
        l.push(b"cpu_priority=")
            .push(PRIORITIES[EXE_PRIORITY as usize - 1].0)
            .push(b"\n");
    }
    match EXE_CORES {
        1 => {
            l.push(b"cpu_cores=performance\n");
        }
        2 => {
            l.push(b"cpu_cores=efficiency\n");
        }
        _ => {}
    }
    if EXE_AFFINITY != [0; 4] {
        l.push(b"cpu_affinity=");
        let mut first = true;
        let mut id = 0u32;
        while id < 256 {
            if exe_cpu_selected(id) {
                let start = id;
                while id + 1 < 256 && exe_cpu_selected(id + 1) {
                    id += 1;
                }
                if !first {
                    l.push(b",");
                }
                first = false;
                l.num(start);
                if id > start {
                    l.push(b"-").num(id);
                }
            }
            id += 1;
        }
        l.push(b"\n");
    }
    if EXE_NUMA >= 0 {
        l.push(b"numa_node=").num(EXE_NUMA as u32).push(b"\n");
    }
    let n = l.n.min(out.len());
    out[..n].copy_from_slice(&l.get()[..n]);
    n
}

fn toggle_cpu(id: u32) {
    unsafe {
        if id >= 256 {
            return;
        }
        if EXE_AFFINITY == [0; 4] {
            // Partir de « tous » : on exclut le processeur cliqué.
            for_each_cpu(|_, cpu, _| {
                if cpu < 256 {
                    EXE_AFFINITY[cpu as usize / 64] |= 1 << (cpu % 64);
                }
            });
        }
        EXE_AFFINITY[id as usize / 64] ^= 1 << (id % 64);
        let mut all = true;
        for_each_cpu(|_, cpu, _| all &= exe_cpu_selected(cpu));
        if EXE_AFFINITY == [0; 4] {
            // Jamais zéro processeur : on revient au processeur cliqué seul.
            EXE_AFFINITY[id as usize / 64] |= 1 << (id % 64);
        } else if all {
            EXE_AFFINITY = [0; 4];
        }
    }
}

pub unsafe fn click(c: &Rect, x: i32, y: i32) -> bool {
    let id = hit(c, x, y);
    let action: Option<&core::ffi::CStr> = match id {
        100 => Some(c"cpu-profile-performance"),
        101 => Some(c"cpu-profile-balanced"),
        102 => Some(c"cpu-profile-powersave"),
        110 => Some(c"cpu-hybrid-auto"),
        111 => Some(c"cpu-hybrid-heavy"),
        112 => Some(c"cpu-hybrid-performance"),
        113 => Some(c"cpu-hybrid-efficiency"),
        120 => Some(c"cpu-fan-auto"),
        121 => Some(c"cpu-fan-quiet"),
        122 => Some(c"cpu-fan-balanced"),
        123 => Some(c"cpu-fan-performance"),
        ID_PRIORITY..=133 => {
            EXE_PRIORITY = (id - ID_PRIORITY) as u8 + 1;
            None
        }
        ID_CORES..=142 => {
            EXE_CORES = (id - ID_CORES) as u8;
            None
        }
        ID_ALL_CPUS => {
            EXE_AFFINITY = [0; 4];
            None
        }
        ID_NUMA => {
            EXE_NUMA = -1;
            None
        }
        161..=176 => {
            EXE_NUMA = id - ID_NUMA - 1;
            None
        }
        ID_SAVE => {
            crate::settings::save_exe_settings_pub(false);
            NOTICE = crate::settings::exe_notice();
            None
        }
        ID_RESET => {
            crate::settings::save_exe_settings_pub(true);
            NOTICE = crate::settings::exe_notice();
            None
        }
        ID_LAUNCH => {
            crate::settings::launch_exe();
            None
        }
        ID_EXPLORER => {
            crate::explorer::open_pc();
            None
        }
        id if id >= ID_CHIP => {
            toggle_cpu((id - ID_CHIP) as u32);
            None
        }
        _ => return false,
    };
    if let Some(a) = action {
        crate::settings::control(a);
        LAST_READ = 0;
    }
    true
}

/// Vérifications sans matériel : sur une machine sans aucune capacité, seul
/// le résumé et la section « par programme » existent.
pub unsafe fn regression_checks() {
    let saved_len = INFO_LEN;
    let saved = INFO;
    let report = b"cpu_model=Test CPU\ncpu_count=2\ncpu_list=0-1\npackages=1\ncap_freq=0\ncap_governor=0\ncap_hybrid=0\ncap_affinity=1\ncap_numa=0\ncap_thermal=0\ncap_throttle=0\ncap_fans=0\ncap_fan_control=0\ncap_prio_high=0\ncap_prio_realtime=0\n";
    INFO[..report.len()].copy_from_slice(report);
    INFO_LEN = report.len();
    let c = rect(300, 80, 1000, 4000);
    let mut found = false;
    for y in (80..4000).step_by(6) {
        for x in (300..1000).step_by(20) {
            found |= matches!(hit(&c, x, y), 100..=123 | 132 | 133 | 140..=142 | 160..=176);
        }
    }
    check!(
        !found,
        "Processeur : aucune commande pour une capacite absente"
    );
    let full = b"cpu_count=4\ncpu_list=0-3\nfreq=0:1200,1:1300,2:1400,3:1500\nfreq_max=4000\ncap_freq=1\ncap_governor=1\nprofiles=performance,powersave\nprofile=powersave\ncap_hybrid=1\npcores=0-1\necores=2-3\ncap_affinity=1\ncap_numa=0\ncap_thermal=1\ntemp=50\ncap_throttle=0\ncap_fans=0\ncap_fan_control=0\ncap_prio_high=1\ncap_prio_realtime=0\nhybrid_policy=heavy\n";
    INFO[..full.len()].copy_from_slice(full);
    INFO_LEN = full.len();
    let mut seen = [false; 4];
    for y in (80..4000).step_by(6) {
        for x in (300..1000).step_by(20) {
            match hit(&c, x, y) {
                100 => seen[0] = true,
                101 => seen[1] = true,
                102 => seen[2] = true,
                111 => seen[3] = true,
                _ => {}
            }
        }
    }
    check!(
        seen[0] && !seen[1] && seen[2] && seen[3],
        "Processeur : seuls les profils disponibles et le routage hybride sont proposes"
    );
    reset_program();
    toggle_cpu(1);
    let mut buf = [0u8; 128];
    let n = write_program(&mut buf);
    check!(
        &buf[..n] == b"cpu_affinity=0,2-3\n",
        "Processeur : exclure un coeur ecrit une affinite valide"
    );
    load_program(b"cpu_priority=high\ncpu_cores=efficiency\ncpu_affinity=0-1\nnuma_node=1\n");
    check!(
        EXE_PRIORITY == 3 && EXE_CORES == 2 && exe_cpu_selected(1) && !exe_cpu_selected(2) && EXE_NUMA == 1,
        "Processeur : le profil du programme est relu"
    );
    reset_program();
    INFO = saved;
    INFO_LEN = saved_len;
}
