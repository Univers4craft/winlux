//! Personnalisation façon Windows 11 : fond d'écran (galerie par catégories),
//! couleur d'accentuation, barre des tâches (taille, position, alignement),
//! icônes du bureau et son de démarrage. Les mêmes panneaux servent à la page
//! Paramètres > Personnalisation et à l'assistant du premier démarrage (OOBE).
//! Les fonds sont calculés pixel par pixel (aucune image tierce) et les sons
//! sont synthétisés en WAV puis joués par PlaySound.

use crate::gfx::*;
use crate::wm::Gfx;
use winlux_testexe::*;

// ---------- Préférences ----------

pub static mut WALLPAPER: usize = 0;
/// -1 : couleur choisie automatiquement d'après le fond d'écran.
pub static mut ACCENT_INDEX: i32 = -1;
/// Accent sur Démarrer et la barre des tâches.
pub static mut ACCENT_SURFACES: bool = false;
/// 0 petite, 1 moyenne, 2 grande.
pub static mut BAR_SIZE: u8 = 1;
/// 0 bas, 1 haut, 2 gauche, 3 droite.
pub static mut BAR_POS: u8 = 0;
pub static mut SOUND: usize = 1;

static mut OOBE_ACTIVE: bool = false;
static mut OOBE_STEP: usize = 0;
static mut HOVER: i32 = -1;
static mut SECTION: usize = 0;
static mut BG_CATEGORY: usize = 0;
static mut SOUND_PLAYED: bool = false;

const CONF: &core::ffi::CStr = c"C:\\.local\\share\\winlux\\personnalisation.conf";

pub const BAR_POSITIONS: [&[u8]; 4] = [b"En bas", b"En haut", b"\xC0 gauche", b"\xC0 droite"];
pub const BAR_SIZES: [&[u8]; 3] = [b"Petite", b"Moyenne", b"Grande"];

pub fn bar_thickness() -> i32 {
    [40, 48, 56][unsafe { BAR_SIZE.min(2) } as usize]
}
pub fn bar_button() -> i32 {
    [36, 44, 52][unsafe { BAR_SIZE.min(2) } as usize]
}
pub fn bar_glyph() -> i32 {
    [20, 24, 28][unsafe { BAR_SIZE.min(2) } as usize]
}
pub fn bar_vertical() -> bool {
    unsafe { BAR_POS >= 2 }
}

// ---------- Couleurs d'accentuation (palette de Windows 11) ----------

const fn hex(v: u32) -> u32 {
    rgb((v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF)
}
pub const ACCENTS: [u32; 48] = [
    hex(0xFFB900),
    hex(0xFF8C00),
    hex(0xF7630C),
    hex(0xCA5010),
    hex(0xDA3B01),
    hex(0xEF6950),
    hex(0xD13438),
    hex(0xFF4343),
    hex(0xE74856),
    hex(0xE81123),
    hex(0xEA005E),
    hex(0xC30052),
    hex(0xE3008C),
    hex(0xBF0077),
    hex(0xC239B3),
    hex(0x9A0089),
    hex(0x0078D4),
    hex(0x0063B1),
    hex(0x8E8CD8),
    hex(0x6B69D6),
    hex(0x8764B8),
    hex(0x744DA9),
    hex(0xB146C2),
    hex(0x881798),
    hex(0x0099BC),
    hex(0x2D7D9A),
    hex(0x00B7C3),
    hex(0x038387),
    hex(0x00B294),
    hex(0x018574),
    hex(0x00CC6A),
    hex(0x10893E),
    hex(0x7A7574),
    hex(0x5D5A58),
    hex(0x68768A),
    hex(0x515C6B),
    hex(0x567C73),
    hex(0x486860),
    hex(0x498205),
    hex(0x107C10),
    hex(0x767676),
    hex(0x4C4A48),
    hex(0x69797E),
    hex(0x4A5459),
    hex(0x647C64),
    hex(0x525E54),
    hex(0x847545),
    hex(0x7E735F),
];

pub fn accent_base() -> u32 {
    unsafe {
        if ACCENT_INDEX >= 0 && (ACCENT_INDEX as usize) < ACCENTS.len() {
            ACCENTS[ACCENT_INDEX as usize]
        } else {
            WALLPAPERS[WALLPAPER.min(WALLPAPERS.len() - 1)].accent
        }
    }
}

unsafe fn apply_accent() {
    set_accent(accent_base());
}

/// Teinte une couleur de la barre ou du menu Démarrer quand l'accent y est affiché.
pub fn tint(c: u32) -> u32 {
    unsafe {
        if ACCENT_SURFACES {
            mix(c, ACCENT_BASE, 32, 100)
        } else {
            c
        }
    }
}
pub fn bar_bg() -> u32 {
    unsafe {
        if ACCENT_SURFACES {
            mix(ACCENT_BASE, rgb(0x10, 0x11, 0x16), 58, 100)
        } else {
            rgb(0x1C, 0x1F, 0x26)
        }
    }
}
pub fn bar_line() -> u32 {
    mix(bar_bg(), rgb(0xFF, 0xFF, 0xFF), 1, 10)
}
pub fn bar_hover() -> u32 {
    mix(bar_bg(), rgb(0xFF, 0xFF, 0xFF), 7, 100)
}
pub fn bar_focus() -> u32 {
    mix(bar_bg(), rgb(0xFF, 0xFF, 0xFF), 12, 100)
}
pub fn bar_tray() -> u32 {
    mix(bar_bg(), rgb(0xFF, 0xFF, 0xFF), 4, 100)
}

// ---------- Fonds d'écran ----------

pub const CATEGORIES: [&[u8]; 5] = [b"Tous", b"Nature", b"Abstrait", b"Sombre", b"Clair"];

pub struct Wallpaper {
    pub name: &'static [u8],
    pub slug: &'static [u8],
    pub category: usize,
    pub accent: u32,
}
pub const WALLPAPERS: [Wallpaper; 13] = [
    Wallpaper {
        name: b"Winlux",
        slug: b"winlux",
        category: 2,
        accent: hex(0x0078D4),
    },
    Wallpaper {
        name: b"Bloom",
        slug: b"bloom",
        category: 2,
        accent: hex(0x0078D4),
    },
    Wallpaper {
        name: b"Vagues",
        slug: b"vagues",
        category: 2,
        accent: hex(0x8764B8),
    },
    Wallpaper {
        name: b"Prisme",
        slug: b"prisme",
        category: 2,
        accent: hex(0xE3008C),
    },
    Wallpaper {
        name: b"Collines",
        slug: b"collines",
        category: 1,
        accent: hex(0x10893E),
    },
    Wallpaper {
        name: b"Cr\xE9puscule",
        slug: b"crepuscule",
        category: 1,
        accent: hex(0xF7630C),
    },
    Wallpaper {
        name: b"For\xEAt",
        slug: b"foret",
        category: 1,
        accent: hex(0x498205),
    },
    Wallpaper {
        name: b"Nuit \xE9toil\xE9e",
        slug: b"nuit",
        category: 3,
        accent: hex(0x6B69D6),
    },
    Wallpaper {
        name: b"Aurore bor\xE9\x61le",
        slug: b"aurore",
        category: 3,
        accent: hex(0x00B294),
    },
    Wallpaper {
        name: b"Graphite",
        slug: b"graphite",
        category: 3,
        accent: hex(0x68768A),
    },
    Wallpaper {
        name: b"Bloom clair",
        slug: b"bloom-clair",
        category: 4,
        accent: hex(0x0063B1),
    },
    Wallpaper {
        name: b"Pastel",
        slug: b"pastel",
        category: 4,
        accent: hex(0xC239B3),
    },
    Wallpaper {
        name: b"Dunes",
        slug: b"dunes",
        category: 4,
        accent: hex(0xCA5010),
    },
];

fn lerp(a: u32, b: u32, t: i32) -> u32 {
    mix(a, b, t.clamp(0, 1000), 1000)
}
fn grad(stops: &[(i32, u32)], t: i32) -> u32 {
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for i in 1..stops.len() {
        if t <= stops[i].0 {
            let (p0, c0) = stops[i - 1];
            let (p1, c1) = stops[i];
            return lerp(c0, c1, (t - p0) * 1000 / (p1 - p0).max(1));
        }
    }
    stops[stops.len() - 1].1
}
/// Mélange additif (lumière) : `k` sur 1000.
fn add(a: u32, b: u32, k: i32) -> u32 {
    let k = k.clamp(0, 1000) as u32;
    let ch = |s: u32| (((a >> s) & 0xFF) + ((b >> s) & 0xFF) * k / 1000).min(255);
    ch(0) | (ch(8) << 8) | (ch(16) << 16)
}
/// Sinus en millièmes, angle en dixièmes de degré (approximation de Bhaskara).
fn sin1k(dd: i32) -> i32 {
    let mut x = dd.rem_euclid(3600) as i64;
    let neg = x >= 1800;
    if neg {
        x -= 1800;
    }
    let p = x * (1800 - x);
    let v = (4000 * p / (4_050_000 - p)) as i32;
    if neg {
        -v
    } else {
        v
    }
}
fn hash(x: i32, y: i32, s: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ s.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^ (h >> 15)
}
fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}
fn dist(x: i32, y: i32, cx: i32, cy: i32) -> i32 {
    let (dx, dy) = ((x - cx) as i64, (y - cy) as i64);
    isqrt(dx * dx + dy * dy) as i32
}
/// Onde triangulaire 0..1000 de période `p`.
fn tri(x: i32, p: i32) -> i32 {
    let p = p.max(2);
    1000 - ((x.rem_euclid(p) * 2000 / p) - 1000).abs()
}

struct Bloom {
    top: u32,
    bottom: u32,
    petals: [(u32, u32); 3],
    cx: i32,
}
const BLOOM: Bloom = Bloom {
    top: hex(0x061233),
    bottom: hex(0x0C2E6E),
    petals: [
        (hex(0x0E3482), hex(0x306CD0)),
        (hex(0x1548A8), hex(0x4E96F0)),
        (hex(0x1E60D0), hex(0xB4D8FF)),
    ],
    cx: 500,
};
const BLOOM_LIGHT: Bloom = Bloom {
    top: hex(0xF1F5FC),
    bottom: hex(0xC9DAF2),
    petals: [
        (hex(0x8DB3EA), hex(0xBCD5F7)),
        (hex(0x6E9DE6), hex(0xA9C9F5)),
        (hex(0x4F86E0), hex(0xEEF5FF)),
    ],
    cx: 500,
};
const GRAPHITE: Bloom = Bloom {
    top: hex(0x26282E),
    bottom: hex(0x131417),
    petals: [
        (hex(0x2C2F37), hex(0x474C59)),
        (hex(0x363A44), hex(0x656C7C)),
        (hex(0x434855), hex(0xA9B1C2)),
    ],
    cx: 620,
};

/// Fleur stylisée : pétales concentriques (gradient continu du bord au cœur).
fn bloom(p: &Bloom, x: i32, y: i32, w: i32, h: i32) -> u32 {
    let mut c = lerp(p.top, p.bottom, y * 1000 / h);
    let r = h.min(w * 3 / 4) * 9 / 25;
    let cx = w * p.cx / 1000;
    let base = h * 3 / 5 + r / 3;
    let petals = [
        (-r * 6 / 5, r * 2 / 5, r * 6 / 5, 0),
        (r * 6 / 5, r * 2 / 5, r * 6 / 5, 0),
        (-r * 3 / 5, r * 9 / 20, r * 8 / 5, 1),
        (r * 3 / 5, r * 9 / 20, r * 8 / 5, 1),
        (0, r / 2, r * 2, 2),
    ];
    for (dx, rx, ry, k) in petals {
        let a = (x - cx - dx) as i64 * 1000 / rx.max(1) as i64;
        let b = (y - base) as i64 * 2000 / ry.max(1) as i64;
        if b < 0 {
            let f = ((a * a + b * b) / (-2 * b)) as i32;
            if f <= 1000 {
                let (o, i) = p.petals[k];
                let col = lerp(o, i, 1000 - f);
                c = lerp(c, col, (1000 - f) * 40);
            }
        }
    }
    c
}

fn shade(id: usize, x: i32, y: i32, w: i32, h: i32) -> u32 {
    let v = y * 1000 / h;
    let xd = x as i64 * 3600 / w as i64;
    let wave = |f10: i32, phase: i32| sin1k((xd * f10 as i64 / 10) as i32 + phase);
    match id {
        2 => {
            // Vagues : bandes ondulées violet → corail.
            let mut c = lerp(hex(0x1A0B3D), hex(0x2A1052), v);
            let n = 6;
            for k in (0..n).rev() {
                let yk = h * (280 + k * 115) / 1000
                    + h * (45 - k * 3) / 1000 * wave(10 + k * 3, k * 830) / 1000
                    + h * 15 / 1000 * wave(25 + k * 5, k * 370) / 1000;
                if y >= yk {
                    let col = grad(
                        &[
                            (0, hex(0x5B2A9E)),
                            (500, hex(0xD9467F)),
                            (1000, hex(0xFFA35C)),
                        ],
                        (k + 1) * 1000 / n,
                    );
                    let d = y - yk;
                    let lit = mix(col, rgb(0xFF, 0xFF, 0xFF), 3, 10);
                    c = lerp(lit, col, d * 1000 / (h * 5 / 100).max(1));
                    c = lerp(
                        c,
                        mix(col, 0, 3, 10),
                        (d - h * 6 / 100) * 1000 / (h / 6).max(1),
                    );
                    break;
                }
            }
            c
        }
        3 => {
            // Prisme : disques translucides sur fond diagonal.
            let u = x * 1000 / w;
            let mut c = grad(&[(0, hex(0x140C3C)), (1000, hex(0x3B0D4F))], (u + v) / 2);
            let (ax, ay, aw) = (x * 1000 / h, y * 1000 / h, w * 1000 / h);
            let disks = [
                (200, 300, 330, hex(0xFF3C8E), 550),
                (450, 650, 380, hex(0x7B5CFF), 520),
                (700, 250, 300, hex(0x2ED3FF), 460),
                (860, 760, 360, hex(0xFF9F43), 460),
                (560, 430, 190, hex(0xFFFFFF), 140),
            ];
            for (px, py, r, col, a) in disks {
                let d = dist(ax, ay, aw * px / 1000, py);
                if d < r {
                    let edge = ((r - d) * 1000 / (r * 4 / 100).max(1)).min(1000);
                    let inner = lerp(
                        col,
                        mix(col, rgb(0xFF, 0xFF, 0xFF), 3, 10),
                        (r - d) * 1000 / r,
                    );
                    c = lerp(c, inner, a * edge / 1000);
                }
            }
            let vx = (x - w / 2).abs() * 1000 / (w / 2).max(1);
            let vy = (y - h / 2).abs() * 1000 / (h / 2).max(1);
            lerp(c, 0, ((vx * vx + vy * vy) / 1000 - 700).max(0) / 3)
        }
        4 => {
            // Collines : ciel, soleil et quatre plans de collines.
            let mut c = grad(&[(0, hex(0x4F9BE0)), (560, hex(0xD6ECF8))], v);
            let d = dist(x, y, w * 74 / 100, h * 26 / 100) * 1000 / h;
            let glow = (1000 - d * 1000 / 380).max(0);
            c = lerp(c, hex(0xFFF4D6), glow * glow / 1000 * 8 / 10);
            if d < 42 {
                c = hex(0xFFFBEF);
            }
            let cols = [hex(0x9CC7A4), hex(0x6BAA67), hex(0x4A8C43), hex(0x2F6B2B)];
            for k in 0..4 {
                let ridge = h * (520 + k * 105) / 1000
                    - h * (70 - k * 8) / 1000 * wave(8 + k * 4, k * 900) / 1000
                    - h * 22 / 1000 * wave(23 + k * 7, k * 400) / 1000;
                if y >= ridge {
                    let col = cols[k as usize];
                    c = lerp(
                        col,
                        mix(col, 0, 25, 100),
                        (y - ridge) * 1000 / (h * 35 / 100),
                    );
                    if y - ridge < 2 {
                        c = mix(c, rgb(0xFF, 0xFF, 0xFF), 15, 100);
                    }
                }
            }
            c
        }
        5 => {
            // Crépuscule : ciel, soleil, montagnes et reflet sur le lac.
            let horizon = h * 700 / 1000;
            let land = |x: i32, y: i32| -> u32 {
                let v = y * 1000 / h;
                let mut c = grad(
                    &[
                        (0, hex(0x1F1640)),
                        (300, hex(0x5A2A6E)),
                        (520, hex(0xD2557A)),
                        (650, hex(0xFF9E6B)),
                        (700, hex(0xFFD08A)),
                    ],
                    v,
                );
                let r = h * 75 / 1000;
                let d = dist(x, y, w / 2, h * 560 / 1000);
                if d < r {
                    c = hex(0xFFF1C4);
                } else {
                    let g = (1000 - (d - r) * 1000 / (r * 5)).max(0);
                    c = add(c, hex(0x805020), g * g / 1000);
                }
                let far = horizon
                    - h * (60 + (140 * tri(x + w / 7, w * 2 / 5) + 60 * tri(x, w / 6)) / 1000)
                        / 1000;
                if y >= far {
                    c = lerp(hex(0x8A4A78), hex(0x5E3366), (y - far) * 1000 / (h / 8));
                }
                let near = horizon
                    - h * (20 + (90 * tri(x + w / 3, w / 3) + 30 * tri(x + 40, w / 9)) / 1000)
                        / 1000;
                if y >= near {
                    c = hex(0x3B1E4A);
                }
                c
            };
            if y <= horizon {
                land(x, y)
            } else {
                let depth = (y - horizon) * 1000 / (h - horizon).max(1);
                let mut c = land(x, (2 * horizon - y).max(0));
                c = lerp(c, hex(0x24122F), 420 + depth * 4 / 10);
                let spread = w * (30 + depth * 10 / 100) / 1000;
                if (x - w / 2).abs() < spread && hash(0, y / 2, 3) % 3 == 0 {
                    c = lerp(c, hex(0xFFD9A0), 380 - depth / 4);
                }
                c
            }
        }
        6 => {
            // Forêt : sapins en plans successifs dans la brume.
            let mut c = grad(
                &[
                    (0, hex(0xE4EFE4)),
                    (550, hex(0xA9C9B2)),
                    (1000, hex(0x6E9A7C)),
                ],
                v,
            );
            let d = dist(x, y, w * 30 / 100, -h / 10) * 1000 / h;
            c = add(c, hex(0x302810), (1000 - d * 1000 / 900).max(0));
            let cols = [hex(0xA3C2AC), hex(0x76A084), hex(0x4A7559), hex(0x23402E)];
            for k in 0..4i32 {
                let yb = h * (560 + k * 125) / 1000;
                let s = (h * (60 + k * 30) / 1000).max(4);
                let height = h * (250 + k * 95) / 1000;
                let off = k * 977;
                let n = (x + off).div_euclid(s);
                let mut inside = y >= yb;
                for nn in n - 1..=n + 1 {
                    if inside {
                        break;
                    }
                    let hs = hash(nn, k, 11);
                    let cx = nn * s + s / 2 - off + (hs % (s as u32 / 3 + 1)) as i32 - s / 6;
                    let hn = height * (700 + (hs >> 8) as i32 % 500) / 1000;
                    let top = yb - hn;
                    if y >= top {
                        let depth = y - top;
                        let tier = hn / 4;
                        let half = depth * 30 / 100 - (depth % tier.max(1)) * 10 / 100;
                        inside = (x - cx).abs() <= half.max(1);
                    }
                }
                if inside {
                    c = cols[k as usize];
                }
            }
            c
        }
        7 => {
            // Nuit étoilée : voie lactée, étoiles, lune et collines.
            let mut c = grad(
                &[
                    (0, hex(0x03060F)),
                    (650, hex(0x0F1A3A)),
                    (1000, hex(0x1A2850)),
                ],
                v,
            );
            let line = h * 150 / 1000 + (x as i64 * h as i64 * 500 / (w as i64 * 1000)) as i32;
            let glow = (1000 - (y - line).abs() * 1000 / (h * 200 / 1000).max(1)).max(0);
            c = add(c, hex(0x2A3570), glow * glow / 1000 * 6 / 10);
            let cell = (h / 160).max(3);
            let (gx, gy) = (x / cell, y / cell);
            let hs = hash(gx, gy, 7);
            if hs % 1000 < 22 {
                let sx = gx * cell + ((hs >> 10) % cell as u32) as i32;
                let sy = gy * cell + ((hs >> 14) % cell as u32) as i32;
                let b = 350 + ((hs >> 18) % 650) as i32;
                if x == sx && y == sy {
                    c = lerp(c, rgb(0xFF, 0xFF, 0xF4), b);
                } else if hs % 1000 < 3 && (x - sx).abs() + (y - sy).abs() <= 1 {
                    c = lerp(c, rgb(0xDD, 0xE6, 0xFF), b / 2);
                }
            }
            let (mx, my, r) = (w * 80 / 100, h * 22 / 100, h * 55 / 1000);
            let d = dist(x, y, mx, my);
            if d <= r && dist(x, y, mx + r * 45 / 100, my - r * 20 / 100) > r * 95 / 100 {
                c = hex(0xF4F1E4);
            } else if d < r * 4 {
                let g = 1000 - d * 1000 / (r * 4);
                c = add(c, hex(0x1E2A50), g * g / 1000);
            }
            let ridge = h * 880 / 1000
                - h * 35 / 1000 * wave(9, 300) / 1000
                - h * 15 / 1000 * wave(27, 0) / 1000;
            if y >= ridge {
                c = hex(0x05080F);
            }
            c
        }
        8 => {
            // Aurore boréale : rideaux lumineux additifs.
            let mut c = lerp(hex(0x020A14), hex(0x0A2234), v);
            let hs = hash(x / 2, y / 2, 5);
            if hs % 1000 < 4 {
                c = lerp(c, rgb(0xFF, 0xFF, 0xFF), 300 + (hs >> 20) as i32 % 500);
            }
            let curtains = [
                (hex(0x3DF5A5), hex(0xA066FF)),
                (hex(0x2BD9E0), hex(0x5A7CFF)),
                (hex(0x5CFF8F), hex(0x2BD9E0)),
            ];
            for k in 0..3i32 {
                let ck = h * (300 + k * 95) / 1000
                    + h * 70 / 1000 * wave(9 + k * 5, k * 1100) / 1000
                    + h * 25 / 1000 * wave(31 + k * 9, k * 500) / 1000;
                let d = y - ck;
                let up = h * (230 - k * 40) / 1000;
                let mut i = if d < 0 {
                    (1000 + d * 1000 / up.max(1)).max(0)
                } else {
                    (1000 - d * 1000 / (h * 25 / 1000).max(1)).max(0)
                };
                i = i * i / 1000;
                i = i * (650 + 350 * wave(120 + k * 37, k * 999) / 1000) / 1000;
                let (lo, hi) = curtains[k as usize];
                let col = lerp(lo, hi, (-d).max(0) * 1000 / up.max(1));
                c = add(c, col, i * 8 / 10);
            }
            let ridge = h * (830 + (60 * tri(x + w / 5, w / 3)) / 1000) / 1000
                - h * 25 / 1000 * wave(14, 0) / 1000;
            if y >= ridge {
                c = lerp(
                    hex(0x0A1826),
                    hex(0x02060C),
                    (y - ridge) * 1000 / (h / 10).max(1),
                );
            }
            c
        }
        9 => bloom(&GRAPHITE, x, y, w, h),
        10 => bloom(&BLOOM_LIGHT, x, y, w, h),
        11 => {
            // Pastel : halos colorés très doux.
            let u = x * 1000 / w;
            let mut c = grad(
                &[
                    (0, hex(0xFDE2E4)),
                    (500, hex(0xE2ECF9)),
                    (1000, hex(0xCDEFEA)),
                ],
                (u + v) / 2,
            );
            let (ax, ay, aw) = (x * 1000 / h, y * 1000 / h, w * 1000 / h);
            let blobs = [
                (150, 250, 420, hex(0xFFD6A5)),
                (520, 180, 380, hex(0xBDB2FF)),
                (880, 420, 460, hex(0x9BF6FF)),
                (350, 820, 450, hex(0xFFC6FF)),
                (760, 880, 380, hex(0xCAFFBF)),
            ];
            for (px, py, r, col) in blobs {
                let d = dist(ax, ay, aw * px / 1000, py);
                if d < r {
                    let t = (r - d) * 1000 / r;
                    c = lerp(c, col, t * t / 1000 * 75 / 100);
                }
            }
            c
        }
        12 => {
            // Dunes : crêtes éclairées d'un côté, ombrées de l'autre.
            let mut c = grad(&[(0, hex(0x9FCFF0)), (450, hex(0xF7E9D4))], v);
            let d = dist(x, y, w / 4, h / 5) * 1000 / h;
            c = add(c, hex(0x40301A), (1000 - d * 1000 / 500).max(0));
            let cols = [hex(0xF3DCB2), hex(0xEBC48D), hex(0xDDA66A), hex(0xC98848)];
            let ridge = |x: i32, k: i32| -> i32 {
                let xd = x as i64 * 3600 / w as i64;
                h * (470 + k * 135) / 1000
                    - h * (60 - k * 5) / 1000
                        * sin1k((xd * (7 + k as i64 * 3) / 10) as i32 + k * 1300)
                        / 1000
                    - h * 20 / 1000 * sin1k((xd * (19 + k as i64 * 4) / 10) as i32 + k * 333) / 1000
            };
            for k in 0..4 {
                let r = ridge(x, k);
                if y >= r {
                    let col = cols[k as usize];
                    let slope = ridge(x + 6, k) - ridge(x - 6, k);
                    let side = if slope > 0 {
                        mix(col, rgb(0xFF, 0xF6, 0xE6), 22, 100)
                    } else {
                        mix(col, hex(0x6E4420), 12, 100)
                    };
                    c = lerp(side, col, (y - r) * 1000 / (h / 12).max(1));
                }
            }
            c
        }
        _ => bloom(&BLOOM, x, y, w, h),
    }
}

/// Image calculée en mémoire (DIB 32 bits), conservée tant que rien ne change.
#[derive(Clone, Copy)]
struct Canvas {
    dc: Handle,
    bmp: Handle,
    bits: *mut u32,
    w: i32,
    h: i32,
    id: usize,
}
const NO_CANVAS: Canvas = Canvas {
    dc: core::ptr::null_mut(),
    bmp: core::ptr::null_mut(),
    bits: core::ptr::null_mut(),
    w: 0,
    h: 0,
    id: usize::MAX,
};
static mut FULL: Canvas = NO_CANVAS;
static mut PREVIEW: Canvas = NO_CANVAS;
static mut THUMBS: [Canvas; 13] = [NO_CANVAS; 13];

unsafe fn canvas(c: &mut Canvas, hdc: Handle, w: i32, h: i32, id: usize) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    if c.dc.is_null() || c.w != w || c.h != h {
        if !c.dc.is_null() {
            DeleteDC(c.dc);
            DeleteObject(c.bmp);
            *c = NO_CANVAS;
        }
        let info = BitmapInfoHeader {
            size: 40,
            width: w,
            height: h,
            planes: 1,
            bit_count: 32,
            compression: 0,
            ..Default::default()
        };
        let dc = CreateCompatibleDC(hdc);
        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let bmp = CreateDIBSection(dc, &info, 0, &mut bits, core::ptr::null_mut(), 0);
        if bmp.is_null() || bits.is_null() {
            DeleteDC(dc);
            return false;
        }
        SelectObject(dc, bmp);
        *c = Canvas {
            dc,
            bmp,
            bits: bits as *mut u32,
            w,
            h,
            id: usize::MAX,
        };
    }
    if c.id != id {
        for y in 0..h {
            // DIB « bottom-up » : la première ligne en mémoire est celle du bas.
            let row = c.bits.add(((h - 1 - y) * w) as usize);
            for x in 0..w {
                let col = shade(id, x, y, w, h);
                let n = (hash(x, y, 1) & 3) as i32 - 1;
                let ch = |s: u32| (((col >> s) & 0xFF) as i32 + n).clamp(0, 255) as u32;
                *row.add(x as usize) = (ch(0) << 16) | (ch(8) << 8) | ch(16);
            }
        }
        c.id = id;
    }
    true
}

static mut IMG_DC: Handle = core::ptr::null_mut();
static mut IMG_W: i32 = 0;
static mut IMG_H: i32 = 0;
static mut IMG_TRIED: bool = false;

/// Image système /usr/share/winlux/wallpaper.bmp (ou WINLUX_WALLPAPER).
unsafe fn load_image(hdc: Handle) -> bool {
    if IMG_TRIED {
        return !IMG_DC.is_null();
    }
    IMG_TRIED = true;
    let mut selected = [0i8; 1024];
    let length = GetEnvironmentVariableA(
        c"WINLUX_WALLPAPER".as_ptr(),
        selected.as_mut_ptr(),
        selected.len() as u32,
    );
    let path = if length > 0 && length < selected.len() as u32 {
        selected.as_ptr()
    } else {
        c"Z:\\usr\\share\\winlux\\wallpaper.bmp".as_ptr()
    };
    let f = CreateFileA(
        path,
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f == -1isize as Handle {
        return false;
    }
    let fsize = GetFileSize(f, core::ptr::null_mut());
    let mut hdr = [0u8; 54];
    let mut read = 0u32;
    if fsize >= 66 {
        ReadFile(f, hdr.as_mut_ptr(), 54, &mut read, core::ptr::null_mut());
    }
    let off = u32::from_le_bytes([hdr[10], hdr[11], hdr[12], hdr[13]]);
    let w = i32::from_le_bytes([hdr[18], hdr[19], hdr[20], hdr[21]]);
    let h = i32::from_le_bytes([hdr[22], hdr[23], hdr[24], hdr[25]]);
    let bpp = u16::from_le_bytes([hdr[28], hdr[29]]);
    if read != 54
        || hdr[0] != b'B'
        || hdr[1] != b'M'
        || w <= 0
        || h == 0
        || (bpp != 24 && bpp != 32)
    {
        CloseHandle(f);
        return false;
    }
    let abs_h = h.abs();
    let stride = ((w as u32 * bpp as u32 / 8 + 3) & !3) as usize;
    let pixel_bytes = stride * abs_h as usize;
    let info = BitmapInfoHeader {
        size: 40,
        width: w,
        height: abs_h,
        planes: 1,
        bit_count: bpp,
        compression: 0,
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
    let dc = CreateCompatibleDC(hdc);
    let dib = CreateDIBSection(dc, &info, 0, &mut bits, core::ptr::null_mut(), 0);
    if dib.is_null() || bits.is_null() {
        DeleteDC(dc);
        CloseHandle(f);
        return false;
    }
    SelectObject(dc, dib);
    SetFilePointer(f, off as i32, core::ptr::null_mut(), 0);
    let dst = bits as *mut u8;
    let mut total = 0usize;
    while total < pixel_bytes {
        let chunk = (pixel_bytes - total).min(65536) as u32;
        let mut got = 0u32;
        ReadFile(f, dst.add(total), chunk, &mut got, core::ptr::null_mut());
        if got == 0 {
            break;
        }
        total += got as usize;
    }
    CloseHandle(f);
    if total < pixel_bytes {
        DeleteObject(dib);
        DeleteDC(dc);
        return false;
    }
    IMG_DC = dc;
    IMG_W = w;
    IMG_H = abs_h;
    true
}

/// Dessine le fond `id` dans `r` ; `slot` choisit le cache utilisé.
unsafe fn paint_wallpaper(hdc: Handle, r: &Rect, id: usize, slot: &mut Canvas) {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if id == 0 && load_image(hdc) {
        StretchBlt(
            hdc,
            r.left,
            r.top,
            w,
            h,
            IMG_DC,
            0,
            0,
            IMG_W,
            IMG_H,
            0x00CC_0020,
        );
        return;
    }
    let shader = if id == 0 { 1 } else { id };
    if canvas(slot, hdc, w, h, shader) {
        BitBlt(hdc, r.left, r.top, w, h, slot.dc, 0, 0, 0x00CC_0020);
    }
}

/// Fond du bureau, plein écran.
pub unsafe fn paint_desktop(hdc: Handle, w: i32, h: i32) {
    paint_wallpaper(
        hdc,
        &rect(0, 0, w, h),
        WALLPAPER,
        &mut *core::ptr::addr_of_mut!(FULL),
    );
}

// ---------- Sons de démarrage ----------

pub const SOUNDS: [(&[u8], &[u8], &[u8]); 6] = [
    (b"Aucun", b"none", b"D\xE9marrage silencieux."),
    (
        b"Aurore",
        b"aurore",
        b"Arp\xE8ge chaleureux, inspir\xE9 de Windows 11.",
    ),
    (
        b"Cristal",
        b"cristal",
        b"Clochettes claires et l\xE9g\xE8res.",
    ),
    (b"Brise", b"brise", b"Nappe douce qui monte lentement."),
    (b"Horizon", b"horizon", b"Deux notes ascendantes, sobres."),
    (
        b"Pulsation",
        b"pulsation",
        b"Signal num\xE9rique bref et moderne.",
    ),
];

const RATE: u32 = 22050;
const MAX_SAMPLES: usize = (RATE as usize) * 27 / 10;
static mut MIXBUF: [i32; MAX_SAMPLES] = [0; MAX_SAMPLES];
static mut WAV: [u8; 44 + MAX_SAMPLES * 2] = [0; 44 + MAX_SAMPLES * 2];
static mut SINE: [i16; 1024] = [0; 1024];

#[derive(Clone, Copy)]
enum Voice {
    Sine,
    Bell,
    Pad,
    Square,
}

fn sine(phase: u32) -> i32 {
    unsafe { SINE[(phase >> 22) as usize] as i32 }
}

/// Ajoute une note (fréquence en millihertz) au mélange.
unsafe fn note(at_ms: u32, mhz: u32, len_ms: u32, amp: i32, voice: Voice) {
    let start = (at_ms * RATE / 1000) as usize;
    let len = (len_ms * RATE / 1000) as usize;
    let inc = ((mhz as u64) << 32) / (1000 * RATE as u64);
    let attack = match voice {
        Voice::Pad => RATE as usize * 35 / 100,
        Voice::Bell => RATE as usize * 4 / 1000,
        _ => RATE as usize * 12 / 1000,
    };
    let mut phase = 0u32;
    let mut phase2 = 0u32;
    let inc2 = (inc * 276 / 100) as u32;
    for n in 0..len {
        let i = start + n;
        if i >= MAX_SAMPLES {
            break;
        }
        let rest = ((len - n) * 1000 / len) as i32;
        let env = match voice {
            Voice::Pad => {
                let up = if n < attack {
                    (n * 1000 / attack) as i32
                } else {
                    1000
                };
                up.min(rest * 2)
            }
            Voice::Bell => rest * rest / 1000 * rest / 1000,
            _ => rest * rest / 1000,
        };
        let env = if n < attack && !matches!(voice, Voice::Pad) {
            env * (n * 1000 / attack) as i32 / 1000
        } else {
            env
        };
        let s = match voice {
            Voice::Sine | Voice::Pad => {
                sine(phase) * 85 / 100 + sine(phase.wrapping_mul(2)) * 15 / 100
            }
            Voice::Bell => sine(phase) * 70 / 100 + sine(phase2) * 30 / 100 * rest / 1000,
            Voice::Square => {
                sine(phase) * 60 / 100
                    + sine(phase.wrapping_mul(3)) * 20 / 100
                    + sine(phase.wrapping_mul(5)) * 12 / 100
            }
        };
        MIXBUF[i] += s * amp / 1000 * env / 1000;
        phase = phase.wrapping_add(inc as u32);
        phase2 = phase2.wrapping_add(inc2);
    }
}

/// Synthétise le son `id` en WAV PCM 16 bits mono ; renvoie la taille.
unsafe fn synth(id: usize) -> usize {
    if SINE[256] == 0 {
        for i in 0..1024 {
            SINE[i] = (sin1k((i * 3600 / 1024) as i32) * 32) as i16;
        }
    }
    for s in MIXBUF.iter_mut() {
        *s = 0;
    }
    let mut end_ms = 0;
    let mut n = |at: u32, mhz: u32, len: u32, amp: i32, v: Voice| {
        note(at, mhz, len, amp, v);
        end_ms = end_ms.max(at + len);
    };
    match id {
        1 => {
            n(0, 196_000, 2100, 260, Voice::Pad);
            for (k, f) in [392_000, 493_880, 587_330, 783_990].iter().enumerate() {
                n(k as u32 * 110, *f, 1700 - k as u32 * 80, 330, Voice::Sine);
            }
        }
        2 => {
            for (k, f) in [1_046_500, 1_318_510, 1_567_980, 2_093_000]
                .iter()
                .enumerate()
            {
                n(k as u32 * 80, *f, 1100, 300, Voice::Bell);
            }
            n(320, 2_637_020, 900, 160, Voice::Bell);
        }
        3 => {
            for (k, f) in [220_000, 329_630, 440_000, 554_370].iter().enumerate() {
                n(k as u32 * 70, *f, 2300, 240, Voice::Pad);
            }
        }
        4 => {
            n(0, 587_330, 900, 380, Voice::Sine);
            n(240, 880_000, 1500, 380, Voice::Sine);
            n(240, 440_000, 1500, 220, Voice::Pad);
        }
        5 => {
            for (k, f) in [659_250, 880_000, 1_318_510].iter().enumerate() {
                n(k as u32 * 120, *f, 140, 260, Voice::Square);
            }
            n(380, 1_760_000, 650, 300, Voice::Sine);
            n(380, 880_000, 650, 160, Voice::Sine);
        }
        _ => return 0,
    }
    let samples = ((end_ms * RATE / 1000) as usize).min(MAX_SAMPLES);
    let peak = MIXBUF[..samples].iter().fold(1, |m, s| m.max(s.abs()));
    let gain = 1000 * 22000 / peak.max(22000);
    let data = samples * 2;
    let header = |o: usize, v: &[u8]| WAV[o..o + v.len()].copy_from_slice(v);
    header(0, b"RIFF");
    header(4, &((36 + data) as u32).to_le_bytes());
    header(8, b"WAVEfmt ");
    header(16, &16u32.to_le_bytes());
    header(20, &1u16.to_le_bytes());
    header(22, &1u16.to_le_bytes());
    header(24, &RATE.to_le_bytes());
    header(28, &(RATE * 2).to_le_bytes());
    header(32, &2u16.to_le_bytes());
    header(34, &16u16.to_le_bytes());
    header(36, b"data");
    header(40, &(data as u32).to_le_bytes());
    for i in 0..samples {
        let v = (MIXBUF[i] * gain / 1000).clamp(-32767, 32767) as i16;
        WAV[44 + i * 2..46 + i * 2].copy_from_slice(&v.to_le_bytes());
    }
    44 + data
}

unsafe fn ensure_dirs() {
    for dir in [
        c"C:\\.local",
        c"C:\\.local\\share",
        c"C:\\.local\\share\\winlux",
        c"C:\\.local\\share\\winlux\\sons",
    ] {
        CreateDirectoryA(dir.as_ptr(), core::ptr::null_mut());
    }
}

/// Joue le son `id` (aperçu ou démarrage), sans bloquer le bureau.
pub unsafe fn play_sound(id: usize) -> bool {
    PlaySoundA(core::ptr::null(), core::ptr::null_mut(), 0x0040);
    let size = synth(id);
    if size == 0 {
        return true;
    }
    ensure_dirs();
    let mut path = [0u8; 96];
    let prefix = b"C:\\.local\\share\\winlux\\sons\\";
    let slug = SOUNDS[id].1;
    path[..prefix.len()].copy_from_slice(prefix);
    path[prefix.len()..prefix.len() + slug.len()].copy_from_slice(slug);
    path[prefix.len() + slug.len()..prefix.len() + slug.len() + 4].copy_from_slice(b".wav");
    let f = CreateFileA(
        path.as_ptr().cast(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f as isize == -1 {
        return false;
    }
    let mut written = 0;
    let ok = WriteFile(
        f,
        core::ptr::addr_of!(WAV).cast(),
        size as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(f);
    ok != 0 && PlaySoundA(path.as_ptr().cast(), core::ptr::null_mut(), 0x0002_0001) != 0
}

/// Ouverture de session : son de démarrage (une seule fois).
pub unsafe fn on_session_start() {
    if SOUND_PLAYED {
        return;
    }
    SOUND_PLAYED = true;
    if SOUND > 0 && SOUND < SOUNDS.len() && !OOBE_ACTIVE {
        play_sound(SOUND);
    }
}

// ---------- Lecture / écriture ----------

fn field<'a>(data: &'a [u8], key: &[u8]) -> &'a [u8] {
    for line in data.split(|&c| c == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.len() > key.len() && line.starts_with(key) && line[key.len()] == b'=' {
            return &line[key.len() + 1..];
        }
    }
    b""
}
fn number(s: &[u8]) -> Option<i32> {
    if s.is_empty() || !s.iter().all(u8::is_ascii_digit) || s.len() > 6 {
        return None;
    }
    Some(s.iter().fold(0, |n, &c| n * 10 + (c - b'0') as i32))
}

/// Charge les préférences ; sans fichier, l'assistant du premier démarrage s'ouvrira.
pub unsafe fn load() {
    let f = CreateFileA(
        CONF.as_ptr(),
        0x8000_0000,
        1,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    let mut data = [0u8; 1024];
    let mut n = 0u32;
    let found = !f.is_null() && f as isize != -1;
    if found {
        ReadFile(
            f,
            data.as_mut_ptr(),
            data.len() as u32,
            &mut n,
            core::ptr::null_mut(),
        );
        CloseHandle(f);
    }
    let data = &data[..n as usize];
    if let Some(i) = WALLPAPERS
        .iter()
        .position(|w| w.slug == field(data, b"wallpaper"))
    {
        WALLPAPER = i;
    }
    ACCENT_INDEX = match number(field(data, b"accent")) {
        Some(i) if (i as usize) < ACCENTS.len() => i,
        _ => -1,
    };
    ACCENT_SURFACES = field(data, b"accent_surfaces") == b"1";
    ACCENT_TITLES = field(data, b"accent_titles") == b"1";
    BAR_SIZE = match field(data, b"taskbar_size") {
        b"small" => 0,
        b"large" => 2,
        _ => 1,
    };
    BAR_POS = match field(data, b"taskbar_position") {
        b"top" => 1,
        b"left" => 2,
        b"right" => 3,
        _ => 0,
    };
    match field(data, b"taskbar_align") {
        b"left" => crate::desktop::ALIGN_LEFT = true,
        b"center" => crate::desktop::ALIGN_LEFT = false,
        _ => {}
    }
    if let Some(i) = SOUNDS
        .iter()
        .position(|s| s.1 == field(data, b"startup_sound"))
    {
        SOUND = i;
    }
    apply_accent();
    let mut v = [0i8; 4];
    let forced = GetEnvironmentVariableA(c"WINLUX_OOBE".as_ptr(), v.as_mut_ptr(), 4) > 0;
    let skipped = GetEnvironmentVariableA(c"WINLUX_NO_OOBE".as_ptr(), v.as_mut_ptr(), 4) > 0;
    OOBE_ACTIVE = forced || (!skipped && field(data, b"oobe") != b"done");
    OOBE_STEP = 0;
}

unsafe fn save() {
    ensure_dirs();
    let mut out = [0u8; 512];
    let mut len = 0;
    let mut put = |k: &[u8], v: &[u8]| {
        for part in [k, b"=", v, b"\n"] {
            let n = part.len().min(out.len() - len);
            out[len..len + n].copy_from_slice(&part[..n]);
            len += n;
        }
    };
    put(b"wallpaper", WALLPAPERS[WALLPAPER].slug);
    let mut digits = [0u8; 2];
    let accent: &[u8] = if ACCENT_INDEX < 0 {
        b"auto"
    } else {
        let i = ACCENT_INDEX as usize;
        if i < 10 {
            digits[0] = b'0' + i as u8;
            &digits[..1]
        } else {
            digits = [b'0' + (i / 10) as u8, b'0' + (i % 10) as u8];
            &digits[..]
        }
    };
    put(b"accent", accent);
    put(
        b"accent_surfaces",
        if ACCENT_SURFACES { b"1" } else { b"0" },
    );
    put(b"accent_titles", if ACCENT_TITLES { b"1" } else { b"0" });
    put(
        b"taskbar_size",
        [b"small".as_slice(), b"medium", b"large"][BAR_SIZE.min(2) as usize],
    );
    put(
        b"taskbar_position",
        [b"bottom".as_slice(), b"top", b"left", b"right"][BAR_POS.min(3) as usize],
    );
    put(
        b"taskbar_align",
        if crate::desktop::ALIGN_LEFT {
            b"left"
        } else {
            b"center"
        },
    );
    put(b"startup_sound", SOUNDS[SOUND].1);
    put(b"oobe", if OOBE_ACTIVE { b"pending" } else { b"done" });
    let f = CreateFileA(
        CONF.as_ptr(),
        0x4000_0000,
        0,
        core::ptr::null_mut(),
        2,
        0,
        core::ptr::null_mut(),
    );
    if f.is_null() || f as isize == -1 {
        crate::desktop::toast(b"Impossible d'enregistrer la personnalisation.");
        return;
    }
    let mut written = 0;
    WriteFile(
        f,
        out.as_ptr(),
        len as u32,
        &mut written,
        core::ptr::null_mut(),
    );
    CloseHandle(f);
}

// ---------- Panneaux (Paramètres et assistant) ----------

/// Icônes système d'abord, raccourcis d'applications ensuite.
const ICON_ORDER: [usize; 7] = [0, 1, 2, 5, 6, 3, 4];

pub const SECTIONS: [&[u8]; 5] = [
    b"Arri\xE8re-plan",
    b"Couleurs",
    b"Barre des t\xE2ches",
    b"Bureau",
    b"Sons",
];

const MAX_ITEMS: usize = 72;
struct Items {
    list: [(i32, Rect); MAX_ITEMS],
    len: usize,
}
impl Items {
    fn new() -> Self {
        Items {
            list: [(0, rect(0, 0, 0, 0)); MAX_ITEMS],
            len: 0,
        }
    }
    fn push(&mut self, id: i32, r: Rect) {
        if self.len < MAX_ITEMS {
            self.list[self.len] = (id, r);
            self.len += 1;
        }
    }
    fn hit(&self, x: i32, y: i32) -> i32 {
        self.list[..self.len]
            .iter()
            .rev()
            .find(|(_, r)| inside(r, x, y))
            .map_or(-1, |(id, _)| *id)
    }
}

fn pill_w(label: &[u8]) -> i32 {
    text_width(label, 13) + 28
}

fn thumb_size(a: &Rect, _compact: bool) -> (i32, i32, i32) {
    let tw = 112;
    let gap = 12;
    let cols = ((a.right - a.left + gap) / (tw + gap)).max(1);
    (tw, tw * 9 / 16, cols)
}

/// Zones cliquables d'une section : la même géométrie sert au dessin et aux clics.
unsafe fn items(section: usize, a: &Rect, compact: bool) -> Items {
    let mut it = Items::new();
    let w = a.right - a.left;
    match section {
        0 => {
            let mut x = a.left;
            for (i, c) in CATEGORIES.iter().enumerate() {
                let pw = pill_w(c);
                it.push(i as i32, rect(x, a.top, x + pw, a.top + 32));
                x += pw + 8;
            }
            let (tw, th, cols) = thumb_size(a, compact);
            let mut n = 0;
            for (i, wp) in WALLPAPERS.iter().enumerate() {
                if BG_CATEGORY != 0 && wp.category != BG_CATEGORY {
                    continue;
                }
                let (col, row) = (n % cols, n / cols);
                let x = a.left + col * (tw + 12);
                let y = a.top + 48 + row * (th + 30);
                if y + th + 22 > a.bottom {
                    break;
                }
                it.push(100 + i as i32, rect(x, y, x + tw, y + th));
                n += 1;
            }
        }
        1 => {
            let mut x = a.left + 150;
            for (i, label) in [b"Sombre".as_slice(), b"Clair"].iter().enumerate() {
                let pw = pill_w(label).max(96);
                it.push(63 + i as i32, rect(x, a.top, x + pw, a.top + 32));
                x += pw + 8;
            }
            it.push(60, rect(a.left, a.top + 48, a.right, a.top + 84));
            let cols = 12;
            let s = ((w - (cols - 1) * 6) / cols).clamp(18, 34);
            for i in 0..ACCENTS.len() as i32 {
                let x = a.left + (i % cols) * (s + 6);
                let y = a.top + 96 + (i / cols) * (s + 6);
                it.push(i, rect(x, y, x + s, y + s));
            }
            let y = a.top + 96 + 4 * (s + 6) + 10;
            it.push(61, rect(a.left, y, a.right, y + 36));
            it.push(62, rect(a.left, y + 42, a.right, y + 78));
        }
        2 => {
            let cw = ((w - 3 * 10) / 4).min(124);
            let ch = if compact { 74 } else { 82 };
            for i in 0..4 {
                let x = a.left + i * (cw + 10);
                it.push(10 + i, rect(x, a.top + 22, x + cw, a.top + 22 + ch));
            }
            let y = a.top + 22 + ch + 40;
            for i in 0..3 {
                let x = a.left + i * (cw + 10);
                it.push(i, rect(x, y, x + cw, y + ch));
            }
            let y = y + ch + 40;
            for i in 0..2 {
                let x = a.left + i * (cw + 10);
                it.push(20 + i, rect(x, y, x + cw, y + ch));
            }
        }
        3 => {
            let cols = if w >= 520 { 2 } else { 1 };
            let cw = (w - (cols - 1) * 12) / cols;
            for (i, &icon) in ICON_ORDER.iter().enumerate() {
                let i = i as i32;
                let x = a.left + (i % cols) * (cw + 12);
                let y = a.top + 30 + (i / cols) * 52;
                it.push(icon as i32, rect(x, y, x + cw, y + 44));
            }
        }
        _ => {
            for i in 0..SOUNDS.len() as i32 {
                let y = a.top + i * 56;
                let r = rect(a.left, y, a.right, y + 48);
                it.push(i, r);
                it.push(10 + i, rect(r.right - 112, y + 8, r.right - 12, y + 40));
            }
        }
    }
    it
}

unsafe fn pill(g: &mut Gfx, r: &Rect, label: &[u8], selected: bool, hover: bool) {
    let p = pal();
    let (bg, fg) = if selected {
        (p.accent, p.on_accent)
    } else if hover {
        (p.hover, p.text)
    } else {
        (p.surface2, p.text)
    };
    round_rect(
        g.hdc,
        g.b,
        r.left,
        r.top,
        r.right,
        r.bottom,
        (r.bottom - r.top) / 2,
        if selected { p.accent } else { p.border },
    );
    round_rect(
        g.hdc,
        g.b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        (r.bottom - r.top) / 2 - 1,
        bg,
    );
    text_centered(
        g.hdc,
        g.f.body,
        13,
        fg,
        (r.left + r.right) / 2,
        (r.top + r.bottom) / 2 - 8,
        label,
    );
}

unsafe fn switch(g: &mut Gfx, x: i32, y: i32, on: bool) {
    let p = pal();
    if on {
        round_rect(g.hdc, g.b, x, y, x + 40, y + 20, 10, p.accent);
        ellipse(g.hdc, g.b, x + 24, y + 4, x + 36, y + 16, p.on_accent);
    } else {
        round_rect(g.hdc, g.b, x, y, x + 40, y + 20, 10, p.text2);
        round_rect(g.hdc, g.b, x + 1, y + 1, x + 39, y + 19, 9, p.surface);
        ellipse(g.hdc, g.b, x + 5, y + 5, x + 15, y + 15, p.text2);
    }
}

unsafe fn card(g: &mut Gfx, r: &Rect, selected: bool, hover: bool) {
    let p = pal();
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
    let inner = if hover && !selected {
        p.hover
    } else {
        p.surface
    };
    let t = if selected { 2 } else { 1 };
    round_rect(
        g.hdc,
        g.b,
        r.left + t,
        r.top + t,
        r.right - t,
        r.bottom - t,
        7 - t,
        inner,
    );
}

fn label_of(section: usize) -> &'static [u8] {
    match section {
        0 => b"Choisissez une image : l'aper\xE7u est imm\xE9diat.",
        1 => b"L'accent colore la s\xE9lection, les boutons et les indicateurs.",
        2 => b"Emplacement, taille et alignement de la barre des t\xE2ches.",
        3 => b"Cochez les ic\xF4nes \xE0 afficher sur le bureau.",
        _ => b"Son jou\xE9 \xE0 l'ouverture de la session.",
    }
}

/// Mini-diagramme d'un écran avec sa barre (cartes Position/Taille/Alignement).
unsafe fn screen_glyph(
    g: &mut Gfx,
    cx: i32,
    top: i32,
    pos: u8,
    thick: i32,
    centered: bool,
    sel: bool,
) {
    let p = pal();
    let (w, h) = (48, 30);
    let r = rect(cx - w / 2, top, cx + w / 2, top + h);
    round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 3, p.text3);
    fill(
        g.hdc,
        g.b,
        r.left + 1,
        r.top + 1,
        r.right - 1,
        r.bottom - 1,
        p.surface2,
    );
    let bar = match pos {
        1 => rect(r.left + 1, r.top + 1, r.right - 1, r.top + 1 + thick),
        2 => rect(r.left + 1, r.top + 1, r.left + 1 + thick, r.bottom - 1),
        3 => rect(r.right - 1 - thick, r.top + 1, r.right - 1, r.bottom - 1),
        _ => rect(r.left + 1, r.bottom - 1 - thick, r.right - 1, r.bottom - 1),
    };
    fill(
        g.hdc,
        g.b,
        bar.left,
        bar.top,
        bar.right,
        bar.bottom,
        if sel { p.accent } else { p.text2 },
    );
    let dot = if sel { p.on_accent } else { p.surface };
    for k in 0..3 {
        if pos >= 2 {
            let y = if centered {
                (bar.top + bar.bottom) / 2 - 6 + k * 4
            } else {
                bar.top + 3 + k * 4
            };
            fill(g.hdc, g.b, bar.left + 1, y, bar.right - 1, y + 2, dot);
        } else {
            let x = if centered {
                (bar.left + bar.right) / 2 - 6 + k * 4
            } else {
                bar.left + 3 + k * 4
            };
            fill(g.hdc, g.b, x, bar.top + 1, x + 2, bar.bottom - 1, dot);
        }
    }
}

unsafe fn draw_items(g: &mut Gfx, section: usize, a: &Rect, compact: bool) {
    let p = pal();
    let it = items(section, a, compact);
    match section {
        0 => {
            for &(id, r) in &it.list[..it.len] {
                if id < 100 {
                    pill(
                        g,
                        &r,
                        CATEGORIES[id as usize],
                        BG_CATEGORY == id as usize,
                        HOVER == id,
                    );
                    continue;
                }
                let i = (id - 100) as usize;
                let sel = WALLPAPER == i;
                if sel || HOVER == id {
                    let k = if sel { 3 } else { 2 };
                    round_rect(
                        g.hdc,
                        g.b,
                        r.left - k,
                        r.top - k,
                        r.right + k,
                        r.bottom + k,
                        6 + k,
                        if sel { p.accent } else { p.text3 },
                    );
                }
                let slot = &mut *core::ptr::addr_of_mut!(THUMBS[i]);
                paint_wallpaper(g.hdc, &r, i, slot);
                if sel {
                    ellipse(
                        g.hdc,
                        g.b,
                        r.right - 24,
                        r.top + 6,
                        r.right - 6,
                        r.top + 24,
                        p.accent,
                    );
                    line(
                        g.hdc,
                        g.b,
                        r.right - 19,
                        r.top + 15,
                        r.right - 16,
                        r.top + 18,
                        2,
                        p.on_accent,
                    );
                    line(
                        g.hdc,
                        g.b,
                        r.right - 16,
                        r.top + 18,
                        r.right - 11,
                        r.top + 11,
                        2,
                        p.on_accent,
                    );
                }
                text_fit(
                    g.hdc,
                    g.f.small,
                    11,
                    if sel { p.text } else { p.text2 },
                    r.left + 2,
                    r.bottom + 6,
                    WALLPAPERS[i].name,
                    r.right - r.left,
                );
            }
        }
        1 => {
            text(g.hdc, g.f.semibold, p.text, a.left, a.top + 8, b"Mode");
            for &(id, r) in &it.list[..it.len] {
                match id {
                    63 | 64 => pill(
                        g,
                        &r,
                        if id == 63 { b"Sombre" } else { b"Clair" },
                        DARK == (id == 63),
                        HOVER == id,
                    ),
                    60..=62 => {
                        if HOVER == id {
                            round_rect(
                                g.hdc,
                                g.b,
                                r.left - 6,
                                r.top,
                                r.right,
                                r.bottom,
                                6,
                                p.hover,
                            );
                        }
                        let (label, on): (&[u8], bool) = match id {
                            60 => (
                                b"Couleur automatique d'apr\xE8s le fond d'\xE9\x63ran",
                                ACCENT_INDEX < 0,
                            ),
                            61 => (
                                b"Accent sur D\xE9marrer et la barre des t\xE2\x63hes",
                                ACCENT_SURFACES,
                            ),
                            _ => (b"Accent sur les barres de titre et bordures", ACCENT_TITLES),
                        };
                        text_fit(
                            g.hdc,
                            g.f.body,
                            13,
                            p.text,
                            r.left,
                            r.top + 10,
                            label,
                            r.right - r.left - 60,
                        );
                        switch(g, r.right - 48, r.top + 8, on);
                    }
                    _ => {
                        let i = id as usize;
                        let sel =
                            ACCENT_INDEX == id || (ACCENT_INDEX < 0 && ACCENTS[i] == accent_base());
                        if sel {
                            round_rect(
                                g.hdc,
                                g.b,
                                r.left - 3,
                                r.top - 3,
                                r.right + 3,
                                r.bottom + 3,
                                6,
                                p.text,
                            );
                            round_rect(
                                g.hdc,
                                g.b,
                                r.left - 1,
                                r.top - 1,
                                r.right + 1,
                                r.bottom + 1,
                                5,
                                p.window,
                            );
                        } else if HOVER == id {
                            round_rect(
                                g.hdc,
                                g.b,
                                r.left - 2,
                                r.top - 2,
                                r.right + 2,
                                r.bottom + 2,
                                5,
                                p.text3,
                            );
                        }
                        round_rect(g.hdc, g.b, r.left, r.top, r.right, r.bottom, 4, ACCENTS[i]);
                        if sel {
                            let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
                            line(
                                g.hdc,
                                g.b,
                                cx - 6,
                                cy,
                                cx - 2,
                                cy + 4,
                                2,
                                rgb(0xFF, 0xFF, 0xFF),
                            );
                            line(
                                g.hdc,
                                g.b,
                                cx - 2,
                                cy + 4,
                                cx + 6,
                                cy - 4,
                                2,
                                rgb(0xFF, 0xFF, 0xFF),
                            );
                        }
                    }
                }
            }
        }
        2 => {
            let first = it.list[0].1;
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                a.left,
                first.top - 22,
                b"Position",
            );
            let size_top = it.list[4].1.top;
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                a.left,
                size_top - 22,
                b"Taille",
            );
            let align_top = it.list[7].1.top;
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                a.left,
                align_top - 22,
                b"Alignement des ic\xF4nes",
            );
            for &(id, r) in &it.list[..it.len] {
                let (sel, label, pos, thick, centered): (bool, &[u8], u8, i32, bool) = match id {
                    10..=13 => {
                        let k = (id - 10) as u8;
                        (
                            BAR_POS == k,
                            BAR_POSITIONS[k as usize],
                            k,
                            5,
                            !crate::desktop::ALIGN_LEFT,
                        )
                    }
                    20 | 21 => (
                        crate::desktop::ALIGN_LEFT == (id == 20),
                        if id == 20 {
                            if bar_vertical() {
                                b"En haut"
                            } else {
                                b"\xC0 gauche"
                            }
                        } else {
                            b"Centr\xE9"
                        },
                        BAR_POS,
                        5,
                        id == 21,
                    ),
                    _ => (
                        BAR_SIZE == id as u8,
                        BAR_SIZES[id as usize],
                        BAR_POS,
                        3 + id * 2,
                        !crate::desktop::ALIGN_LEFT,
                    ),
                };
                card(g, &r, sel, HOVER == id);
                screen_glyph(
                    g,
                    (r.left + r.right) / 2,
                    r.top + 10,
                    pos,
                    thick,
                    centered,
                    sel,
                );
                text_centered(
                    g.hdc,
                    g.f.body,
                    13,
                    p.text,
                    (r.left + r.right) / 2,
                    r.bottom - 26,
                    label,
                );
            }
        }
        3 => {
            text(
                g.hdc,
                g.f.semibold,
                p.text,
                a.left,
                a.top + 4,
                b"Ic\xF4nes du bureau",
            );
            for &(id, r) in &it.list[..it.len] {
                let i = id as usize;
                let on = crate::desktop::desktop_icon_visible(i);
                card(g, &r, false, HOVER == id);
                let (name, gl) = crate::desktop::desktop_icon_info(i);
                set_glyph_bg(Some(if HOVER == id { p.hover } else { p.surface }));
                glyph(g.hdc, g.b, g.f, gl, r.left + 12, r.top + 10, 24);
                set_glyph_bg(None);
                text(g.hdc, g.f.body, p.text, r.left + 48, r.top + 14, name);
                let bx = r.right - 34;
                let by = r.top + 12;
                if on {
                    round_rect(g.hdc, g.b, bx, by, bx + 20, by + 20, 4, p.accent);
                    line(g.hdc, g.b, bx + 5, by + 10, bx + 8, by + 13, 2, p.on_accent);
                    line(g.hdc, g.b, bx + 8, by + 13, bx + 15, by + 6, 2, p.on_accent);
                } else {
                    round_rect(g.hdc, g.b, bx, by, bx + 20, by + 20, 4, p.text2);
                    round_rect(g.hdc, g.b, bx + 1, by + 1, bx + 19, by + 19, 3, p.surface);
                }
            }
        }
        _ => {
            for &(id, r) in &it.list[..it.len] {
                if id >= 10 {
                    let i = (id - 10) as usize;
                    if i == 0 {
                        continue;
                    }
                    let hover = HOVER == id;
                    round_rect(
                        g.hdc,
                        g.b,
                        r.left,
                        r.top,
                        r.right,
                        r.bottom,
                        5,
                        if hover { p.accent } else { p.border },
                    );
                    round_rect(
                        g.hdc,
                        g.b,
                        r.left + 1,
                        r.top + 1,
                        r.right - 1,
                        r.bottom - 1,
                        4,
                        if hover { p.selected } else { p.surface2 },
                    );
                    let (cx, cy) = (r.left + 18, (r.top + r.bottom) / 2);
                    for k in 0..8 {
                        fill(
                            g.hdc,
                            g.b,
                            cx - 4 + k,
                            cy - 6 + k * 3 / 4,
                            cx - 3 + k,
                            cy + 7 - k * 3 / 4,
                            p.text,
                        );
                    }
                    text(
                        g.hdc,
                        g.f.body,
                        p.text,
                        r.left + 32,
                        r.top + 8,
                        b"\xC9\x63outer",
                    );
                    continue;
                }
                let i = id as usize;
                let sel = SOUND == i;
                card(g, &r, sel, HOVER == id);
                let (cx, cy) = (r.left + 22, (r.top + r.bottom) / 2);
                ellipse(
                    g.hdc,
                    g.b,
                    cx - 9,
                    cy - 9,
                    cx + 9,
                    cy + 9,
                    if sel { p.accent } else { p.text2 },
                );
                ellipse(g.hdc, g.b, cx - 7, cy - 7, cx + 7, cy + 7, p.surface);
                if sel {
                    ellipse(g.hdc, g.b, cx - 4, cy - 4, cx + 4, cy + 4, p.accent);
                }
                text(
                    g.hdc,
                    g.f.semibold,
                    p.text,
                    r.left + 44,
                    r.top + 7,
                    SOUNDS[i].0,
                );
                text_fit(
                    g.hdc,
                    g.f.small,
                    11,
                    p.text2,
                    r.left + 44,
                    r.top + 26,
                    SOUNDS[i].2,
                    r.right - r.left - 180,
                );
            }
        }
    }
}

/// Aperçu : écran miniature avec le fond, la barre et une fenêtre.
unsafe fn draw_mock(g: &mut Gfx, m: &Rect) {
    let p = pal();
    round_rect(
        g.hdc,
        g.b,
        m.left - 7,
        m.top - 7,
        m.right + 7,
        m.bottom + 7,
        10,
        rgb(0x0E, 0x0F, 0x12),
    );
    paint_wallpaper(g.hdc, m, WALLPAPER, &mut *core::ptr::addr_of_mut!(PREVIEW));
    let (w, h) = (m.right - m.left, m.bottom - m.top);
    let t = [7, 9, 11][BAR_SIZE.min(2) as usize];
    let bar = match BAR_POS {
        1 => rect(m.left, m.top, m.right, m.top + t),
        2 => rect(m.left, m.top, m.left + t, m.bottom),
        3 => rect(m.right - t, m.top, m.right, m.bottom),
        _ => rect(m.left, m.bottom - t, m.right, m.bottom),
    };
    let desk = match BAR_POS {
        1 => rect(m.left, bar.bottom, m.right, m.bottom),
        2 => rect(bar.right, m.top, m.right, m.bottom),
        3 => rect(m.left, m.top, bar.left, m.bottom),
        _ => rect(m.left, m.top, m.right, bar.top),
    };
    let mut y = desk.top + 6;
    for i in 0..crate::desktop::DESKTOP_ICON_COUNT {
        if crate::desktop::desktop_icon_visible(i) && y + 10 < desk.bottom {
            round_rect(
                g.hdc,
                g.b,
                desk.left + 6,
                y,
                desk.left + 14,
                y + 8,
                2,
                rgb(0xE8, 0xEC, 0xF2),
            );
            y += 13;
        }
    }
    let win = rect(
        desk.left + (desk.right - desk.left) * 30 / 100,
        desk.top + (desk.bottom - desk.top) * 18 / 100,
        desk.left + (desk.right - desk.left) * 82 / 100,
        desk.top + (desk.bottom - desk.top) * 80 / 100,
    );
    round_rect(
        g.hdc,
        g.b,
        win.left,
        win.top,
        win.right,
        win.bottom,
        4,
        if ACCENT_TITLES { p.accent } else { p.border },
    );
    round_rect(
        g.hdc,
        g.b,
        win.left + 1,
        win.top + 1,
        win.right - 1,
        win.bottom - 1,
        3,
        p.window,
    );
    let title = if ACCENT_TITLES {
        mix(p.mica, p.accent, 3, 10)
    } else {
        p.mica
    };
    fill(
        g.hdc,
        g.b,
        win.left + 1,
        win.top + 3,
        win.right - 1,
        win.top + 10,
        title,
    );
    fill(
        g.hdc,
        g.b,
        win.left + 8,
        win.top + 18,
        win.left + 8 + (win.right - win.left) / 3,
        win.top + 22,
        p.accent,
    );
    for k in 0..3 {
        fill(
            g.hdc,
            g.b,
            win.left + 8,
            win.top + 30 + k * 8,
            win.right - 12 - k * 14,
            win.top + 33 + k * 8,
            p.text3,
        );
    }
    fill(
        g.hdc,
        g.b,
        bar.left,
        bar.top,
        bar.right,
        bar.bottom,
        bar_bg(),
    );
    let n = 5;
    let b = t - 3;
    let vertical = BAR_POS >= 2;
    let span = n * (b + 3);
    let along = if vertical { h } else { w };
    let start = if crate::desktop::ALIGN_LEFT {
        4
    } else {
        (along - span) / 2
    };
    for k in 0..n {
        let o = start + k * (b + 3);
        let c = if k == 0 {
            p.accent
        } else {
            rgb(0xC8, 0xCD, 0xD4)
        };
        if vertical {
            fill(
                g.hdc,
                g.b,
                bar.left + 1,
                bar.top + o,
                bar.left + 1 + b,
                bar.top + o + b,
                c,
            );
        } else {
            fill(
                g.hdc,
                g.b,
                bar.left + o,
                bar.top + 1,
                bar.left + o + b,
                bar.top + 1 + b,
                c,
            );
        }
    }
    let _ = w;
}

/// Colonne d'aperçu à droite des réglages (page Paramètres assez large).
fn has_mock(section: usize, r: &Rect, compact: bool) -> bool {
    !compact && section != 4 && r.right - r.left >= 700
}
unsafe fn sec_area(section: usize, r: &Rect, compact: bool) -> Rect {
    if has_mock(section, r, compact) {
        rect(r.left, r.top, r.right - 268, r.bottom)
    } else {
        *r
    }
}

unsafe fn draw_section(g: &mut Gfx, section: usize, r: &Rect, compact: bool) {
    let p = pal();
    if has_mock(section, r, compact) {
        let m = rect(r.right - 232, r.top + 7, r.right - 8, r.top + 7 + 126);
        draw_mock(g, &m);
        let tx = m.left;
        let (title, detail): (&[u8], &[u8]) = match section {
            0 => (
                WALLPAPERS[WALLPAPER].name,
                CATEGORIES[WALLPAPERS[WALLPAPER].category],
            ),
            1 => (
                if ACCENT_INDEX < 0 {
                    b"Accent automatique"
                } else {
                    b"Accent personnalis\xE9"
                },
                if DARK { b"Mode sombre" } else { b"Mode clair" },
            ),
            2 => (
                BAR_POSITIONS[BAR_POS as usize],
                BAR_SIZES[BAR_SIZE as usize],
            ),
            _ => (b"Bureau", b"Ic\xF4nes syst\xE8me et raccourcis"),
        };
        let ty = m.bottom + 22;
        text(g.hdc, g.f.title, p.text, tx, ty, title);
        text(g.hdc, g.f.body, p.text2, tx, ty + 26, detail);
        if section == 1 {
            round_rect(g.hdc, g.b, tx, ty + 54, tx + 64, ty + 78, 6, ACCENT_BASE);
            round_rect(g.hdc, g.b, tx + 70, ty + 54, tx + 134, ty + 78, 6, ACCENT);
        }
    }
    let a = sec_area(section, r, compact);
    draw_items(g, section, &a, compact);
}

/// Applique un choix ; renvoie vrai si l'écran doit être repeint.
unsafe fn activate(section: usize, id: i32) -> bool {
    if id < 0 {
        return false;
    }
    match section {
        0 => {
            if id < 100 {
                BG_CATEGORY = id as usize;
                return true;
            }
            WALLPAPER = (id - 100) as usize;
            if ACCENT_INDEX < 0 {
                apply_accent();
            }
        }
        1 => match id {
            60 => {
                ACCENT_INDEX = if ACCENT_INDEX < 0 {
                    ACCENTS
                        .iter()
                        .position(|&c| c == accent_base())
                        .map_or(16, |i| i as i32)
                } else {
                    -1
                };
                apply_accent();
            }
            61 => ACCENT_SURFACES = !ACCENT_SURFACES,
            62 => ACCENT_TITLES = !ACCENT_TITLES,
            63 | 64 => {
                let dark = id == 63;
                set_theme(dark);
                crate::settings::control_quiet(if dark { c"theme-dark" } else { c"theme-light" });
            }
            _ => {
                ACCENT_INDEX = id;
                apply_accent();
            }
        },
        2 => match id {
            10..=13 => BAR_POS = (id - 10) as u8,
            20 | 21 => {
                crate::desktop::ALIGN_LEFT = id == 20;
                crate::settings::control_quiet(if id == 20 {
                    c"align-left"
                } else {
                    c"align-center"
                });
            }
            _ => BAR_SIZE = id as u8,
        },
        3 => crate::desktop::toggle_desktop_icon(id as usize),
        _ => {
            if id >= 10 {
                play_sound((id - 10) as usize);
                return true;
            }
            SOUND = id as usize;
            play_sound(SOUND);
        }
    }
    save();
    true
}

// ---------- Page Paramètres > Personnalisation ----------

fn tab_rect(c: &Rect, i: usize) -> Rect {
    let mut x = c.left;
    for k in 0..i {
        x += pill_w(SECTIONS[k]) + 6;
    }
    rect(x, c.top + 46, x + pill_w(SECTIONS[i]), c.top + 80)
}
fn settings_area(c: &Rect) -> Rect {
    rect(c.left, c.top + 98, c.right, c.bottom - 28)
}

pub unsafe fn open_settings(section: usize) {
    SECTION = section.min(SECTIONS.len() - 1);
    crate::settings::open_page(crate::settings::PAGE_PERSONALIZE);
}

pub unsafe fn draw_settings(g: &mut Gfx, c: &Rect) {
    for i in 0..SECTIONS.len() {
        let r = tab_rect(c, i);
        pill(g, &r, SECTIONS[i], SECTION == i, HOVER == 900 + i as i32);
    }
    draw_section(g, SECTION, &settings_area(c), false);
}

unsafe fn settings_hit(c: &Rect, x: i32, y: i32) -> i32 {
    for i in 0..SECTIONS.len() {
        if inside(&tab_rect(c, i), x, y) {
            return 900 + i as i32;
        }
    }
    items(SECTION, &sec_area(SECTION, &settings_area(c), false), false).hit(x, y)
}

pub unsafe fn settings_move(c: &Rect, x: i32, y: i32) -> bool {
    let h = settings_hit(c, x, y);
    let changed = h != HOVER;
    HOVER = h;
    changed
}

pub unsafe fn settings_click(c: &Rect, x: i32, y: i32) -> bool {
    let h = settings_hit(c, x, y);
    if h >= 900 {
        SECTION = (h - 900) as usize;
        HOVER = -1;
        return true;
    }
    activate(SECTION, h)
}

// ---------- Assistant du premier démarrage ----------

const STEPS: [&[u8]; 7] = [
    b"Bienvenue",
    b"Arri\xE8re-plan",
    b"Couleurs",
    b"Barre des t\xE2\x63hes",
    b"Bureau",
    b"Son de d\xE9marrage",
    b"Termin\xE9",
];

pub fn oobe_active() -> bool {
    unsafe { OOBE_ACTIVE }
}

/// Relance l'assistant (depuis Paramètres).
pub unsafe fn start_oobe() {
    OOBE_ACTIVE = true;
    OOBE_STEP = 0;
    HOVER = -1;
}

unsafe fn finish_oobe() {
    OOBE_ACTIVE = false;
    HOVER = -1;
    save();
    crate::desktop::toast(b"Personnalisation enregistr\xE9\x65. Modifiable dans Param\xE8tres.");
}

fn oobe_card(desk: &Rect) -> Rect {
    let w = 880.min(desk.right - desk.left - 32);
    let h = 600.min(desk.bottom - desk.top - 32);
    let x = desk.left + (desk.right - desk.left - w) / 2;
    let y = desk.top + (desk.bottom - desk.top - h) / 2;
    rect(x, y, x + w, y + h)
}
fn oobe_side(c: &Rect) -> i32 {
    if c.right - c.left >= 760 {
        232
    } else {
        0
    }
}
fn oobe_content(c: &Rect) -> Rect {
    rect(
        c.left + oobe_side(c) + 36,
        c.top + 104,
        c.right - 36,
        c.bottom - 76,
    )
}
fn oobe_buttons(c: &Rect) -> [Rect; 3] {
    let y = c.bottom - 60;
    [
        rect(
            c.right - 36 - 120 - 10 - 112,
            y,
            c.right - 36 - 120 - 10,
            y + 36,
        ),
        rect(c.right - 36 - 120, y, c.right - 36, y + 36),
        rect(
            c.left + oobe_side(c) + 30,
            y,
            c.left + oobe_side(c) + 30 + 150,
            y + 36,
        ),
    ]
}

unsafe fn oobe_hit(desk: &Rect, x: i32, y: i32) -> i32 {
    let c = oobe_card(desk);
    for (i, r) in oobe_buttons(&c).iter().enumerate() {
        if inside(r, x, y) && !(i == 0 && OOBE_STEP == 0) && !(i == 2 && OOBE_STEP == 6) {
            return 1000 + i as i32;
        }
    }
    if (1..=5).contains(&OOBE_STEP) {
        return items(OOBE_STEP - 1, &oobe_content(&c), true).hit(x, y);
    }
    -1
}

pub unsafe fn draw_oobe(g: &mut Gfx, desk: &Rect) {
    let p = pal();
    let c = oobe_card(desk);
    round_rect(
        g.hdc,
        g.b,
        c.left - 1,
        c.top - 1,
        c.right + 1,
        c.bottom + 1,
        12,
        p.shadow,
    );
    panel(
        g.hdc, g.b, c.left, c.top, c.right, c.bottom, 10, p.window, p.border,
    );
    let side = oobe_side(&c);
    if side > 0 {
        round_rect(
            g.hdc,
            g.b,
            c.left + 1,
            c.top + 1,
            c.left + side,
            c.bottom - 1,
            9,
            p.mica,
        );
        fill(
            g.hdc,
            g.b,
            c.left + side - 10,
            c.top + 1,
            c.left + side,
            c.bottom - 1,
            p.mica,
        );
        fill(
            g.hdc,
            g.b,
            c.left + side,
            c.top + 1,
            c.left + side + 1,
            c.bottom - 1,
            p.border,
        );
        let (lx, ly) = (c.left + 28, c.top + 32);
        for (k, col) in [hex(0xF25022), hex(0x7FBA00), hex(0x00A4EF), hex(0xFFB900)]
            .iter()
            .enumerate()
        {
            let (dx, dy) = ((k % 2) as i32 * 13, (k / 2) as i32 * 13);
            fill(
                g.hdc,
                g.b,
                lx + dx,
                ly + dy,
                lx + dx + 11,
                ly + dy + 11,
                *col,
            );
        }
        text(g.hdc, g.f.title, p.text, lx + 36, ly + 4, b"Winlux");
        for (i, name) in STEPS.iter().enumerate() {
            let y = c.top + 104 + i as i32 * 44;
            let done = i < OOBE_STEP;
            let cur = i == OOBE_STEP;
            if cur {
                round_rect(
                    g.hdc,
                    g.b,
                    c.left + 14,
                    y - 8,
                    c.left + side - 14,
                    y + 28,
                    6,
                    p.selected,
                );
                fill(
                    g.hdc,
                    g.b,
                    c.left + 14,
                    y + 2,
                    c.left + 17,
                    y + 18,
                    p.accent,
                );
            }
            let (cx, cy) = (c.left + 40, y + 10);
            if done {
                ellipse(g.hdc, g.b, cx - 9, cy - 9, cx + 9, cy + 9, p.accent);
                line(g.hdc, g.b, cx - 4, cy, cx - 1, cy + 3, 2, p.on_accent);
                line(g.hdc, g.b, cx - 1, cy + 3, cx + 5, cy - 3, 2, p.on_accent);
            } else {
                ellipse(
                    g.hdc,
                    g.b,
                    cx - 9,
                    cy - 9,
                    cx + 9,
                    cy + 9,
                    if cur { p.accent } else { p.text3 },
                );
                ellipse(
                    g.hdc,
                    g.b,
                    cx - 7,
                    cy - 7,
                    cx + 7,
                    cy + 7,
                    if cur { p.selected } else { p.mica },
                );
            }
            text(
                g.hdc,
                if cur { g.f.semibold } else { g.f.body },
                if cur || done { p.text } else { p.text2 },
                c.left + 60,
                y + 2,
                name,
            );
        }
    }
    let left = c.left + side + 36;
    let (title, sub): (&[u8], &[u8]) = match OOBE_STEP {
        0 => (
            b"Bienvenue dans Winlux",
            b"Quelques r\xE9glages pour que ce bureau vous ressemble.",
        ),
        6 => (
            b"Tout est pr\xEAt",
            b"Vos choix sont appliqu\xE9s et enregistr\xE9s.",
        ),
        s => (SECTIONS[s - 1], label_of(s - 1)),
    };
    text(g.hdc, g.f.display, p.text, left, c.top + 32, title);
    text_fit(
        g.hdc,
        g.f.body,
        13,
        p.text2,
        left,
        c.top + 72,
        sub,
        c.right - 36 - left,
    );
    let a = oobe_content(&c);
    match OOBE_STEP {
        0 => {
            let user = crate::login::current_user_name();
            let mut y = a.top + 6;
            if !user.is_empty() {
                let mut hello = [0u8; 48];
                let n = user.len().min(hello.len() - 10);
                hello[..9].copy_from_slice(b"Bonjour, ");
                hello[9..9 + n].copy_from_slice(&user[..n]);
                hello[9 + n] = b'!';
                text(g.hdc, g.f.title, p.text, a.left, y, &hello[..10 + n]);
                y += 40;
            }
            for line in [
                b"Choisissez un fond d'\xE9\x63ran, une couleur d'accentuation,".as_slice(),
                b"l'emplacement de la barre des t\xE2\x63hes, les ic\xF4nes du bureau",
                b"et le son jou\xE9 \xE0 l'ouverture de session.",
                b"",
                b"Chaque choix s'applique tout de suite : le bureau derri\xE8re",
                b"cette fen\xEAtre vous en montre le r\xE9sultat.",
                b"",
                b"Tout reste modifiable dans Param\xE8tres > Personnalisation.",
            ] {
                text(g.hdc, g.f.body, p.text2, a.left, y, line);
                y += 24;
            }
        }
        6 => {
            let mut y = a.top + 4;
            let rows: [(&[u8], &[u8]); 5] = [
                (b"Fond d'\xE9\x63ran", WALLPAPERS[WALLPAPER].name),
                (
                    b"Accent",
                    if ACCENT_INDEX < 0 {
                        b"Automatique"
                    } else {
                        b"Personnalis\xE9"
                    },
                ),
                (b"Barre des t\xE2\x63hes", BAR_POSITIONS[BAR_POS as usize]),
                (b"Taille de la barre", BAR_SIZES[BAR_SIZE as usize]),
                (b"Son de d\xE9marrage", SOUNDS[SOUND].0),
            ];
            for (k, v) in rows {
                round_rect(g.hdc, g.b, a.left, y, a.right, y + 44, 6, p.surface);
                text(g.hdc, g.f.body, p.text2, a.left + 16, y + 14, k);
                text(g.hdc, g.f.semibold, p.text, a.left + 230, y + 14, v);
                if k == b"Accent" {
                    round_rect(
                        g.hdc,
                        g.b,
                        a.right - 46,
                        y + 12,
                        a.right - 16,
                        y + 32,
                        5,
                        ACCENT_BASE,
                    );
                }
                y += 52;
            }
        }
        s => draw_items(g, s - 1, &a, true),
    }
    let [back, next, skip] = oobe_buttons(&c);
    if OOBE_STEP > 0 {
        round_rect(
            g.hdc,
            g.b,
            back.left,
            back.top,
            back.right,
            back.bottom,
            5,
            p.border,
        );
        round_rect(
            g.hdc,
            g.b,
            back.left + 1,
            back.top + 1,
            back.right - 1,
            back.bottom - 1,
            4,
            if HOVER == 1000 { p.hover } else { p.surface2 },
        );
        text_centered(
            g.hdc,
            g.f.body,
            13,
            p.text,
            (back.left + back.right) / 2,
            back.top + 10,
            b"Retour",
        );
    }
    round_rect(
        g.hdc,
        g.b,
        next.left,
        next.top,
        next.right,
        next.bottom,
        5,
        if HOVER == 1001 {
            mix(p.accent, rgb(0xFF, 0xFF, 0xFF), 2, 10)
        } else {
            p.accent
        },
    );
    text_centered(
        g.hdc,
        g.f.semibold,
        13,
        p.on_accent,
        (next.left + next.right) / 2,
        next.top + 10,
        match OOBE_STEP {
            0 => b"Commencer",
            6 => b"Terminer",
            _ => b"Suivant",
        },
    );
    if OOBE_STEP < 6 {
        text(
            g.hdc,
            g.f.body,
            if HOVER == 1002 { p.accent } else { p.text2 },
            skip.left + 6,
            skip.top + 10,
            b"Passer l'assistant",
        );
    }
}

pub unsafe fn oobe_move(desk: &Rect, x: i32, y: i32) -> bool {
    let h = oobe_hit(desk, x, y);
    let changed = h != HOVER;
    HOVER = h;
    changed
}

pub unsafe fn oobe_click(desk: &Rect, x: i32, y: i32) -> bool {
    match oobe_hit(desk, x, y) {
        1000 => OOBE_STEP = OOBE_STEP.saturating_sub(1),
        1001 => {
            if OOBE_STEP >= 6 {
                finish_oobe();
            } else {
                OOBE_STEP += 1;
            }
        }
        1002 => finish_oobe(),
        id if (1..=5).contains(&OOBE_STEP) => return activate(OOBE_STEP - 1, id),
        _ => return false,
    }
    HOVER = -1;
    true
}

pub unsafe fn oobe_key(vk: u32) -> bool {
    match vk {
        0x0D | 0x27 => {
            if OOBE_STEP >= 6 {
                finish_oobe();
            } else {
                OOBE_STEP += 1;
            }
        }
        0x08 | 0x25 => OOBE_STEP = OOBE_STEP.saturating_sub(1),
        0x1B => finish_oobe(),
        _ => return false,
    }
    HOVER = -1;
    true
}

pub unsafe fn regression_checks() {
    check!(
        sin1k(900) == 1000 && sin1k(2700) == -1000 && sin1k(0) == 0,
        "Sinus entier exact aux quadrants"
    );
    let size = synth(1);
    check!(
        size > 44 && &WAV[..4] == b"RIFF" && &WAV[8..12] == b"WAVE",
        "Le son de demarrage est un WAV valide"
    );
    check!(synth(0) == 0, "Le son Aucun ne produit rien");
    for id in 0..WALLPAPERS.len() {
        let a = shade(id, 10, 10, 320, 180);
        let b = shade(id, 160, 150, 320, 180);
        check!(a != b || id == 0, "Chaque fond d'ecran a du relief");
    }
    let saved = (BAR_POS, BAR_SIZE, ACCENT_INDEX);
    ACCENT_INDEX = 6;
    check!(accent_base() == ACCENTS[6], "Accent choisi dans la palette");
    ACCENT_INDEX = -1;
    check!(
        accent_base() == WALLPAPERS[WALLPAPER].accent,
        "Accent automatique selon le fond"
    );
    let c = rect(0, 0, 760, 560);
    for s in 0..SECTIONS.len() {
        let it = items(s, &sec_area(s, &c, false), false);
        check!(
            it.len > 0 && it.list[..it.len].iter().all(|(_, r)| r.bottom <= c.bottom),
            "Les choix tiennent dans la page"
        );
    }
    (BAR_POS, BAR_SIZE, ACCENT_INDEX) = saved;
}
