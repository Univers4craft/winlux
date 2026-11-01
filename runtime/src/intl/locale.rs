//! Locale Windows de Winlux, déduite des réglages Linux : la langue choisie
//! dans Paramètres (`/etc/winlux/locale.conf`, `LANG=`) et la région de la
//! locale glibc (`fr_BE.UTF-8`…). Les .exe reçoivent ainsi le LCID, les
//! pages de code, les séparateurs et les formats de date du système.

use std::sync::OnceLock;

/// Textes propres à une langue (noms des jours et des mois).
pub struct Language {
    pub iso639: &'static str,
    pub iso639_3: &'static str,
    pub english: &'static str,
    pub native: &'static str,
    /// Lundi en premier, comme LOCALE_SDAYNAME1.
    pub days: [&'static str; 7],
    pub days_abbr: [&'static str; 7],
    pub months: [&'static str; 12],
    pub months_abbr: [&'static str; 12],
    pub am: &'static str,
    pub pm: &'static str,
}

/// Une locale Windows (langue + région).
pub struct Locale {
    pub name: &'static str,
    pub lcid: u32,
    pub lang: &'static Language,
    pub country_english: &'static str,
    pub country_native: &'static str,
    pub iso3166: &'static str,
    pub iso3166_3: &'static str,
    pub ansi_cp: u32,
    pub oem_cp: u32,
    pub decimal: &'static str,
    pub thousand: &'static str,
    pub list: &'static str,
    pub currency: &'static str,
    pub int_currency: &'static str,
    /// LOCALE_ICURRENCY : 0 = « €1 », 1 = « 1€ », 2 = « € 1 », 3 = « 1 € ».
    pub currency_pos: u32,
    pub short_date: &'static str,
    pub long_date: &'static str,
    pub time_format: &'static str,
    pub short_time: &'static str,
    pub year_month: &'static str,
    /// LOCALE_IFIRSTDAYOFWEEK : 0 = lundi … 6 = dimanche.
    pub first_day: u32,
    /// LOCALE_IMEASURE : 0 = métrique, 1 = impérial.
    pub measure: u32,
    /// Identifiant de disposition clavier par défaut (KLID).
    pub keyboard: u32,
    /// GEOID de la région (GetUserGeoID).
    pub geo_id: u32,
}

const FRENCH: Language = Language {
    iso639: "fr",
    iso639_3: "fra",
    english: "French",
    native: "français",
    days: [
        "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
    ],
    days_abbr: ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."],
    months: [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ],
    months_abbr: [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ],
    am: "",
    pm: "",
};

const ENGLISH: Language = Language {
    iso639: "en",
    iso639_3: "eng",
    english: "English",
    native: "English",
    days: [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ],
    days_abbr: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    months: [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    months_abbr: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    am: "AM",
    pm: "PM",
};

const SPANISH: Language = Language {
    iso639: "es",
    iso639_3: "spa",
    english: "Spanish",
    native: "español",
    days: [
        "lunes",
        "martes",
        "miércoles",
        "jueves",
        "viernes",
        "sábado",
        "domingo",
    ],
    days_abbr: ["lun.", "mar.", "mié.", "jue.", "vie.", "sáb.", "dom."],
    months: [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ],
    months_abbr: [
        "ene.", "feb.", "mar.", "abr.", "may.", "jun.", "jul.", "ago.", "sept.", "oct.", "nov.",
        "dic.",
    ],
    am: "a. m.",
    pm: "p. m.",
};

const GERMAN: Language = Language {
    iso639: "de",
    iso639_3: "deu",
    english: "German",
    native: "Deutsch",
    days: [
        "Montag",
        "Dienstag",
        "Mittwoch",
        "Donnerstag",
        "Freitag",
        "Samstag",
        "Sonntag",
    ],
    days_abbr: ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"],
    months: [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    months_abbr: [
        "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
    ],
    am: "",
    pm: "",
};

const ITALIAN: Language = Language {
    iso639: "it",
    iso639_3: "ita",
    english: "Italian",
    native: "italiano",
    days: [
        "lunedì",
        "martedì",
        "mercoledì",
        "giovedì",
        "venerdì",
        "sabato",
        "domenica",
    ],
    days_abbr: ["lun", "mar", "mer", "gio", "ven", "sab", "dom"],
    months: [
        "gennaio",
        "febbraio",
        "marzo",
        "aprile",
        "maggio",
        "giugno",
        "luglio",
        "agosto",
        "settembre",
        "ottobre",
        "novembre",
        "dicembre",
    ],
    months_abbr: [
        "gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic",
    ],
    am: "",
    pm: "",
};

const PORTUGUESE: Language = Language {
    iso639: "pt",
    iso639_3: "por",
    english: "Portuguese",
    native: "português",
    days: [
        "segunda-feira",
        "terça-feira",
        "quarta-feira",
        "quinta-feira",
        "sexta-feira",
        "sábado",
        "domingo",
    ],
    days_abbr: ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"],
    months: [
        "janeiro",
        "fevereiro",
        "março",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ],
    months_abbr: [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ],
    am: "",
    pm: "",
};

macro_rules! loc {
    ($name:literal, $lcid:literal, $lang:ident, $ce:literal, $cn:literal, $iso:literal, $iso3:literal,
     $dec:literal, $th:literal, $list:literal, $cur:literal, $icur:literal, $cpos:literal,
     $sd:literal, $ld:literal, $tf:literal, $st:literal, $ym:literal, $first:literal, $measure:literal,
     $kb:literal, $geo:literal) => {
        Locale {
            name: $name,
            lcid: $lcid,
            lang: &$lang,
            country_english: $ce,
            country_native: $cn,
            iso3166: $iso,
            iso3166_3: $iso3,
            ansi_cp: 1252,
            oem_cp: 850,
            decimal: $dec,
            thousand: $th,
            list: $list,
            currency: $cur,
            int_currency: $icur,
            currency_pos: $cpos,
            short_date: $sd,
            long_date: $ld,
            time_format: $tf,
            short_time: $st,
            year_month: $ym,
            first_day: $first,
            measure: $measure,
            keyboard: $kb,
            geo_id: $geo,
        }
    };
}

/// Locales décrites par Winlux. La première de chaque langue est sa région
/// par défaut.
pub static LOCALES: [Locale; 14] = [
    loc!(
        "fr-FR",
        0x040C,
        FRENCH,
        "France",
        "France",
        "FR",
        "FRA",
        ",",
        "\u{202f}",
        ";",
        "€",
        "EUR",
        3,
        "dd/MM/yyyy",
        "dddd d MMMM yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM yyyy",
        0,
        0,
        0x0000_040C,
        84
    ),
    loc!(
        "fr-BE",
        0x080C,
        FRENCH,
        "Belgium",
        "Belgique",
        "BE",
        "BEL",
        ",",
        "\u{202f}",
        ";",
        "€",
        "EUR",
        3,
        "dd-MM-yy",
        "dddd d MMMM yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM yyyy",
        0,
        0,
        0x0000_080C,
        21
    ),
    loc!(
        "fr-CH",
        0x100C,
        FRENCH,
        "Switzerland",
        "Suisse",
        "CH",
        "CHE",
        ".",
        "\u{2019}",
        ";",
        "CHF",
        "CHF",
        2,
        "dd.MM.yyyy",
        "dddd, d MMMM yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM yyyy",
        0,
        0,
        0x0000_100C,
        223
    ),
    loc!(
        "fr-CA",
        0x0C0C,
        FRENCH,
        "Canada",
        "Canada",
        "CA",
        "CAN",
        ",",
        "\u{a0}",
        ";",
        "$",
        "CAD",
        3,
        "yyyy-MM-dd",
        "d MMMM yyyy",
        "HH:mm:ss",
        "HH 'h' mm",
        "MMMM yyyy",
        6,
        0,
        0x0000_1009,
        39
    ),
    loc!(
        "en-US",
        0x0409,
        ENGLISH,
        "United States",
        "United States",
        "US",
        "USA",
        ".",
        ",",
        ",",
        "$",
        "USD",
        0,
        "M/d/yyyy",
        "dddd, MMMM d, yyyy",
        "h:mm:ss tt",
        "h:mm tt",
        "MMMM yyyy",
        6,
        1,
        0x0000_0409,
        244
    ),
    loc!(
        "en-GB",
        0x0809,
        ENGLISH,
        "United Kingdom",
        "United Kingdom",
        "GB",
        "GBR",
        ".",
        ",",
        ",",
        "£",
        "GBP",
        0,
        "dd/MM/yyyy",
        "dd MMMM yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM yyyy",
        0,
        0,
        0x0000_0809,
        242
    ),
    loc!(
        "es-ES",
        0x0C0A,
        SPANISH,
        "Spain",
        "España",
        "ES",
        "ESP",
        ",",
        ".",
        ";",
        "€",
        "EUR",
        3,
        "dd/MM/yyyy",
        "dddd, d' de 'MMMM' de 'yyyy",
        "H:mm:ss",
        "H:mm",
        "MMMM' de 'yyyy",
        0,
        0,
        0x0000_040A,
        217
    ),
    loc!(
        "es-MX",
        0x080A,
        SPANISH,
        "Mexico",
        "México",
        "MX",
        "MEX",
        ".",
        ",",
        ",",
        "$",
        "MXN",
        0,
        "dd/MM/yyyy",
        "dddd, d' de 'MMMM' de 'yyyy",
        "hh:mm:ss tt",
        "hh:mm tt",
        "MMMM' de 'yyyy",
        6,
        0,
        0x0000_080A,
        166
    ),
    loc!(
        "de-DE",
        0x0407,
        GERMAN,
        "Germany",
        "Deutschland",
        "DE",
        "DEU",
        ",",
        ".",
        ";",
        "€",
        "EUR",
        3,
        "dd.MM.yyyy",
        "dddd, d. MMMM yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM yyyy",
        0,
        0,
        0x0000_0407,
        94
    ),
    loc!(
        "de-AT",
        0x0C07,
        GERMAN,
        "Austria",
        "Österreich",
        "AT",
        "AUT",
        ",",
        "\u{a0}",
        ";",
        "€",
        "EUR",
        2,
        "dd.MM.yyyy",
        "dddd, d. MMMM yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM yyyy",
        0,
        0,
        0x0000_0C07,
        14
    ),
    loc!(
        "de-CH",
        0x0807,
        GERMAN,
        "Switzerland",
        "Schweiz",
        "CH",
        "CHE",
        ".",
        "\u{2019}",
        ";",
        "CHF",
        "CHF",
        2,
        "dd.MM.yyyy",
        "dddd, d. MMMM yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM yyyy",
        0,
        0,
        0x0000_0807,
        223
    ),
    loc!(
        "it-IT",
        0x0410,
        ITALIAN,
        "Italy",
        "Italia",
        "IT",
        "ITA",
        ",",
        ".",
        ";",
        "€",
        "EUR",
        3,
        "dd/MM/yyyy",
        "dddd d MMMM yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM yyyy",
        0,
        0,
        0x0000_0410,
        118
    ),
    loc!(
        "pt-PT",
        0x0816,
        PORTUGUESE,
        "Portugal",
        "Portugal",
        "PT",
        "PRT",
        ",",
        "\u{a0}",
        ";",
        "€",
        "EUR",
        3,
        "dd/MM/yyyy",
        "dddd, d' de 'MMMM' de 'yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM' de 'yyyy",
        6,
        0,
        0x0000_0816,
        193
    ),
    loc!(
        "pt-BR",
        0x0416,
        PORTUGUESE,
        "Brazil",
        "Brasil",
        "BR",
        "BRA",
        ",",
        ".",
        ";",
        "R$",
        "BRL",
        2,
        "dd/MM/yyyy",
        "dddd, d' de 'MMMM' de 'yyyy",
        "HH:mm:ss",
        "HH:mm",
        "MMMM' de 'yyyy",
        6,
        0,
        0x0000_0416,
        32
    ),
];

/// Locale invariante (LCID 0x7F, nom vide) : formats fixes, en anglais.
pub static INVARIANT: Locale = Locale {
    name: "",
    lcid: 0x007F,
    lang: &INVARIANT_LANGUAGE,
    country_english: "Invariant Country",
    country_native: "Invariant Country",
    iso3166: "IV",
    iso3166_3: "IVC",
    ansi_cp: 1252,
    oem_cp: 437,
    decimal: ".",
    thousand: ",",
    list: ",",
    currency: "\u{a4}",
    int_currency: "XDR",
    currency_pos: 0,
    short_date: "MM/dd/yyyy",
    long_date: "dddd, dd MMMM yyyy",
    time_format: "HH:mm:ss",
    short_time: "HH:mm",
    year_month: "yyyy MMMM",
    first_day: 6,
    measure: 0,
    keyboard: 0x0000_0409,
    geo_id: 244,
};

const INVARIANT_LANGUAGE: Language = Language {
    iso639: "iv",
    iso639_3: "IVL",
    english: "Invariant Language",
    native: "Invariant Language",
    ..ENGLISH
};

/// Locale d'une étiquette Linux ou Windows (`fr`, `fr_BE.UTF-8`, `de-CH`,
/// `pt_BR@euro`…). Une langue sans région prend sa région par défaut.
pub fn from_tag(tag: &str) -> Option<&'static Locale> {
    if tag.is_empty() {
        return Some(&INVARIANT);
    }
    let tag = tag.split(['.', '@']).next().unwrap_or("").trim();
    let mut parts = tag.split(['_', '-']);
    let lang = parts.next()?.to_ascii_lowercase();
    let region = parts.next().map(|r| r.to_ascii_uppercase());
    if let Some(r) = &region {
        if let Some(l) = LOCALES
            .iter()
            .find(|l| l.lang.iso639 == lang && l.iso3166 == r)
        {
            return Some(l);
        }
    }
    LOCALES.iter().find(|l| l.lang.iso639 == lang)
}

pub fn from_lcid(lcid: u32) -> Option<&'static Locale> {
    let id = lcid & 0xFFFF;
    if id == 0x7F {
        return Some(&INVARIANT);
    }
    LOCALES
        .iter()
        .find(|l| l.lcid == id)
        // LANG_xxx seul (sous-langue neutre) : la région par défaut.
        .or_else(|| LOCALES.iter().find(|l| l.lcid & 0x3FF == id && id < 0x400))
}

fn conf_value(path: &str, key: &str) -> Option<String> {
    let root = std::env::var("WINLUX_CONTROL_ROOT").unwrap_or_default();
    let text = std::fs::read_to_string(format!("{root}{path}")).ok()?;
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key)
            .then(|| v.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|v| !v.is_empty())
    })
}

/// Étiquette glibc de l'environnement (`LC_ALL`, puis `LC_CTYPE`, `LANG`),
/// sinon celle de /etc/default/locale.
fn glibc_tag() -> Option<String> {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX" && !v.starts_with("C."))
        .or_else(|| conf_value("/etc/default/locale", "LANG"))
}

/// Langue de l'interface choisie dans Paramètres (`fr`, `en`, `es`, `de`).
pub fn ui_language_tag() -> Option<String> {
    std::env::var("WINLUX_LANG")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| conf_value("/etc/winlux/locale.conf", "LANG"))
}

/// Calcule la locale : la région de glibc quand elle correspond à la langue
/// de Paramètres, sinon la région par défaut de cette langue.
pub fn detect(ui: Option<&str>, glibc: Option<&str>) -> &'static Locale {
    let from_glibc = glibc.and_then(from_tag);
    match ui.and_then(from_tag) {
        Some(ui) => match from_glibc {
            Some(g) if g.lang.iso639 == ui.lang.iso639 => g,
            _ => ui,
        },
        None => from_glibc.unwrap_or(&LOCALES[0]),
    }
}

/// Locale de l'utilisateur, calculée une fois par processus.
pub fn current() -> &'static Locale {
    static CURRENT: OnceLock<&'static Locale> = OnceLock::new();
    CURRENT.get_or_init(|| {
        if let Some(l) = std::env::var("WINLUX_LOCALE")
            .ok()
            .and_then(|t| from_tag(&t))
        {
            return l;
        }
        detect(ui_language_tag().as_deref(), glibc_tag().as_deref())
    })
}

/// Langue d'interface (GetUserDefaultUILanguage) : la langue de Paramètres
/// dans sa région par défaut, ou la locale de l'utilisateur.
pub fn ui_locale() -> &'static Locale {
    static UI: OnceLock<&'static Locale> = OnceLock::new();
    UI.get_or_init(|| {
        let user = current();
        match ui_language_tag().as_deref().and_then(from_tag) {
            Some(ui) if ui.lang.iso639 != user.lang.iso639 => ui,
            _ => user,
        }
    })
}

// ---------------------------------------------------------------------
// Formats de date et d'heure (images « dd/MM/yyyy », « HH:mm:ss tt »)
// ---------------------------------------------------------------------

/// Champs utiles d'un SYSTEMTIME.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub millis: u16,
}

/// Applique une image de format Windows. Les textes entre apostrophes sont
/// recopiés tels quels ; `''` donne une apostrophe.
pub fn format_picture(loc: &Locale, picture: &str, t: &DateTime) -> String {
    let chars: Vec<char> = picture.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    // Le mois en toutes lettres prend la forme génitive après un jour ; les
    // langues décrites ici n'en ont pas besoin.
    let weekday = (t.day_of_week as usize + 6) % 7; // 0 = lundi
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            i += 1;
            if i < chars.len() && chars[i] == '\'' {
                out.push('\'');
                i += 1;
                continue;
            }
            while i < chars.len() && chars[i] != '\'' {
                out.push(chars[i]);
                i += 1;
            }
            i += 1;
            continue;
        }
        let mut n = 1;
        while i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
        let hour12 = match t.hour % 12 {
            0 => 12,
            h => h,
        };
        match c {
            'd' => match n {
                1 => out.push_str(&t.day.to_string()),
                2 => out.push_str(&format!("{:02}", t.day)),
                3 => out.push_str(loc.lang.days_abbr[weekday]),
                _ => out.push_str(loc.lang.days[weekday]),
            },
            'M' => {
                let m = (t.month.clamp(1, 12) - 1) as usize;
                match n {
                    1 => out.push_str(&t.month.to_string()),
                    2 => out.push_str(&format!("{:02}", t.month)),
                    3 => out.push_str(loc.lang.months_abbr[m]),
                    _ => out.push_str(loc.lang.months[m]),
                }
            }
            'y' => match n {
                1 => out.push_str(&(t.year % 100).to_string()),
                2 => out.push_str(&format!("{:02}", t.year % 100)),
                _ => out.push_str(&format!("{:04}", t.year)),
            },
            'g' => {}
            'h' => out.push_str(&if n >= 2 {
                format!("{hour12:02}")
            } else {
                hour12.to_string()
            }),
            'H' => out.push_str(&if n >= 2 {
                format!("{:02}", t.hour)
            } else {
                t.hour.to_string()
            }),
            'm' => out.push_str(&if n >= 2 {
                format!("{:02}", t.minute)
            } else {
                t.minute.to_string()
            }),
            's' => out.push_str(&if n >= 2 {
                format!("{:02}", t.second)
            } else {
                t.second.to_string()
            }),
            't' => {
                let marker = if t.hour < 12 {
                    loc.lang.am
                } else {
                    loc.lang.pm
                };
                if n == 1 {
                    out.extend(marker.chars().take(1));
                } else {
                    out.push_str(marker);
                }
            }
            _ => {
                for _ in 0..n {
                    out.push(c);
                }
            }
        }
        i += n;
    }
    // Sans indicateur AM/PM, l'espace qui le précédait disparaît.
    out.trim_end().to_string()
}

/// Nombre formaté selon la locale (séparateurs et groupes de trois).
pub fn format_number(loc: &Locale, value: &str, decimals: Option<usize>) -> Option<String> {
    let (neg, digits) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let (int_part, frac_part) = digits.split_once('.').unwrap_or((digits, ""));
    if int_part.is_empty() && frac_part.is_empty()
        || !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let want = decimals.unwrap_or(2);
    // Arrondi au plus proche, la moitié s'éloignant de zéro (comme Windows).
    let mut all: Vec<u8> = int_part.bytes().collect();
    let int_len = all.len();
    all.extend(frac_part.bytes().take(want));
    while all.len() < int_len + want {
        all.push(b'0');
    }
    if frac_part.as_bytes().get(want).is_some_and(|&d| d >= b'5') {
        let mut i = all.len();
        loop {
            if i == 0 {
                all.insert(0, b'1');
                break;
            }
            i -= 1;
            if all[i] == b'9' {
                all[i] = b'0';
            } else {
                all[i] += 1;
                break;
            }
        }
    }
    let split = all.len() - want;
    let frac = String::from_utf8_lossy(&all[split..]).into_owned();
    let int_owned = String::from_utf8_lossy(&all[..split]).into_owned();
    let int_part = if int_owned.is_empty() {
        "0"
    } else {
        int_owned.as_str()
    };
    let mut grouped = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push_str(loc.thousand);
        }
        grouped.push(c);
    }
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&grouped);
    if want > 0 {
        out.push_str(loc.decimal);
        out.push_str(&frac);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_langue_de_parametres_et_la_region_glibc() {
        assert_eq!(detect(Some("fr"), Some("fr_BE.UTF-8")).name, "fr-BE");
        assert_eq!(detect(Some("fr"), Some("en_US.UTF-8")).name, "fr-FR");
        assert_eq!(detect(Some("de"), None).name, "de-DE");
        assert_eq!(detect(None, Some("es_MX.UTF-8")).name, "es-MX");
        assert_eq!(detect(Some("xx"), Some("C.UTF-8")).name, "fr-FR");
        assert_eq!(detect(Some("en"), Some("en_GB.UTF-8")).lcid, 0x0809);
        assert_eq!(from_lcid(0x0C).map(|l| l.name), Some("fr-FR"));
        assert_eq!(from_lcid(0x0407).map(|l| l.lang.native), Some("Deutsch"));
    }

    #[test]
    fn formats_de_date_et_d_heure() {
        let t = DateTime {
            year: 2026,
            month: 8,
            day_of_week: 6,
            day: 1,
            hour: 14,
            minute: 5,
            second: 9,
            millis: 0,
        };
        let fr = from_tag("fr-FR").unwrap();
        assert_eq!(format_picture(fr, fr.short_date, &t), "01/08/2026");
        assert_eq!(format_picture(fr, fr.long_date, &t), "samedi 1 août 2026");
        assert_eq!(format_picture(fr, fr.time_format, &t), "14:05:09");
        let us = from_tag("en_US").unwrap();
        assert_eq!(format_picture(us, us.short_date, &t), "8/1/2026");
        assert_eq!(format_picture(us, us.time_format, &t), "2:05:09 PM");
        let es = from_tag("es").unwrap();
        assert_eq!(
            format_picture(es, es.long_date, &t),
            "sábado, 1 de agosto de 2026"
        );
        assert_eq!(format_picture(fr, "''HH'h'mm", &t), "'14h05");
    }

    #[test]
    fn nombres_avec_separateurs() {
        let de = from_tag("de").unwrap();
        assert_eq!(
            format_number(de, "-1234567.891", None).unwrap(),
            "-1.234.567,89"
        );
        let us = from_tag("en").unwrap();
        assert_eq!(format_number(us, "12.5", Some(0)).unwrap(), "13");
        assert_eq!(format_number(us, "999.996", None).unwrap(), "1,000.00");
        assert!(format_number(us, "1e5", None).is_none());
    }
}
