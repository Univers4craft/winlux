//! Widget météo pour la barre des tâches Winlux : icône et température dans
//! la zone de notification, panneau détaillé au clic avec prévisions.
//! Les données sont lues depuis Z:\run\winlux\weather.json (écrit par un
//! script shell qui interroge wttr.in).

use crate::gfx::*;
use winlux_testexe::*;

// Largeur du widget dans la barre des tâches
pub const TRAY_WIDTH: i32 = 64;

// Panneau déroulant
pub static mut PANEL_OPEN: bool = false;
static mut HOVER_ROW: i32 = -1;

// Données météo actuelles
static mut TEMP: i32 = 0;
static mut FEELS_LIKE: i32 = 0;
static mut HUMIDITY: u32 = 0;
static mut WIND_KMH: u32 = 0;
static mut CONDITION: WeatherCondition = WeatherCondition::Unknown;
static mut CITY: [u8; 32] = [0; 32];
static mut CITY_LEN: usize = 0;
static mut DATA_AVAILABLE: bool = false;
static mut LAST_POLL: u32 = 0;

// Prévisions (3 jours)
const MAX_FORECAST: usize = 3;
static mut FORECAST: [DayForecast; MAX_FORECAST] = [DayForecast {
    day: [0; 4],
    day_len: 0,
    temp_min: 0,
    temp_max: 0,
    condition: WeatherCondition::Unknown,
}; MAX_FORECAST];
static mut FORECAST_COUNT: usize = 0;

#[derive(Clone, Copy, PartialEq)]
enum WeatherCondition {
    Sunny,
    PartlyCloudy,
    Cloudy,
    Rain,
    Snow,
    Storm,
    Fog,
    Unknown,
}

#[derive(Clone, Copy)]
struct DayForecast {
    day: [u8; 4],
    day_len: usize,
    temp_min: i32,
    temp_max: i32,
    condition: WeatherCondition,
}

// Buffer de lecture
const CAPACITY: usize = 2048;
static mut BUF: [u8; CAPACITY] = [0; CAPACITY];
static mut BUF_LEN: usize = 0;

unsafe fn read_weather_file() {
    let file = CreateFileA(
        c"Z:\\run\\winlux\\weather.json".as_ptr(),
        0x8000_0000,
        7,
        core::ptr::null_mut(),
        3,
        0,
        core::ptr::null_mut(),
    );
    if file as isize == -1 || file.is_null() {
        return;
    }
    let mut count = 0u32;
    ReadFile(
        file,
        BUF.as_mut_ptr(),
        (CAPACITY - 1) as u32,
        &mut count,
        core::ptr::null_mut(),
    );
    CloseHandle(file);
    BUF_LEN = count as usize;
    if BUF_LEN == 0 {
        return;
    }
    BUF[BUF_LEN] = 0;
    parse_weather();
}

unsafe fn find_value(key: &[u8]) -> Option<&'static [u8]> {
    let buf = &BUF[..BUF_LEN];
    let mut i = 0;
    while i + key.len() < buf.len() {
        if buf[i..].starts_with(key) {
            let start = i + key.len();
            // Chercher le séparateur '=' ou ':'
            if start < buf.len() && (buf[start] == b'=' || buf[start] == b':') {
                let val_start = start + 1;
                let mut end = val_start;
                while end < buf.len() && buf[end] != b'\n' && buf[end] != b'\r' {
                    end += 1;
                }
                return Some(&BUF[val_start..end]);
            }
        }
        i += 1;
    }
    None
}

unsafe fn parse_number(s: &[u8]) -> i32 {
    let s = trim(s);
    if s.is_empty() {
        return 0;
    }
    let negative = s[0] == b'-';
    let start = if negative || s[0] == b'+' { 1 } else { 0 };
    let mut val: i32 = 0;
    for &c in &s[start..] {
        if c >= b'0' && c <= b'9' {
            val = val * 10 + (c - b'0') as i32;
        }
    }
    if negative {
        -val
    } else {
        val
    }
}

fn trim(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && (s[start] == b' ' || s[start] == b'\t') {
        start += 1;
    }
    while end > start && (s[end - 1] == b' ' || s[end - 1] == b'\t') {
        end -= 1;
    }
    &s[start..end]
}

unsafe fn parse_condition(s: &[u8]) -> WeatherCondition {
    let s = trim(s);
    if contains(s, b"storm") || contains(s, b"thunder") || contains(s, b"orage") {
        WeatherCondition::Storm
    } else if contains(s, b"snow") || contains(s, b"neige") || contains(s, b"sleet") {
        WeatherCondition::Snow
    } else if contains(s, b"rain")
        || contains(s, b"pluie")
        || contains(s, b"drizzle")
        || contains(s, b"shower")
    {
        WeatherCondition::Rain
    } else if contains(s, b"fog") || contains(s, b"mist") || contains(s, b"brouillard") {
        WeatherCondition::Fog
    } else if contains(s, b"cloud")
        || contains(s, b"overcast")
        || contains(s, b"nuag")
        || contains(s, b"couvert")
    {
        WeatherCondition::Cloudy
    } else if contains(s, b"partly") || contains(s, b"partiel") {
        WeatherCondition::PartlyCloudy
    } else if contains(s, b"sun")
        || contains(s, b"clear")
        || contains(s, b"soleil")
        || contains(s, b"d\xe9gag")
    {
        WeatherCondition::Sunny
    } else {
        WeatherCondition::Unknown
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    for i in 0..=(haystack.len() - needle.len()) {
        let lo = |c: u8| {
            if c >= b'A' && c <= b'Z' {
                c + 32
            } else {
                c
            }
        };
        if needle
            .iter()
            .zip(&haystack[i..])
            .all(|(&a, &b)| lo(a) == lo(b))
        {
            return true;
        }
    }
    false
}

unsafe fn parse_weather() {
    if let Some(v) = find_value(b"temp") {
        TEMP = parse_number(v);
        DATA_AVAILABLE = true;
    }
    if let Some(v) = find_value(b"feels_like") {
        FEELS_LIKE = parse_number(v);
    }
    if let Some(v) = find_value(b"humidity") {
        HUMIDITY = parse_number(v).max(0) as u32;
    }
    if let Some(v) = find_value(b"wind") {
        WIND_KMH = parse_number(v).max(0) as u32;
    }
    if let Some(v) = find_value(b"condition") {
        CONDITION = parse_condition(v);
    }
    if let Some(v) = find_value(b"city") {
        let v = trim(v);
        let n = v.len().min(CITY.len());
        CITY[..n].copy_from_slice(&v[..n]);
        CITY_LEN = n;
    }
    // Prévisions : forecast0_day, forecast0_min, forecast0_max, forecast0_cond
    FORECAST_COUNT = 0;
    for i in 0..MAX_FORECAST {
        let mut key_day = [0u8; 16];
        let kl = fmt_forecast_key(&mut key_day, i as u8, b"_day");
        if let Some(d) = find_value(&key_day[..kl]) {
            let d = trim(d);
            let n = d.len().min(4);
            FORECAST[i].day[..n].copy_from_slice(&d[..n]);
            FORECAST[i].day_len = n;
        }
        let mut key_min = [0u8; 16];
        let kl = fmt_forecast_key(&mut key_min, i as u8, b"_min");
        if let Some(v) = find_value(&key_min[..kl]) {
            FORECAST[i].temp_min = parse_number(v);
        }
        let mut key_max = [0u8; 16];
        let kl = fmt_forecast_key(&mut key_max, i as u8, b"_max");
        if let Some(v) = find_value(&key_max[..kl]) {
            FORECAST[i].temp_max = parse_number(v);
        }
        let mut key_cond = [0u8; 20];
        let kl = fmt_forecast_key(&mut key_cond, i as u8, b"_cond");
        if let Some(v) = find_value(&key_cond[..kl]) {
            FORECAST[i].condition = parse_condition(v);
        }
        FORECAST_COUNT += 1;
    }
}

fn fmt_forecast_key(buf: &mut [u8], idx: u8, suffix: &[u8]) -> usize {
    let prefix = b"forecast";
    let mut pos = 0;
    for &c in prefix {
        buf[pos] = c;
        pos += 1;
    }
    buf[pos] = b'0' + idx;
    pos += 1;
    for &c in suffix {
        buf[pos] = c;
        pos += 1;
    }
    pos
}

// Rafraîchir toutes les 30 secondes
pub unsafe fn poll() -> bool {
    let now = GetTickCount();
    if LAST_POLL != 0 && now.wrapping_sub(LAST_POLL) < 30_000 {
        return false;
    }
    LAST_POLL = now;
    let old_temp = TEMP;
    let old_cond = CONDITION as u8;
    read_weather_file();
    TEMP != old_temp || CONDITION as u8 != old_cond
}

// ---------- Dessin dans la barre des tâches ----------

pub unsafe fn draw_tray(hdc: Handle, b: &mut Brushes, f: &Fonts, x: i32, y: i32) {
    if !DATA_AVAILABLE {
        // Pas de données : afficher un tiret
        text(
            hdc,
            f.small,
            rgb(0x9A, 0xA3, 0xAE),
            x + 8,
            y + 2,
            b"--\xB0C",
        );
        return;
    }
    // Icône météo (16x16)
    draw_weather_icon(hdc, b, x + 2, y - 2, 18, CONDITION);
    // Température
    let mut temp_str = [0u8; 8];
    let tlen = format_temp(&mut temp_str, TEMP);
    text(
        hdc,
        f.small,
        rgb(0xF2, 0xF2, 0xF2),
        x + 22,
        y + 2,
        &temp_str[..tlen],
    );
}

unsafe fn draw_weather_icon(
    hdc: Handle,
    b: &mut Brushes,
    x: i32,
    y: i32,
    s: i32,
    cond: WeatherCondition,
) {
    match cond {
        WeatherCondition::Sunny => {
            // Soleil : cercle jaune + rayons
            let cx = x + s / 2;
            let cy = y + s / 2;
            let r = s / 3;
            ellipse(
                hdc,
                b,
                cx - r,
                cy - r,
                cx + r,
                cy + r,
                rgb(0xFF, 0xC8, 0x00),
            );
            let ray = rgb(0xFF, 0xD5, 0x40);
            let rl = s / 6;
            // Rayons cardinaux
            fill(hdc, b, cx - 1, cy - r - rl, cx + 1, cy - r - 1, ray);
            fill(hdc, b, cx - 1, cy + r + 1, cx + 1, cy + r + rl, ray);
            fill(hdc, b, cx - r - rl, cy - 1, cx - r - 1, cy + 1, ray);
            fill(hdc, b, cx + r + 1, cy - 1, cx + r + rl, cy + 1, ray);
        }
        WeatherCondition::PartlyCloudy => {
            // Soleil partiel + petit nuage
            let cx = x + s * 3 / 8;
            let cy = y + s * 3 / 8;
            let r = s / 4;
            ellipse(
                hdc,
                b,
                cx - r,
                cy - r,
                cx + r,
                cy + r,
                rgb(0xFF, 0xC8, 0x00),
            );
            draw_cloud(
                hdc,
                b,
                x + s / 4,
                y + s / 3,
                s * 2 / 3,
                rgb(0xCC, 0xD0, 0xD8),
            );
        }
        WeatherCondition::Cloudy => {
            draw_cloud(
                hdc,
                b,
                x + s / 8,
                y + s / 4,
                s * 3 / 4,
                rgb(0x9A, 0xA3, 0xAE),
            );
        }
        WeatherCondition::Rain => {
            draw_cloud(
                hdc,
                b,
                x + s / 8,
                y + s / 8,
                s * 3 / 4,
                rgb(0x70, 0x78, 0x88),
            );
            // Gouttes
            let drop = rgb(0x60, 0xA0, 0xF0);
            fill(
                hdc,
                b,
                x + s / 4,
                y + s * 3 / 4,
                x + s / 4 + 2,
                y + s - 1,
                drop,
            );
            fill(
                hdc,
                b,
                x + s / 2,
                y + s * 3 / 4 + 2,
                x + s / 2 + 2,
                y + s,
                drop,
            );
            fill(
                hdc,
                b,
                x + s * 3 / 4 - 2,
                y + s * 3 / 4,
                x + s * 3 / 4,
                y + s - 1,
                drop,
            );
        }
        WeatherCondition::Snow => {
            draw_cloud(
                hdc,
                b,
                x + s / 8,
                y + s / 8,
                s * 3 / 4,
                rgb(0x90, 0x98, 0xA8),
            );
            let flake = rgb(0xE0, 0xE8, 0xF0);
            fill(
                hdc,
                b,
                x + s / 4,
                y + s * 3 / 4,
                x + s / 4 + 2,
                y + s * 3 / 4 + 2,
                flake,
            );
            fill(
                hdc,
                b,
                x + s / 2,
                y + s * 4 / 5,
                x + s / 2 + 2,
                y + s * 4 / 5 + 2,
                flake,
            );
            fill(
                hdc,
                b,
                x + s * 3 / 4 - 2,
                y + s * 3 / 4,
                x + s * 3 / 4,
                y + s * 3 / 4 + 2,
                flake,
            );
        }
        WeatherCondition::Storm => {
            draw_cloud(
                hdc,
                b,
                x + s / 8,
                y + s / 8,
                s * 3 / 4,
                rgb(0x50, 0x58, 0x68),
            );
            // Éclair
            let bolt = rgb(0xFF, 0xD5, 0x40);
            fill(
                hdc,
                b,
                x + s / 2 - 1,
                y + s / 2,
                x + s / 2 + 2,
                y + s * 3 / 4,
                bolt,
            );
            fill(
                hdc,
                b,
                x + s / 2 - 3,
                y + s * 3 / 4 - 1,
                x + s / 2 + 1,
                y + s * 3 / 4 + 1,
                bolt,
            );
            fill(
                hdc,
                b,
                x + s / 2 - 2,
                y + s * 3 / 4,
                x + s / 2 + 1,
                y + s - 2,
                bolt,
            );
        }
        WeatherCondition::Fog => {
            let fog = rgb(0x9A, 0xA3, 0xAE);
            for i in 0..3 {
                let ly = y + s / 4 + i * s / 5;
                fill(hdc, b, x + 2, ly, x + s - 2, ly + 2, fog);
            }
        }
        WeatherCondition::Unknown => {
            draw_cloud(
                hdc,
                b,
                x + s / 8,
                y + s / 4,
                s * 3 / 4,
                rgb(0x60, 0x68, 0x78),
            );
        }
    }
}

unsafe fn draw_cloud(hdc: Handle, b: &mut Brushes, x: i32, y: i32, w: i32, color: u32) {
    let h = w * 2 / 3;
    round_rect(hdc, b, x, y + h / 3, x + w, y + h, 4, color);
    ellipse(hdc, b, x + w / 6, y, x + w / 2, y + h * 2 / 3, color);
    ellipse(
        hdc,
        b,
        x + w / 3,
        y - h / 6,
        x + w * 3 / 4,
        y + h / 2,
        color,
    );
}

fn format_temp(buf: &mut [u8], temp: i32) -> usize {
    let mut pos = 0;
    if temp < 0 {
        buf[pos] = b'-';
        pos += 1;
    }
    let abs = if temp < 0 { -temp } else { temp } as u32;
    if abs >= 10 {
        buf[pos] = b'0' + (abs / 10).min(9) as u8;
        pos += 1;
    }
    buf[pos] = b'0' + (abs % 10) as u8;
    pos += 1;
    buf[pos] = 0xB0; // °
    pos += 1;
    buf[pos] = b'C';
    pos += 1;
    pos
}

fn format_number(buf: &mut [u8], val: u32) -> usize {
    let mut pos = 0;
    if val >= 100 {
        buf[pos] = b'0' + (val / 100).min(9) as u8;
        pos += 1;
    }
    if val >= 10 {
        buf[pos] = b'0' + (val / 10 % 10) as u8;
        pos += 1;
    }
    buf[pos] = b'0' + (val % 10) as u8;
    pos += 1;
    pos
}

// ---------- Panneau détaillé ----------

const PANEL_W: i32 = 320;

pub unsafe fn draw_panel(hdc: Handle, b: &mut Brushes, f: &Fonts, bar_top: i32, screen_w: i32) {
    if !PANEL_OPEN {
        return;
    }
    let pw = PANEL_W;
    let ph = if FORECAST_COUNT > 0 {
        280 + FORECAST_COUNT as i32 * 52
    } else {
        260
    };
    let px = (screen_w - pw) / 2;
    let py = bar_top - 12 - ph;

    panel(
        hdc,
        b,
        px,
        py,
        px + pw,
        py + ph,
        8,
        rgb(0x24, 0x27, 0x2F),
        rgb(0x3A, 0x3F, 0x4A),
    );

    // En-tête : ville
    if CITY_LEN > 0 {
        text(
            hdc,
            f.semibold,
            rgb(0xF2, 0xF2, 0xF2),
            px + 16,
            py + 14,
            &CITY[..CITY_LEN],
        );
    } else {
        text(
            hdc,
            f.semibold,
            rgb(0xF2, 0xF2, 0xF2),
            px + 16,
            py + 14,
            tr4(&[b"M\xE9t\xE9o", b"Weather", b"El tiempo", b"Wetter"]),
        );
    }

    if !DATA_AVAILABLE {
        text(
            hdc,
            f.body,
            rgb(0x9A, 0xA3, 0xAE),
            px + 16,
            py + 50,
            tr4(&[
                b"Aucune donn\xE9e disponible",
                b"No data available",
                b"No hay datos disponibles",
                b"Keine Daten verf\xFCgbar",
            ]),
        );
        text(
            hdc,
            f.small,
            rgb(0x70, 0x78, 0x88),
            px + 16,
            py + 72,
            b"V\xE9rifiez Z:\\run\\winlux\\weather.json",
        );
        return;
    }

    // Grande température + icône
    let mut big_temp = [0u8; 8];
    let btl = format_temp(&mut big_temp, TEMP);
    draw_weather_icon(hdc, b, px + 16, py + 42, 40, CONDITION);
    text(
        hdc,
        f.display,
        rgb(0xF2, 0xF2, 0xF2),
        px + 64,
        py + 46,
        &big_temp[..btl],
    );

    // Condition texte
    let cond_text = condition_label(CONDITION);
    text(
        hdc,
        f.body,
        rgb(0xC5, 0xC5, 0xC5),
        px + 64,
        py + 82,
        cond_text,
    );

    // Séparateur
    fill(
        hdc,
        b,
        px + 16,
        py + 110,
        px + pw - 16,
        py + 111,
        rgb(0x3A, 0x3F, 0x4A),
    );

    // Détails : ressenti, humidité, vent
    let detail_y = py + 120;
    let col_w = (pw - 32) / 3;

    // Ressenti
    let mut fl = [0u8; 8];
    let fll = format_temp(&mut fl, FEELS_LIKE);
    text(
        hdc,
        f.small,
        rgb(0x9A, 0xA3, 0xAE),
        px + 16,
        detail_y,
        tr4(&[b"Ressenti", b"Feels like", b"Sensaci\xF3n", b"Gef\xFChlt"]),
    );
    text(
        hdc,
        f.semibold,
        rgb(0xF2, 0xF2, 0xF2),
        px + 16,
        detail_y + 16,
        &fl[..fll],
    );

    // Humidité
    let mut hum = [0u8; 8];
    let hl = format_number(&mut hum, HUMIDITY);
    hum[hl] = b'%';
    text(
        hdc,
        f.small,
        rgb(0x9A, 0xA3, 0xAE),
        px + 16 + col_w,
        detail_y,
        tr4(&[b"Humidit\xE9", b"Humidity", b"Humedad", b"Luftfeuchtigkeit"]),
    );
    text(
        hdc,
        f.semibold,
        rgb(0xF2, 0xF2, 0xF2),
        px + 16 + col_w,
        detail_y + 16,
        &hum[..hl + 1],
    );

    // Vent
    let mut wnd = [0u8; 12];
    let wl = format_number(&mut wnd, WIND_KMH);
    let suffix = b" km/h";
    wnd[wl..wl + suffix.len()].copy_from_slice(suffix);
    text(
        hdc,
        f.small,
        rgb(0x9A, 0xA3, 0xAE),
        px + 16 + col_w * 2,
        detail_y,
        tr4(&[b"Vent", b"Wind", b"Viento", b"Wind"]),
    );
    text(
        hdc,
        f.semibold,
        rgb(0xF2, 0xF2, 0xF2),
        px + 16 + col_w * 2,
        detail_y + 16,
        &wnd[..wl + suffix.len()],
    );

    // Prévisions
    if FORECAST_COUNT > 0 {
        fill(
            hdc,
            b,
            px + 16,
            detail_y + 48,
            px + pw - 16,
            detail_y + 49,
            rgb(0x3A, 0x3F, 0x4A),
        );
        text(
            hdc,
            f.semibold,
            rgb(0xF2, 0xF2, 0xF2),
            px + 16,
            detail_y + 58,
            tr4(&[
                b"Pr\xE9visions",
                b"Forecast",
                b"Previsi\xF3n",
                b"Vorhersage",
            ]),
        );

        for i in 0..FORECAST_COUNT {
            let fy = detail_y + 80 + i as i32 * 52;
            let fc = &FORECAST[i];

            // Fond au survol
            if HOVER_ROW == i as i32 {
                round_rect(
                    hdc,
                    b,
                    px + 8,
                    fy - 4,
                    px + pw - 8,
                    fy + 40,
                    4,
                    rgb(0x2E, 0x33, 0x3D),
                );
            }

            // Jour
            if fc.day_len > 0 {
                text(
                    hdc,
                    f.body,
                    rgb(0xC5, 0xC5, 0xC5),
                    px + 16,
                    fy + 4,
                    &fc.day[..fc.day_len],
                );
            }

            // Icône
            draw_weather_icon(hdc, b, px + 80, fy, 28, fc.condition);

            // Min/Max
            let mut minmax = [0u8; 16];
            let mut pos = 0;
            let ml = format_temp(&mut minmax, fc.temp_min);
            pos += ml;
            minmax[pos] = b' ';
            pos += 1;
            minmax[pos] = b'/';
            pos += 1;
            minmax[pos] = b' ';
            pos += 1;
            let ml2 = format_temp(&mut minmax[pos..], fc.temp_max);
            pos += ml2;
            text(
                hdc,
                f.body,
                rgb(0xF2, 0xF2, 0xF2),
                px + 120,
                fy + 4,
                &minmax[..pos],
            );
        }
    }
}

fn condition_label(c: WeatherCondition) -> &'static [u8] {
    match c {
        WeatherCondition::Sunny => tr4(&[b"Ensoleill\xE9", b"Sunny", b"Soleado", b"Sonnig"]),
        WeatherCondition::PartlyCloudy => tr4(&[
            b"Partiellement nuageux",
            b"Partly cloudy",
            b"Parcialmente nublado",
            b"Teilweise bew\xF6lkt",
        ]),
        WeatherCondition::Cloudy => tr4(&[b"Nuageux", b"Cloudy", b"Nublado", b"Bew\xF6lkt"]),
        WeatherCondition::Rain => tr4(&[b"Pluie", b"Rain", b"Lluvia", b"Regen"]),
        WeatherCondition::Snow => tr4(&[b"Neige", b"Snow", b"Nieve", b"Schnee"]),
        WeatherCondition::Storm => tr4(&[b"Orage", b"Storm", b"Tormenta", b"Gewitter"]),
        WeatherCondition::Fog => tr4(&[b"Brouillard", b"Fog", b"Niebla", b"Nebel"]),
        WeatherCondition::Unknown => b"--",
    }
}

// ---------- Interaction souris ----------

pub unsafe fn panel_rect(bar_top: i32, screen_w: i32) -> Rect {
    if !PANEL_OPEN {
        return rect(0, 0, 0, 0);
    }
    let pw = PANEL_W;
    let ph = if FORECAST_COUNT > 0 {
        280 + FORECAST_COUNT as i32 * 52
    } else {
        260
    };
    let px = (screen_w - pw) / 2;
    let py = bar_top - 12 - ph;
    rect(px, py, px + pw, py + ph)
}

pub unsafe fn mouse_panel(x: i32, y: i32, bar_top: i32, screen_w: i32) -> bool {
    if !PANEL_OPEN {
        return false;
    }
    let pr = panel_rect(bar_top, screen_w);
    if !inside(&pr, x, y) {
        PANEL_OPEN = false;
        HOVER_ROW = -1;
        return true;
    }
    // Survol des prévisions
    let detail_y = pr.top + 12 + 120;
    let old = HOVER_ROW;
    HOVER_ROW = -1;
    for i in 0..FORECAST_COUNT as i32 {
        let fy = detail_y + 80 + i * 52;
        if y >= fy - 4 && y < fy + 40 {
            HOVER_ROW = i;
        }
    }
    HOVER_ROW != old
}

pub unsafe fn toggle() {
    PANEL_OPEN = !PANEL_OPEN;
    HOVER_ROW = -1;
}
