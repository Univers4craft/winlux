//! Paramètres > Graphismes : GPU, mesures en direct, réglages avancés et
//! overlay de performances.
//!
//! Garde-fous : chaque ligne n'existe que si le pilote fournit la donnée ou
//! accepte le réglage sur cette machine (rapport `WinluxQueryGpu`). Aucune
//! section grisée : ce qui ne marche pas n'est pas affiché.

use super::{content, control, field, value, Buffer, CAPACITY, EMPTY, SCROLL};
use crate::gfx::*;
use crate::gpuinfo::{self as gpu, Line};
use crate::wm::{Frame, Gfx};
use winlux_testexe::*;

const ROW_H: i32 = 58;
const HEADER_H: i32 = 34;
const BTN_H: i32 = 30;

const PERF: u32 = 1;
const POWER: u32 = 2;
const CLOCK: u32 = 3;
const OVL_ENABLE: u32 = 4;
const OVL_KEY: u32 = 5;
const OVL_POS: u32 = 6;
const OVL_AUTO: u32 = 7;
const OVL_METRIC: u32 = 8;
const OPEN_EXPLORER: u32 = 9;
const RT_BACKEND: u32 = 10;
const RT_DEVICE: u32 = 11;
const RT_VIDEO: u32 = 12;
const RT_VSYNC: u32 = 13;

pub static mut HOVER: u32 = 0;

#[derive(Clone, Copy)]
pub struct Btn {
    label: &'static [u8],
    action: u32,
    selected: bool,
}

const fn btn(label: &'static [u8], action: u32, selected: bool) -> Btn {
    Btn {
        label,
        action,
        selected,
    }
}

const fn act(kind: u32, gpu: usize, arg: u32) -> u32 {
    (kind << 16) | ((gpu as u32) << 8) | arg
}

enum Item<'a> {
    Header(&'a [u8]),
    Row(&'a [u8], &'a [u8], &'a [Btn]),
}

unsafe fn flag(key: &[u8], default: bool) -> bool {
    match value(key) {
        b"1" => true,
        b"0" => false,
        _ => default,
    }
}

/// Parcourt les éléments visibles dans l'ordre d'affichage. Le dessin et le
/// clic utilisent la même liste : impossible de cliquer un bouton masqué.
unsafe fn items(mut visit: impl FnMut(Item)) {
    let count = gpu::count();
    if count == 0 {
        visit(Item::Row(
            b"Aucune carte graphique d\xE9tect\xE9e",
            b"Le pilote n'expose aucun GPU PCI (machine virtuelle ou affichage de secours).",
            &[],
        ));
    }
    for i in 0..count {
        let name = gpu::get(i, b"name").unwrap_or(b"Carte graphique");
        visit(Item::Header(name));
        let mut kind = Line::new();
        if let Some(role) = gpu::get(i, b"role").and_then(gpu::role_label) {
            kind.push(role).push(b"   \xB7   ");
        }
        kind.push(b"Pilote ")
            .push(gpu::get(i, b"driver").unwrap_or(b"inconnu"));
        visit(Item::Row(b"Type", kind.get(), &[]));

        if gpu::has_metrics(i) {
            let mut live = Line::new();
            gpu::summary(i, &mut live);
            visit(Item::Row(b"Mesures en direct", live.get(), &[]));
        }

        if let Some(level) = gpu::get(i, b"perf_level") {
            let label: &[u8] = match level {
                b"auto" => b"Automatique (recommand\xE9)",
                b"low" => b"\xC9conomie d'\xE9nergie",
                b"high" => b"Performances maximales",
                _ => b"R\xE9glage personnalis\xE9 par un autre outil",
            };
            let buttons = [
                btn(b"Auto", act(PERF, i, 0), level == b"auto"),
                btn(b"\xC9conomie", act(PERF, i, 1), level == b"low"),
                btn(b"Maximum", act(PERF, i, 2), level == b"high"),
            ];
            visit(Item::Row(b"Profil de performances", label, &buttons));
        }

        if let (Some(cur), Some(min), Some(max)) = (
            gpu::num(i, b"power_limit_w"),
            gpu::num(i, b"power_limit_min_w"),
            gpu::num(i, b"power_limit_max_w"),
        ) {
            let mut v = Line::new();
            v.num(cur)
                .push(b" W   (plage ")
                .num(min)
                .push(b" - ")
                .num(max)
                .push(b" W)");
            let has_default = gpu::get(i, b"power_limit_default_w").is_some();
            let all = [
                btn(b"- 10 W", act(POWER, i, 0), false),
                btn(b"+ 10 W", act(POWER, i, 1), false),
                btn(b"Par d\xE9faut", act(POWER, i, 2), false),
            ];
            let n = if has_default { 3 } else { 2 };
            visit(Item::Row(b"Limite de puissance", v.get(), &all[..n]));
        }

        if let (Some(cur), Some(min), Some(max)) = (
            gpu::num(i, b"oc_sclk_mhz"),
            gpu::num(i, b"oc_sclk_min_mhz"),
            gpu::num(i, b"oc_sclk_max_mhz"),
        ) {
            let mut v = Line::new();
            v.push(b"C\x9Cur max. ")
                .num(cur)
                .push(b" MHz (")
                .num(min)
                .push(b"-")
                .num(max)
                .push(b"). Trop haut = instabilit\xE9.");
            let buttons = [
                btn(b"- 25 MHz", act(CLOCK, i, 0), false),
                btn(b"+ 25 MHz", act(CLOCK, i, 1), false),
                btn(b"R\xE9tablir", act(CLOCK, i, 2), false),
            ];
            visit(Item::Row(b"Overclocking (avanc\xE9)", v.get(), &buttons));
        }
    }

    runtime_items(&mut visit);

    if gpu::hybrid() {
        visit(Item::Header(
            b"Pr\xE9f\xE9rences graphiques par application",
        ));
        visit(Item::Row(
            b"Choisir le GPU d'un programme",
            b"Clic droit sur le .exe > R\xE9glages de l'ex\xE9cutable > GPU.",
            &[btn(b"Explorateur", act(OPEN_EXPLORER, 0, 0), false)],
        ));
    }

    visit(Item::Header(b"Overlay de performances en jeu"));
    let enabled = flag(b"overlay_enabled", false);
    visit(Item::Row(
        b"Overlay",
        if enabled {
            b"Activ\xE9 : le raccourci l'affiche ou le masque dans le jeu."
        } else {
            b"D\xE9sactiv\xE9"
        },
        &[
            btn(b"Activer", act(OVL_ENABLE, 0, 1), enabled),
            btn(b"D\xE9sactiver", act(OVL_ENABLE, 0, 0), !enabled),
        ],
    ));
    if !enabled {
        return;
    }
    let key = value(b"overlay_hotkey");
    visit(Item::Row(
        b"Raccourci clavier",
        b"Affiche ou masque l'overlay",
        &[
            btn(
                b"Maj+F12",
                act(OVL_KEY, 0, 0),
                key.is_empty() || key == b"shift-f12",
            ),
            btn(b"Ctrl+Maj+O", act(OVL_KEY, 0, 1), key == b"ctrl-shift-o"),
            btn(b"Alt+F12", act(OVL_KEY, 0, 2), key == b"alt-f12"),
        ],
    ));
    let pos = value(b"overlay_position");
    visit(Item::Row(
        b"Position",
        b"Coin de la fen\xEAtre du jeu",
        &[
            btn(
                b"Haut gauche",
                act(OVL_POS, 0, 0),
                pos.is_empty() || pos == b"top-left",
            ),
            btn(b"Haut droite", act(OVL_POS, 0, 1), pos == b"top-right"),
            btn(b"Bas gauche", act(OVL_POS, 0, 2), pos == b"bottom-left"),
            btn(b"Bas droite", act(OVL_POS, 0, 3), pos == b"bottom-right"),
        ],
    ));
    let auto = flag(b"overlay_autoshow", false);
    visit(Item::Row(
        b"Afficher au lancement",
        b"Sans attendre le raccourci",
        &[
            btn(b"Oui", act(OVL_AUTO, 0, 1), auto),
            btn(b"Non", act(OVL_AUTO, 0, 0), !auto),
        ],
    ));
    // La charge GPU n'est proposée que si au moins un GPU la fournit.
    let gpu_load = (0..count).any(|i| gpu::get(i, b"load").is_some());
    let metrics = [
        btn(
            b"Images/s",
            act(OVL_METRIC, 0, 0),
            flag(b"overlay_fps", true),
        ),
        btn(
            b"Temps d'image",
            act(OVL_METRIC, 0, 1),
            flag(b"overlay_frametime", true),
        ),
        btn(b"CPU", act(OVL_METRIC, 0, 2), flag(b"overlay_cpu", true)),
        btn(b"GPU", act(OVL_METRIC, 0, 3), flag(b"overlay_gpu", true)),
    ];
    visit(Item::Row(
        b"Mesures affich\xE9es",
        b"Cliquer pour afficher ou masquer",
        &metrics[..if gpu_load { 4 } else { 3 }],
    ));
}

// ── Moteur 3D et décodage vidéo des .exe ───────────────────────────────
// Rapport `WinluxGraphicsInfo` du runtime (Direct3D 9/11/12, Vulkan, OpenGL,
// VA-API). Chaque ligne n'apparaît que si le matériel offre un vrai choix :
// sans GPU Vulkan matériel, pas de choix de moteur ; sans décodage matériel,
// pas de choix de décodage. Hors Winlux (fonctions absentes), rien ne s'affiche.

static mut RUNTIME: Buffer = EMPTY;
/// Remis à `false` à l'ouverture de la page pour relire le rapport.
pub static mut RUNTIME_LOADED: bool = false;
static mut RUNTIME_NOTICE: Line = Line::new();

unsafe fn runtime_proc(name: &core::ffi::CStr) -> *const core::ffi::c_void {
    let k = LoadLibraryA(c"kernel32.dll".as_ptr());
    if k.is_null() {
        return core::ptr::null();
    }
    GetProcAddress(k, name.as_ptr())
}

unsafe fn runtime_refresh() {
    RUNTIME_LOADED = true;
    RUNTIME.len = 0;
    let p = runtime_proc(c"WinluxGraphicsInfo");
    if p.is_null() {
        return;
    }
    let f: unsafe extern "win64" fn(*mut u8, u32) -> u32 = core::mem::transmute(p);
    let n = f(RUNTIME.bytes.as_mut_ptr(), CAPACITY as u32);
    RUNTIME.len = (n as usize).min(CAPACITY - 1);
}

unsafe fn rt(key: &[u8]) -> &'static [u8] {
    if !RUNTIME_LOADED {
        runtime_refresh();
    }
    let data: &'static Buffer = &*core::ptr::addr_of!(RUNTIME);
    field(data.get(), key)
}

unsafe fn rt_set(key: &core::ffi::CStr, value: &[u8]) {
    let p = runtime_proc(c"WinluxSetGraphicsOption");
    if p.is_null() || value.len() > 30 {
        return;
    }
    let mut v = [0u8; 32];
    v[..value.len()].copy_from_slice(value);
    let f: unsafe extern "win64" fn(*const core::ffi::c_char, *const core::ffi::c_char) -> i32 =
        core::mem::transmute(p);
    f(key.as_ptr(), v.as_ptr().cast());
    runtime_refresh();
}

/// GPU Vulkan matériels : (index Vulkan, nom).
unsafe fn rt_gpus(out: &mut [(&'static [u8], &'static [u8]); 8]) -> usize {
    let mut n = 0;
    for item in rt(b"vulkan_hw").split(|&c| c == b';') {
        let Some(colon) = item.iter().position(|&c| c == b':') else {
            continue;
        };
        if n < out.len() {
            out[n] = (&item[..colon], &item[colon + 1..]);
            n += 1;
        }
    }
    n
}

fn pref_label(v: &[u8]) -> &'static [u8] {
    match v {
        b"vulkan" => b"Vulkan",
        b"opengl" => b"OpenGL",
        b"hardware" => b"Mat\xE9riel",
        b"software" => b"Logiciel",
        _ => b"Automatique",
    }
}

unsafe fn runtime_items(visit: &mut impl FnMut(Item)) {
    if rt(b"opengl").is_empty() {
        return;
    }
    let mut gpus = [(&b""[..], &b""[..]); 8];
    let hw = rt_gpus(&mut gpus);
    let backend = hw >= 1 && rt(b"opengl") == b"1";
    let device = hw >= 2;
    let video = !rt(b"video_hw_available").is_empty();
    if !(backend || device || video || hw >= 1) {
        return;
    }
    visit(Item::Header(b"Jeux et applications Windows (Direct3D)"));
    if backend {
        let pref = rt(b"backend_pref");
        let mut v = Line::new();
        v.push(b"Direct3D 12 utilise ")
            .push(pref_label(rt(b"backend_d3d12")))
            .push(b", Direct3D 9 et 11 : OpenGL");
        let buttons = [
            btn(
                b"Auto",
                act(RT_BACKEND, 0, 0),
                pref == b"auto" || pref.is_empty(),
            ),
            btn(b"Vulkan", act(RT_BACKEND, 0, 1), pref == b"vulkan"),
            btn(b"OpenGL", act(RT_BACKEND, 0, 2), pref == b"opengl"),
        ];
        visit(Item::Row(b"Moteur de rendu", v.get(), &buttons));
    }
    if device {
        let sel = rt(b"vulkan_selected");
        let auto = rt(b"vulkan_device_pref") == b"auto";
        let mut v = Line::new();
        v.push(
            gpus[..hw]
                .iter()
                .find(|g| g.0 == sel)
                .map(|g| g.1)
                .unwrap_or(b""),
        );
        if auto {
            v.push(b" (automatique)");
        }
        let buttons = [
            btn(b"Auto", act(RT_DEVICE, 0, 0), auto),
            btn(b"Pr\xE9c\xE9dente", act(RT_DEVICE, 0, 1), false),
            btn(b"Suivante", act(RT_DEVICE, 0, 2), false),
        ];
        visit(Item::Row(b"Carte graphique Vulkan", v.get(), &buttons));
    }
    if video {
        let pref = rt(b"video_pref");
        let active = rt(b"video_active") == b"vaapi";
        let mut v = Line::new();
        if active {
            v.push(b"Mat\xE9riel : ").push(rt(b"video_hw_codecs"));
        } else {
            v.push(b"Logiciel (mat\xE9riel possible : ")
                .push(rt(b"video_hw_available"))
                .push(b")");
        }
        let buttons = [
            btn(
                b"Auto",
                act(RT_VIDEO, 0, 0),
                pref == b"auto" || pref.is_empty(),
            ),
            btn(b"Mat\xE9riel", act(RT_VIDEO, 0, 1), pref == b"hardware"),
            btn(b"Logiciel", act(RT_VIDEO, 0, 2), pref == b"software"),
        ];
        visit(Item::Row(
            b"D\xE9codage vid\xE9o (DXVA2)",
            v.get(),
            &buttons,
        ));
    }
    if hw >= 1 {
        let on = rt(b"vsync") == b"1";
        visit(Item::Row(
            b"Synchronisation verticale",
            if on {
                b"Activ\xE9e : pas de d\xE9chirure d'image"
            } else {
                b"D\xE9sactiv\xE9e : latence minimale"
            },
            &[
                btn(b"Activer", act(RT_VSYNC, 0, 1), on),
                btn(b"D\xE9sactiver", act(RT_VSYNC, 0, 0), !on),
            ],
        ));
    }
}

unsafe fn runtime_click(kind: u32, arg: u32) {
    match kind {
        RT_BACKEND => rt_set(
            c"backend",
            [b"auto".as_slice(), b"vulkan", b"opengl"][arg as usize % 3],
        ),
        RT_DEVICE if arg == 0 => rt_set(c"vulkan_device", b"auto"),
        RT_DEVICE => {
            let mut gpus = [(&b""[..], &b""[..]); 8];
            let hw = rt_gpus(&mut gpus);
            if hw == 0 {
                return;
            }
            let sel = rt(b"vulkan_selected");
            let cur = gpus[..hw].iter().position(|g| g.0 == sel).unwrap_or(0);
            let next = if arg == 1 {
                (cur + hw - 1) % hw
            } else {
                (cur + 1) % hw
            };
            // Copie : le rapport est relu (et réécrit) par rt_set.
            let mut idx = [0u8; 8];
            let len = gpus[next].0.len().min(8);
            idx[..len].copy_from_slice(&gpus[next].0[..len]);
            rt_set(c"vulkan_device", &idx[..len]);
        }
        RT_VIDEO => rt_set(
            c"video_decode",
            [b"auto".as_slice(), b"hardware", b"software"][arg as usize % 3],
        ),
        RT_VSYNC => rt_set(c"vsync", if arg == 1 { b"1" } else { b"0" }),
        _ => {}
    }
}

/// Ligne d'état en bas de page : ce que le runtime a détecté.
pub unsafe fn runtime_notice() -> &'static [u8] {
    let l: &'static mut Line = &mut *core::ptr::addr_of_mut!(RUNTIME_NOTICE);
    l.len = 0;
    if rt(b"opengl").is_empty() {
        return l.get();
    }
    let vk = rt(b"vulkan") == b"1";
    let mut gpus = [(&b""[..], &b""[..]); 8];
    let hw = rt_gpus(&mut gpus);
    l.push(b"D\xE9tect\xE9 : OpenGL ")
        .push(if rt(b"opengl") == b"1" {
            b"oui"
        } else {
            b"non"
        })
        .push(b" \xB7 Vulkan ")
        .push(if vk { rt(b"vulkan_version") } else { b"absent" })
        .push(if vk && hw == 0 { b" (logiciel)" } else { b"" })
        .push(b" \xB7 VA-API ")
        .push(if rt(b"vaapi") == b"1" {
            rt(b"vaapi_vendor")
        } else {
            b"absent"
        });
    l.get()
}

fn button_width(c: &Rect, n: usize) -> i32 {
    let w = c.right - c.left;
    (w * 55 / 100 / n.max(1) as i32 - 6).clamp(64, 112)
}

/// Rectangle du bouton `k` sur `n`, aligné à droite de la ligne.
fn button_rect(c: &Rect, top: i32, k: usize, n: usize) -> Rect {
    let bw = button_width(c, n);
    let right = c.right - (n - 1 - k) as i32 * (bw + 6);
    let y = top + (ROW_H - 8 - BTN_H) / 2;
    rect(right - bw, y, right, y + BTN_H)
}

unsafe fn walk(c: &Rect, mut visit: impl FnMut(Item, i32)) {
    let mut y = c.top + 44 - SCROLL as i32 * 12;
    items(|item| {
        let h = match item {
            Item::Header(_) => HEADER_H,
            Item::Row(..) => ROW_H,
        };
        visit(item, y);
        y += h;
    });
}

fn visible(c: &Rect, top: i32, h: i32) -> bool {
    top >= c.top + 40 && top + h <= c.bottom - 24
}

pub unsafe fn draw(g: &mut Gfx, f: &Frame) {
    gpu::refresh(false);
    let c = content(f);
    // Borne le défilement à la hauteur réelle de la page.
    let mut total = 0;
    items(|item| {
        total += match item {
            Item::Header(_) => HEADER_H,
            Item::Row(..) => ROW_H,
        }
    });
    let room = c.bottom - 24 - (c.top + 44);
    let max_scroll = ((total - room).max(0) + 11) / 12;
    SCROLL = SCROLL.min(max_scroll as usize);
    let p = pal();
    walk(&c, |item, y| match item {
        Item::Header(title) => {
            if visible(&c, y, HEADER_H) {
                text_fit(
                    g.hdc,
                    g.f.semibold,
                    13,
                    p.text,
                    c.left,
                    y + 12,
                    title,
                    c.right - c.left,
                );
            }
        }
        Item::Row(label, val, buttons) => {
            if !visible(&c, y, ROW_H) {
                return;
            }
            round_rect(
                g.hdc,
                g.b,
                c.left - 8,
                y,
                c.right + 8,
                y + ROW_H - 6,
                8,
                p.border,
            );
            round_rect(
                g.hdc,
                g.b,
                c.left - 7,
                y + 1,
                c.right + 7,
                y + ROW_H - 7,
                7,
                p.surface,
            );
            let text_right = if buttons.is_empty() {
                c.right
            } else {
                button_rect(&c, y, 0, buttons.len()).left - 10
            };
            let tw = (text_right - c.left).max(40);
            text_fit(g.hdc, g.f.semibold, 13, p.text, c.left, y + 8, label, tw);
            text_fit(g.hdc, g.f.small, 11, p.text2, c.left, y + 28, val, tw);
            for (k, b) in buttons.iter().enumerate() {
                let r = button_rect(&c, y, k, buttons.len());
                round_rect(
                    g.hdc,
                    g.b,
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    6,
                    if b.selected { p.accent } else { p.border },
                );
                round_rect(
                    g.hdc,
                    g.b,
                    r.left + 1,
                    r.top + 1,
                    r.right - 1,
                    r.bottom - 1,
                    5,
                    if b.selected {
                        p.selected
                    } else if HOVER == b.action + 1 {
                        p.hover
                    } else {
                        p.surface2
                    },
                );
                let mut end = b.label.len();
                while end > 0 && text_width(&b.label[..end], 11) > r.right - r.left - 8 {
                    end -= 1;
                }
                text_centered(
                    g.hdc,
                    g.f.small,
                    11,
                    p.text,
                    (r.left + r.right) / 2,
                    r.top + 9,
                    &b.label[..end],
                );
            }
        }
    });
}

/// Action sous le pointeur, +1 (0 = aucune).
pub unsafe fn hit(f: &Frame, x: i32, y: i32) -> u32 {
    let c = content(f);
    let mut found = 0;
    walk(&c, |item, top| {
        if let Item::Row(_, _, buttons) = item {
            if !visible(&c, top, ROW_H) {
                return;
            }
            for (k, b) in buttons.iter().enumerate() {
                if inside(&button_rect(&c, top, k, buttons.len()), x, y) {
                    found = b.action + 1;
                }
            }
        }
    });
    found
}

struct Command {
    buf: [u8; 96],
    len: usize,
}

impl Command {
    fn new(s: &[u8]) -> Self {
        let mut c = Command {
            buf: [0; 96],
            len: 0,
        };
        c.push(s);
        c
    }
    fn push(&mut self, s: &[u8]) -> &mut Self {
        let n = s.len().min(self.buf.len() - 1 - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s[..n]);
        self.len += n;
        self
    }
    unsafe fn run(&mut self) {
        self.buf[self.len] = 0;
        if let Ok(cmd) = core::ffi::CStr::from_bytes_until_nul(&self.buf) {
            control(cmd);
        }
    }
}

unsafe fn gpu_set(i: usize, setting: &[u8], val: &[u8]) {
    let Some(card) = gpu::get(i, b"card") else {
        return;
    };
    // Seuls des noms `cardN` sont transmis à winlux-control.
    if !card.starts_with(b"card") || !card[4..].iter().all(u8::is_ascii_digit) {
        return;
    }
    Command::new(b"gpu-set ")
        .push(card)
        .push(b" ")
        .push(setting)
        .push(b" ")
        .push(val)
        .run();
}

pub unsafe fn click(f: &Frame, x: i32, y: i32) -> bool {
    let id = hit(f, x, y);
    if id == 0 {
        return false;
    }
    let id = id - 1;
    let (kind, i, arg) = (id >> 16, ((id >> 8) & 0xff) as usize, id & 0xff);
    match kind {
        PERF => gpu_set(
            i,
            b"perf",
            [b"auto".as_slice(), b"low", b"high"][arg as usize % 3],
        ),
        POWER => {
            let (Some(cur), Some(min), Some(max)) = (
                gpu::num(i, b"power_limit_w"),
                gpu::num(i, b"power_limit_min_w"),
                gpu::num(i, b"power_limit_max_w"),
            ) else {
                return true;
            };
            if arg == 2 {
                gpu_set(i, b"power-limit", b"default");
            } else {
                let next = if arg == 0 {
                    cur.saturating_sub(10)
                } else {
                    cur.saturating_add(10)
                }
                .clamp(min, max);
                let mut t = [0u8; 10];
                let n = gpu::fmt_u32(&mut t, next);
                gpu_set(i, b"power-limit", &t[..n]);
            }
        }
        CLOCK => {
            let (Some(cur), Some(min), Some(max)) = (
                gpu::num(i, b"oc_sclk_mhz"),
                gpu::num(i, b"oc_sclk_min_mhz"),
                gpu::num(i, b"oc_sclk_max_mhz"),
            ) else {
                return true;
            };
            if arg == 2 {
                gpu_set(i, b"oc-reset", b"0");
            } else {
                let next = if arg == 0 {
                    cur.saturating_sub(25)
                } else {
                    cur.saturating_add(25)
                }
                .clamp(min, max);
                let mut t = [0u8; 10];
                let n = gpu::fmt_u32(&mut t, next);
                gpu_set(i, b"oc-sclk", &t[..n]);
            }
        }
        OVL_ENABLE => {
            control(if arg == 1 {
                c"overlay-on"
            } else {
                c"overlay-off"
            });
        }
        OVL_KEY => {
            control(
                [
                    c"overlay-key-shift-f12",
                    c"overlay-key-ctrl-shift-o",
                    c"overlay-key-alt-f12",
                ][arg as usize % 3],
            );
        }
        OVL_POS => {
            control(
                [
                    c"overlay-pos-top-left",
                    c"overlay-pos-top-right",
                    c"overlay-pos-bottom-left",
                    c"overlay-pos-bottom-right",
                ][arg as usize % 4],
            );
        }
        OVL_AUTO => {
            control(if arg == 1 {
                c"overlay-auto-on"
            } else {
                c"overlay-auto-off"
            });
        }
        OVL_METRIC => {
            let (key, name): (&[u8], &[u8]) = match arg {
                0 => (b"overlay_fps", b"fps"),
                1 => (b"overlay_frametime", b"frametime"),
                2 => (b"overlay_cpu", b"cpu"),
                _ => (b"overlay_gpu", b"gpu"),
            };
            Command::new(b"overlay-")
                .push(name)
                .push(if flag(key, true) { b"-off" } else { b"-on" })
                .run();
        }
        OPEN_EXPLORER => crate::explorer::open_home(),
        RT_BACKEND..=RT_VSYNC => runtime_click(kind, arg),
        _ => {}
    }
    true
}

/// Sélection du GPU sur la page de l'exécutable : uniquement si hybride.
pub unsafe fn exe_choices() -> bool {
    gpu::refresh(false);
    gpu::hybrid()
}

/// Lignes et actions visibles, pour les tests de non-régression.
struct Seen {
    actions: [u32; 64],
    n: usize,
    power: bool,
    clock: bool,
    perf: bool,
    live: bool,
}

unsafe fn seen() -> Seen {
    let mut s = Seen {
        actions: [0; 64],
        n: 0,
        power: false,
        clock: false,
        perf: false,
        live: false,
    };
    items(|item| {
        if let Item::Row(label, _, buttons) = item {
            s.power |= label == b"Limite de puissance";
            s.clock |= label == b"Overclocking (avanc\xE9)";
            s.perf |= label == b"Profil de performances";
            s.live |= label == b"Mesures en direct";
            for b in buttons {
                if s.n < s.actions.len() {
                    s.actions[s.n] = b.action;
                    s.n += 1;
                }
            }
        }
    });
    s
}

pub unsafe fn regression_checks() {
    let saved = super::STATUS;
    super::STATUS = super::EMPTY;

    gpu::set_for_test(
        b"gpu_count=1\ngpu_hybrid=0\ngpu0_card=card0\ngpu0_name=Virtio\ngpu0_vendor=other\ngpu0_driver=virtio-pci\ngpu0_role=single\n",
    );
    let s = seen();
    check!(
        !s.power && !s.clock && !s.perf && !s.live,
        "GPU sans capteurs : aucune mesure ni reglage avance affiche"
    );
    check!(
        !exe_choices(),
        "Un seul GPU : pas de choix du GPU par application"
    );

    gpu::set_for_test(
        b"gpu_count=2\ngpu_hybrid=1\ngpu0_card=card0\ngpu0_name=Intel\ngpu0_role=integrated\ngpu0_driver=i915\ngpu0_freq_mhz=300\ngpu1_card=card1\ngpu1_name=Radeon\ngpu1_role=dedicated\ngpu1_driver=amdgpu\ngpu1_load=12\ngpu1_power_limit_w=150\ngpu1_power_limit_min_w=100\ngpu1_power_limit_max_w=200\ngpu1_perf_level=auto\n",
    );
    let s = seen();
    check!(
        s.power && s.perf && s.live && !s.clock,
        "GPU AMD : limite et profil affiches, overclocking masque si non expose"
    );
    check!(
        exe_choices(),
        "Machine hybride : le choix du GPU par application est propose"
    );
    let shown = &s.actions[..s.n];
    check!(
        shown.contains(&act(POWER, 1, 1)) && !shown.contains(&act(POWER, 1, 2)),
        "Sans valeur par defaut exposee, le bouton Par defaut est masque"
    );
    check!(
        !shown.contains(&act(OVL_KEY, 0, 0)),
        "Overlay desactive : ses options detaillees restent masquees"
    );
    gpu::set_for_test(b"");
    super::STATUS = saved;
}
