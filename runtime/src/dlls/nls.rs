//! Données de langue (NLS) : GetLocaleInfo(Ex), GetCalendarInfoEx,
//! EnumCalendarInfoExEx, EnumDateFormatsExEx, EnumTimeFormatsEx,
//! ResolveLocaleName, EnumSystemLocalesEx, CompareStringEx, FindNLSStringEx…
//!
//! .NET (sans ICU) et de nombreux programmes lisent ici les formats de dates,
//! de nombres et de monnaie, les noms des jours et des mois. Winlux décrit
//! les langues de son interface (français, anglais, espagnol, allemand),
//! leurs formes neutres (« fr », « en »…) et la locale invariante. Les
//! valeurs suivent les conventions publiées de chaque pays.

use std::ffi::c_void;

const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INVALID_FLAGS: u32 = 1004;

const LOCALE_RETURN_NUMBER: u32 = 0x2000_0000;
const LOCALE_RETURN_GENITIVE_NAMES: u32 = 0x1000_0000;
const CAL_RETURN_NUMBER: u32 = 0x2000_0000;
const CAL_GREGORIAN: u32 = 1;
const ENUM_ALL_CALENDARS: u32 = 0xFFFF_FFFF;

/// Locale de l'utilisateur Winlux (même valeur que GetUserDefaultLCID).
pub const USER_LOCALE: &str = "fr-FR";
pub const USER_LCID: u32 = 0x040C;

pub struct Locale {
    pub name: &'static str,
    pub lcid: u32,
    /// Forme spécifique d'une locale neutre (« fr » → « fr-FR »).
    specific: &'static str,
    parent: &'static str,
    lang2: &'static str,
    lang3: &'static str,
    ctry2: &'static str,
    ctry3: &'static str,
    eng_lang: &'static str,
    eng_ctry: &'static str,
    nat_lang: &'static str,
    nat_ctry: &'static str,
    decimal: &'static str,
    thousand: &'static str,
    currency: &'static str,
    intl_currency: &'static str,
    /// LOCALE_ICURRENCY / LOCALE_INEGCURR.
    curr_pos: u32,
    curr_neg: u32,
    eng_curr: &'static str,
    nat_curr: &'static str,
    list: &'static str,
    /// 0 métrique, 1 impérial (États-Unis).
    measure: u32,
    paper: u32,
    /// 0 = lundi … 6 = dimanche.
    first_day: u32,
    /// 0 : semaine du 1er janvier, 2 : première semaine de 4 jours.
    first_week: u32,
    short_dates: &'static [&'static str],
    long_dates: &'static [&'static str],
    year_month: &'static str,
    month_day: &'static str,
    long_times: &'static [&'static str],
    short_times: &'static [&'static str],
    am: &'static str,
    pm: &'static str,
    /// Lundi en premier, comme LOCALE_SDAYNAME1.
    days: [&'static str; 7],
    abbrev_days: [&'static str; 7],
    shortest_days: [&'static str; 7],
    months: [&'static str; 12],
    genitive_months: Option<[&'static str; 12]>,
    abbrev_months: [&'static str; 12],
    era: &'static str,
    abbrev_era: &'static str,
    calendar_name: &'static str,
    geo: u32,
    oem_cp: u32,
    neg_percent: u32,
    pos_percent: u32,
    pos_infinity: &'static str,
    neg_infinity: &'static str,
    sort_name: &'static str,
}

const EN_DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const EN_ABBREV_DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const EN_SHORTEST_DAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
const EN_MONTHS: [&str; 12] = [
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
];
const EN_ABBREV_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const FR_DAYS: [&str; 7] = [
    "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
];
const FR_ABBREV_DAYS: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
const FR_SHORTEST_DAYS: [&str; 7] = ["lu", "ma", "me", "je", "ve", "sa", "di"];
const FR_MONTHS: [&str; 12] = [
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
];
const FR_ABBREV_MONTHS: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
    "déc.",
];
const ES_DAYS: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];
const ES_ABBREV_DAYS: [&str; 7] = ["lun.", "mar.", "mié.", "jue.", "vie.", "sáb.", "dom."];
const ES_SHORTEST_DAYS: [&str; 7] = ["LU", "MA", "MI", "JU", "VI", "SA", "DO"];
const ES_MONTHS: [&str; 12] = [
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
];
const ES_ABBREV_MONTHS: [&str; 12] = [
    "ene.", "feb.", "mar.", "abr.", "may.", "jun.", "jul.", "ago.", "sept.", "oct.", "nov.", "dic.",
];
const DE_DAYS: [&str; 7] = [
    "Montag",
    "Dienstag",
    "Mittwoch",
    "Donnerstag",
    "Freitag",
    "Samstag",
    "Sonntag",
];
const DE_ABBREV_DAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
const DE_MONTHS: [&str; 12] = [
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
];
const DE_ABBREV_MONTHS: [&str; 12] = [
    "Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez.",
];

const INVARIANT: Locale = Locale {
    name: "",
    lcid: 0x7F,
    specific: "",
    parent: "",
    lang2: "iv",
    lang3: "ivl",
    ctry2: "IV",
    ctry3: "ivc",
    eng_lang: "Invariant Language",
    eng_ctry: "Invariant Country",
    nat_lang: "Invariant Language",
    nat_ctry: "Invariant Country",
    decimal: ".",
    thousand: ",",
    currency: "¤",
    intl_currency: "XDR",
    curr_pos: 0,
    curr_neg: 0,
    eng_curr: "International Monetary Fund",
    nat_curr: "International Monetary Fund",
    list: ",",
    measure: 0,
    paper: 9,
    first_day: 6,
    first_week: 0,
    short_dates: &["MM/dd/yyyy", "yyyy-MM-dd"],
    long_dates: &["dddd, dd MMMM yyyy"],
    year_month: "yyyy MMMM",
    month_day: "MMMM dd",
    long_times: &["HH:mm:ss"],
    short_times: &["HH:mm", "hh:mm tt", "H:mm", "h:mm tt"],
    am: "AM",
    pm: "PM",
    days: EN_DAYS,
    abbrev_days: EN_ABBREV_DAYS,
    shortest_days: EN_SHORTEST_DAYS,
    months: EN_MONTHS,
    genitive_months: None,
    abbrev_months: EN_ABBREV_MONTHS,
    era: "A.D.",
    abbrev_era: "AD",
    calendar_name: "Gregorian Calendar",
    geo: 244,
    oem_cp: 437,
    neg_percent: 0,
    pos_percent: 0,
    pos_infinity: "Infinity",
    neg_infinity: "-Infinity",
    sort_name: "",
};

const EN_US: Locale = Locale {
    name: "en-US",
    lcid: 0x0409,
    specific: "en-US",
    parent: "en",
    lang2: "en",
    lang3: "eng",
    ctry2: "US",
    ctry3: "USA",
    eng_lang: "English",
    eng_ctry: "United States",
    nat_lang: "English",
    nat_ctry: "United States",
    decimal: ".",
    thousand: ",",
    currency: "$",
    intl_currency: "USD",
    curr_pos: 0,
    curr_neg: 1,
    eng_curr: "US Dollar",
    nat_curr: "US Dollar",
    list: ",",
    measure: 1,
    paper: 1,
    first_day: 6,
    first_week: 0,
    short_dates: &[
        "M/d/yyyy",
        "M/d/yy",
        "MM/dd/yy",
        "MM/dd/yyyy",
        "yy/MM/dd",
        "yyyy-MM-dd",
        "dd-MMM-yy",
    ],
    long_dates: &[
        "dddd, MMMM d, yyyy",
        "MMMM d, yyyy",
        "dddd, d MMMM, yyyy",
        "d MMMM, yyyy",
    ],
    year_month: "MMMM yyyy",
    month_day: "MMMM d",
    long_times: &["h:mm:ss tt", "hh:mm:ss tt", "H:mm:ss", "HH:mm:ss"],
    short_times: &["h:mm tt", "hh:mm tt", "H:mm", "HH:mm"],
    am: "AM",
    pm: "PM",
    days: EN_DAYS,
    abbrev_days: EN_ABBREV_DAYS,
    shortest_days: EN_SHORTEST_DAYS,
    months: EN_MONTHS,
    genitive_months: None,
    abbrev_months: EN_ABBREV_MONTHS,
    era: "A.D.",
    abbrev_era: "AD",
    calendar_name: "Gregorian Calendar",
    geo: 244,
    oem_cp: 437,
    neg_percent: 1,
    pos_percent: 1,
    pos_infinity: "∞",
    neg_infinity: "-∞",
    sort_name: "en-US",
};

const FR_FR: Locale = Locale {
    name: "fr-FR",
    lcid: 0x040C,
    specific: "fr-FR",
    parent: "fr",
    lang2: "fr",
    lang3: "fra",
    ctry2: "FR",
    ctry3: "FRA",
    eng_lang: "French",
    eng_ctry: "France",
    nat_lang: "français",
    nat_ctry: "France",
    decimal: ",",
    thousand: "\u{202F}",
    currency: "€",
    intl_currency: "EUR",
    curr_pos: 3,
    curr_neg: 8,
    eng_curr: "Euro",
    nat_curr: "euro",
    list: ";",
    measure: 0,
    paper: 9,
    first_day: 0,
    first_week: 2,
    short_dates: &[
        "dd/MM/yyyy",
        "dd/MM/yy",
        "dd.MM.yy",
        "dd-MM-yy",
        "yyyy-MM-dd",
    ],
    long_dates: &[
        "dddd d MMMM yyyy",
        "d MMM yy",
        "d MMMM yyyy",
        "dddd d MMM yyyy",
    ],
    year_month: "MMMM yyyy",
    month_day: "d MMMM",
    long_times: &["HH:mm:ss", "HH' h 'mm"],
    short_times: &["HH:mm", "HH' h 'mm"],
    am: "",
    pm: "",
    days: FR_DAYS,
    abbrev_days: FR_ABBREV_DAYS,
    shortest_days: FR_SHORTEST_DAYS,
    months: FR_MONTHS,
    genitive_months: None,
    abbrev_months: FR_ABBREV_MONTHS,
    era: "ap. J.-C.",
    abbrev_era: "ap. J.-C.",
    calendar_name: "calendrier grégorien",
    geo: 84,
    oem_cp: 850,
    neg_percent: 0,
    pos_percent: 0,
    pos_infinity: "∞",
    neg_infinity: "-∞",
    sort_name: "fr-FR",
};

const ES_ES: Locale = Locale {
    name: "es-ES",
    lcid: 0x0C0A,
    specific: "es-ES",
    parent: "es",
    lang2: "es",
    lang3: "spa",
    ctry2: "ES",
    ctry3: "ESP",
    eng_lang: "Spanish",
    eng_ctry: "Spain",
    nat_lang: "español",
    nat_ctry: "España",
    decimal: ",",
    thousand: ".",
    currency: "€",
    intl_currency: "EUR",
    curr_pos: 3,
    curr_neg: 8,
    eng_curr: "Euro",
    nat_curr: "euro",
    list: ";",
    measure: 0,
    paper: 9,
    first_day: 0,
    first_week: 2,
    short_dates: &["dd/MM/yyyy", "dd/MM/yy", "d/M/yy", "dd-MM-yy", "yyyy-MM-dd"],
    long_dates: &[
        "dddd, d' de 'MMMM' de 'yyyy",
        "d' de 'MMMM' de 'yyyy",
        "dddd d' de 'MMMM' de 'yyyy",
    ],
    year_month: "MMMM' de 'yyyy",
    month_day: "d' de 'MMMM",
    long_times: &["H:mm:ss", "HH:mm:ss"],
    short_times: &["H:mm", "HH:mm"],
    am: "a.\u{A0}m.",
    pm: "p.\u{A0}m.",
    days: ES_DAYS,
    abbrev_days: ES_ABBREV_DAYS,
    shortest_days: ES_SHORTEST_DAYS,
    months: ES_MONTHS,
    genitive_months: None,
    abbrev_months: ES_ABBREV_MONTHS,
    era: "d. C.",
    abbrev_era: "d. C.",
    calendar_name: "calendario gregoriano",
    geo: 217,
    oem_cp: 850,
    neg_percent: 0,
    pos_percent: 0,
    pos_infinity: "∞",
    neg_infinity: "-∞",
    sort_name: "es-ES",
};

const DE_DE: Locale = Locale {
    name: "de-DE",
    lcid: 0x0407,
    specific: "de-DE",
    parent: "de",
    lang2: "de",
    lang3: "deu",
    ctry2: "DE",
    ctry3: "DEU",
    eng_lang: "German",
    eng_ctry: "Germany",
    nat_lang: "Deutsch",
    nat_ctry: "Deutschland",
    decimal: ",",
    thousand: ".",
    currency: "€",
    intl_currency: "EUR",
    curr_pos: 3,
    curr_neg: 8,
    eng_curr: "Euro",
    nat_curr: "Euro",
    list: ";",
    measure: 0,
    paper: 9,
    first_day: 0,
    first_week: 2,
    short_dates: &["dd.MM.yyyy", "dd.MM.yy", "d.M.yy", "dd/MM/yy", "yyyy-MM-dd"],
    long_dates: &["dddd, d. MMMM yyyy", "d. MMMM yyyy", "d. MMM. yyyy"],
    year_month: "MMMM yyyy",
    month_day: "d. MMMM",
    long_times: &["HH:mm:ss", "H:mm:ss"],
    short_times: &["HH:mm", "H:mm"],
    am: "",
    pm: "",
    days: DE_DAYS,
    abbrev_days: DE_ABBREV_DAYS,
    shortest_days: DE_ABBREV_DAYS,
    months: DE_MONTHS,
    genitive_months: None,
    abbrev_months: DE_ABBREV_MONTHS,
    era: "n. Chr.",
    abbrev_era: "n. Chr.",
    calendar_name: "Gregorianischer Kalender",
    geo: 94,
    oem_cp: 850,
    neg_percent: 0,
    pos_percent: 0,
    pos_infinity: "∞",
    neg_infinity: "-∞",
    sort_name: "de-DE",
};

/// Forme neutre d'une locale : mêmes formats, sans pays.
const fn neutral(l: Locale, name: &'static str, lcid: u32) -> Locale {
    Locale {
        name,
        lcid,
        specific: l.name,
        parent: "",
        ctry2: "",
        ctry3: "",
        sort_name: name,
        ..l
    }
}

static LOCALES: [Locale; 9] = [
    INVARIANT,
    EN_US,
    FR_FR,
    ES_ES,
    DE_DE,
    neutral(EN_US, "en", 0x09),
    neutral(FR_FR, "fr", 0x0C),
    neutral(ES_ES, "es", 0x0A),
    neutral(DE_DE, "de", 0x07),
];

impl Locale {
    fn is_neutral(&self) -> bool {
        !self.name.is_empty() && !self.name.contains('-')
    }
    fn display(&self, english: bool) -> String {
        let (lang, ctry) = if english {
            (self.eng_lang, self.eng_ctry)
        } else {
            (self.nat_lang, self.nat_ctry)
        };
        if self.name.is_empty() || self.is_neutral() {
            lang.to_string()
        } else {
            format!("{lang} ({ctry})")
        }
    }
}

/// Locale désignée par un nom (insensible à la casse, `_` accepté).
/// `None` (LOCALE_NAME_USER_DEFAULT) et « !x-sys-default-locale » donnent
/// celle de l'utilisateur.
pub fn by_name(name: Option<&str>) -> Option<&'static Locale> {
    let n = match name {
        None => USER_LOCALE.to_string(),
        Some(n) if n.eq_ignore_ascii_case("!x-sys-default-locale") => USER_LOCALE.to_string(),
        Some(n) => n.replace('_', "-"),
    };
    LOCALES.iter().find(|l| l.name.eq_ignore_ascii_case(&n))
}

/// Locale désignée par un LCID ; LOCALE_USER_DEFAULT (0x400),
/// LOCALE_SYSTEM_DEFAULT (0x800) et 0 donnent celle de l'utilisateur.
pub fn by_lcid(lcid: u32) -> Option<&'static Locale> {
    let lcid = match lcid & 0xFFFF {
        0 | 0x0400 | 0x0800 => USER_LCID,
        x => x,
    };
    LOCALES.iter().find(|l| l.lcid == lcid)
}

fn set_last_error(e: u32) {
    crate::process::set_last_error(e);
}

/// Valeur d'un LCTYPE : texte ou nombre.
enum Info {
    Text(String),
    Number(u32),
}

fn info(l: &Locale, lc_type: u32) -> Option<Info> {
    use Info::{Number as N, Text as T};
    let t = |s: &str| Some(T(s.to_string()));
    let genitive = lc_type & LOCALE_RETURN_GENITIVE_NAMES != 0;
    let base = lc_type & 0xFFFF;
    match base {
        0x01 => Some(N(l.lcid)),                  // LOCALE_ILANGUAGE
        0x02 | 0x73 => Some(T(l.display(false))), // LOCALE_SLOCALIZEDDISPLAYNAME, SNATIVEDISPLAYNAME
        0x72 => Some(T(l.display(true))),         // LOCALE_SENGLISHDISPLAYNAME
        0x03 => t(&l.lang3.to_ascii_uppercase()), // LOCALE_SABBREVLANGNAME (« FRA »)
        0x04 | 0x6F => t(l.nat_lang), // LOCALE_SNATIVELANGUAGENAME, SLOCALIZEDLANGUAGENAME
        0x05 => Some(N(l.geo)),       // LOCALE_ICOUNTRY (approché)
        0x06 | 0x08 => t(l.nat_ctry), // LOCALE_SLOCALIZEDCOUNTRYNAME, SNATIVECOUNTRYNAME
        0x07 => t(&l.ctry3.to_ascii_uppercase()), // LOCALE_SABBREVCTRYNAME
        0x09 => Some(N(l.lcid & 0x3FF)), // LOCALE_IDEFAULTLANGUAGE
        0x0A => Some(N(l.geo)),       // LOCALE_IDEFAULTCOUNTRY
        0x0B => Some(N(l.oem_cp)),    // LOCALE_IDEFAULTCODEPAGE
        0x0C => t(l.list),            // LOCALE_SLIST
        0x0D => Some(N(l.measure)),   // LOCALE_IMEASURE
        0x0E => t(l.decimal),         // LOCALE_SDECIMAL
        0x0F => t(l.thousand),        // LOCALE_STHOUSAND
        0x10 | 0x18 => t("3;0"),      // LOCALE_SGROUPING, SMONGROUPING
        0x11 | 0x19 => Some(N(2)),    // LOCALE_IDIGITS, ICURRDIGITS
        0x12 => Some(N(1)),           // LOCALE_ILZERO
        0x13 => t("0123456789"),      // LOCALE_SNATIVEDIGITS
        0x14 => t(l.currency),        // LOCALE_SCURRENCY
        0x15 => t(l.intl_currency),   // LOCALE_SINTLSYMBOL
        0x16 => t(l.decimal),         // LOCALE_SMONDECIMALSEP
        0x17 => t(l.thousand),        // LOCALE_SMONTHOUSANDSEP
        0x1A => Some(N(2)),           // LOCALE_IINTLCURRDIGITS
        0x1B => Some(N(l.curr_pos)),  // LOCALE_ICURRENCY
        0x1C => Some(N(l.curr_neg)),  // LOCALE_INEGCURR
        0x1D => t(date_sep(l.short_dates[0])), // LOCALE_SDATE
        0x1E => t(":"),               // LOCALE_STIME
        0x1F => t(l.short_dates[0]),  // LOCALE_SSHORTDATE
        0x20 => t(l.long_dates[0]),   // LOCALE_SLONGDATE
        0x21 => Some(N(0)),           // LOCALE_IDATE (approché)
        0x22 => Some(N(0)),           // LOCALE_ILDATE
        0x23 => Some(N(u32::from(!l.long_times[0].starts_with('h')))), // LOCALE_ITIME
        0x24 => Some(N(u32::from(
            l.long_times[0].starts_with("HH") || l.long_times[0].starts_with("hh"),
        ))), // LOCALE_ITLZERO
        0x25 => Some(N(1)),           // LOCALE_ICENTURY
        0x26 => Some(N(1)),           // LOCALE_IDAYLZERO
        0x27 => Some(N(1)),           // LOCALE_IMONLZERO
        0x28 => t(l.am),              // LOCALE_S1159
        0x29 => t(l.pm),              // LOCALE_S2359
        0x2A..=0x30 => t(l.days[(base - 0x2A) as usize]),
        0x31..=0x37 => t(l.abbrev_days[(base - 0x31) as usize]),
        0x38..=0x43 => {
            let i = (base - 0x38) as usize;
            t(if genitive {
                l.genitive_months.unwrap_or(l.months)[i]
            } else {
                l.months[i]
            })
        }
        0x44..=0x4F => t(l.abbrev_months[(base - 0x44) as usize]),
        0x50 => t(""),                                   // LOCALE_SPOSITIVESIGN
        0x51 => t("-"),                                  // LOCALE_SNEGATIVESIGN
        0x52 => Some(N(3)),                              // LOCALE_IPOSSIGNPOSN
        0x53 => Some(N(1)),                              // LOCALE_INEGSIGNPOSN
        0x54 => Some(N(u32::from(l.curr_pos & 1 == 0))), // LOCALE_IPOSSYMPRECEDES
        0x55 => Some(N(u32::from(l.curr_pos >= 2))),     // LOCALE_IPOSSEPBYSPACE
        0x56 => Some(N(u32::from(l.curr_neg < 4))),      // LOCALE_INEGSYMPRECEDES
        0x57 => Some(N(u32::from(l.curr_neg >= 8))),     // LOCALE_INEGSEPBYSPACE
        0x59 => t(l.lang2),                              // LOCALE_SISO639LANGNAME
        0x5A => t(l.ctry2),                              // LOCALE_SISO3166CTRYNAME
        0x5B => Some(N(l.geo)),                          // LOCALE_IGEOID
        0x5C => t(l.name),                               // LOCALE_SNAME
        0x5D => t("h:mm:ss"),                            // LOCALE_SDURATION
        0x5E => t(""),                                   // LOCALE_SKEYBOARDSTOINSTALL
        0x60..=0x66 => t(l.shortest_days[(base - 0x60) as usize]),
        0x67 => t(l.lang3),        // LOCALE_SISO639LANGNAME2
        0x68 => t(l.ctry3),        // LOCALE_SISO3166CTRYNAME2
        0x69 => t("NaN"),          // LOCALE_SNAN
        0x6A => t(l.pos_infinity), // LOCALE_SPOSINFINITY
        0x6B => t(l.neg_infinity), // LOCALE_SNEGINFINITY
        0x6C => t(if l.name.is_empty() { "" } else { "Latn;" }), // LOCALE_SSCRIPTS
        0x6D => t(l.parent),       // LOCALE_SPARENT
        0x6E => t(l.name),         // LOCALE_SCONSOLEFALLBACKNAME
        0x70 => Some(N(0)),        // LOCALE_IREADINGLAYOUT (gauche à droite)
        0x71 => Some(N(u32::from(l.is_neutral()))), // LOCALE_INEUTRAL
        0x74 => Some(N(l.neg_percent)), // LOCALE_INEGATIVEPERCENT
        0x75 => Some(N(l.pos_percent)), // LOCALE_IPOSITIVEPERCENT
        0x76 => t("%"),            // LOCALE_SPERCENT
        0x77 => t("‰"),            // LOCALE_SPERMILLE
        0x78 => t(l.month_day),    // LOCALE_SMONTHDAY
        0x79 => t(l.short_times[0]), // LOCALE_SSHORTTIME
        0x7A => t(&l.lang3.to_ascii_lowercase()), // LOCALE_SOPENTYPELANGUAGETAG
        0x7B => t(l.sort_name),    // LOCALE_SSORTLOCALE
        0x1001 => t(l.eng_lang),   // LOCALE_SENGLISHLANGUAGENAME
        0x1002 => t(l.eng_ctry),   // LOCALE_SENGLISHCOUNTRYNAME
        0x1003 => t(l.long_times[0]), // LOCALE_STIMEFORMAT
        0x1004 => Some(N(1252)),   // LOCALE_IDEFAULTANSICODEPAGE
        0x1005 => Some(N(0)),      // LOCALE_ITIMEMARKPOSN
        0x1006 => t(l.year_month), // LOCALE_SYEARMONTH
        0x1007 => t(l.eng_curr),   // LOCALE_SENGCURRNAME
        0x1008 => t(l.nat_curr),   // LOCALE_SNATIVECURRNAME
        0x1009 => Some(N(CAL_GREGORIAN)), // LOCALE_ICALENDARTYPE
        0x100A => Some(N(l.paper)), // LOCALE_IPAPERSIZE
        0x100B => Some(N(0)),      // LOCALE_IOPTIONALCALENDAR
        0x100C => Some(N(l.first_day)), // LOCALE_IFIRSTDAYOFWEEK
        0x100D => Some(N(l.first_week)), // LOCALE_IFIRSTWEEKOFYEAR
        0x100E | 0x100F => t(""),  // LOCALE_SMONTHNAME13, SABBREVMONTHNAME13
        0x1010 => Some(N(1)),      // LOCALE_INEGNUMBER (« -1,1 »)
        0x1011 => Some(N(10000)),  // LOCALE_IDEFAULTMACCODEPAGE
        0x1012 => Some(N(20297)),  // LOCALE_IDEFAULTEBCDICCODEPAGE
        0x1013 => t("Default"),    // LOCALE_SSORTNAME
        0x1014 => Some(N(1)),      // LOCALE_IDIGITSUBSTITUTION
        _ => None,
    }
}

fn date_sep(fmt: &str) -> &'static str {
    if fmt.contains('.') {
        "."
    } else if fmt.contains('-') && !fmt.contains('/') {
        "-"
    } else {
        "/"
    }
}

/// Copie une chaîne UTF-16 terminée par zéro ; `cap == 0` demande la taille.
unsafe fn put_text(s: &str, buf: *mut u16, cap: i32) -> i32 {
    let need = s.encode_utf16().count() + 1;
    if cap == 0 {
        return need as i32;
    }
    if buf.is_null() || cap < 0 || (cap as usize) < need {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    crate::dlls::put_w(s, buf, cap as u32);
    need as i32
}

/// LOCALE_RETURN_NUMBER : la valeur est écrite comme un DWORD (2 caractères).
unsafe fn put_number(v: u32, buf: *mut u16, cap: i32) -> i32 {
    if cap == 0 {
        return 2;
    }
    if buf.is_null() || cap < 2 {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    (buf as *mut u32).write_unaligned(v);
    2
}

unsafe fn locale_info(l: &Locale, lc_type: u32, buf: *mut u16, cap: i32) -> i32 {
    let Some(v) = info(l, lc_type) else {
        set_last_error(ERROR_INVALID_FLAGS);
        return 0;
    };
    match (v, lc_type & LOCALE_RETURN_NUMBER != 0) {
        (Info::Number(n), true) => put_number(n, buf, cap),
        (Info::Number(n), false) => put_text(&n.to_string(), buf, cap),
        (Info::Text(s), true) => match s.parse::<u32>() {
            Ok(n) => put_number(n, buf, cap),
            Err(_) => {
                set_last_error(ERROR_INVALID_FLAGS);
                0
            }
        },
        (Info::Text(s), false) => put_text(&s, buf, cap),
    }
}

unsafe fn opt_name(name: *const u16) -> Option<String> {
    (!name.is_null()).then(|| crate::dlls::wstr(name))
}

pub unsafe extern "win64" fn get_locale_info_ex(
    name: *const u16,
    lc_type: u32,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let text = opt_name(name);
    let Some(l) = by_name(text.as_deref()) else {
        crate::trace!("GetLocaleInfoEx({text:?}) : locale inconnue");
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let r = locale_info(l, lc_type, buf, cap);
    if r == 0 {
        crate::trace!("GetLocaleInfoEx({:?}, 0x{lc_type:x}) : échec", l.name);
    }
    r
}

pub unsafe extern "win64" fn get_locale_info_w(
    lcid: u32,
    lc_type: u32,
    buf: *mut u16,
    cap: i32,
) -> i32 {
    let Some(l) = by_lcid(lcid) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    locale_info(l, lc_type, buf, cap)
}

/// GetLocaleInfoA : la même valeur, convertie dans la page de code ANSI.
pub unsafe extern "win64" fn get_locale_info_a(
    lcid: u32,
    lc_type: u32,
    buf: *mut u8,
    cap: i32,
) -> i32 {
    let Some(l) = by_lcid(lcid) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let Some(v) = info(l, lc_type) else {
        set_last_error(ERROR_INVALID_FLAGS);
        return 0;
    };
    if lc_type & LOCALE_RETURN_NUMBER != 0 {
        let n = match v {
            Info::Number(n) => n,
            Info::Text(s) => match s.parse() {
                Ok(n) => n,
                Err(_) => {
                    set_last_error(ERROR_INVALID_FLAGS);
                    return 0;
                }
            },
        };
        if cap == 0 {
            return 4;
        }
        if buf.is_null() || cap < 4 {
            set_last_error(ERROR_INSUFFICIENT_BUFFER);
            return 0;
        }
        (buf as *mut u32).write_unaligned(n);
        return 4;
    }
    let s = match v {
        Info::Number(n) => n.to_string(),
        Info::Text(s) => s,
    };
    let bytes = crate::dlls::encode_ansi(&s);
    let need = bytes.len() + 1;
    if cap == 0 {
        return need as i32;
    }
    if buf.is_null() || cap < 0 || (cap as usize) < need {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, bytes.len());
    *buf.add(bytes.len()) = 0;
    need as i32
}

pub unsafe extern "win64" fn get_user_default_locale_name(buf: *mut u16, cap: i32) -> i32 {
    put_text(USER_LOCALE, buf, cap)
}

pub unsafe extern "win64" fn locale_name_to_lcid(name: *const u16, _flags: u32) -> u32 {
    by_name(opt_name(name).as_deref()).map_or(0, |l| l.lcid)
}

pub unsafe extern "win64" fn lcid_to_locale_name(
    lcid: u32,
    buf: *mut u16,
    cap: i32,
    _flags: u32,
) -> i32 {
    match by_lcid(lcid) {
        Some(l) => put_text(l.name, buf, cap),
        None => {
            set_last_error(ERROR_INVALID_PARAMETER);
            0
        }
    }
}

pub unsafe extern "win64" fn is_valid_locale_name(name: *const u16) -> i32 {
    i32::from(!name.is_null() && by_name(opt_name(name).as_deref()).is_some())
}

pub extern "win64" fn is_valid_locale(lcid: u32, _flags: u32) -> i32 {
    i32::from(by_lcid(lcid).is_some())
}

/// ResolveLocaleName : nom de la locale spécifique la plus proche
/// (« fr » → « fr-FR », « fr-CA » → « fr-FR »), 0 si la langue est inconnue.
pub unsafe extern "win64" fn resolve_locale_name(name: *const u16, buf: *mut u16, cap: i32) -> i32 {
    let text = opt_name(name);
    let found = by_name(text.as_deref()).or_else(|| {
        let lang = text.as_deref()?.split(['-', '_']).next()?.to_string();
        by_name(Some(&lang))
    });
    match found {
        Some(l) => put_text(l.specific, buf, cap),
        None => 0,
    }
}

type LocaleEnumProc = unsafe extern "win64" fn(*const u16, u32, isize) -> i32;

/// EnumSystemLocalesEx : LOCALE_NEUTRALDATA (0x10) et LOCALE_SPECIFICDATA
/// (0x20) filtrent ; 0 ou LOCALE_WINDOWS (1) donnent tout.
pub unsafe extern "win64" fn enum_system_locales_ex(
    proc_: Option<LocaleEnumProc>,
    flags: u32,
    param: isize,
    _reserved: *mut c_void,
) -> i32 {
    let Some(f) = proc_ else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    for l in LOCALES.iter().filter(|l| !l.name.is_empty()) {
        let kind = if l.is_neutral() { 0x10 } else { 0x20 };
        if flags & 0x30 != 0 && flags & kind == 0 {
            continue;
        }
        let w: Vec<u16> = l.name.encode_utf16().chain(Some(0)).collect();
        if f(w.as_ptr(), 1 | kind, param) == 0 {
            break;
        }
    }
    1
}

pub extern "win64" fn get_user_geo_id(_class: u32) -> u32 {
    by_lcid(USER_LCID).map_or(244, |l| l.geo)
}

// --- Calendriers -----------------------------------------------------------------------

fn calendar_info(l: &Locale, cal_type: u32) -> Option<Info> {
    use Info::{Number as N, Text as T};
    let t = |s: &str| Some(T(s.to_string()));
    let genitive = cal_type & 0x1000_0000 != 0;
    let base = cal_type & 0xFFFF;
    match base {
        0x01 => Some(N(CAL_GREGORIAN)), // CAL_ICALINTVALUE
        0x02 => t(l.calendar_name),     // CAL_SCALNAME
        0x03 => t("0"),                 // CAL_IYEAROFFSETRANGE
        0x04 => t(l.era),               // CAL_SERASTRING
        0x05 => t(l.short_dates[0]),    // CAL_SSHORTDATE
        0x06 => t(l.long_dates[0]),     // CAL_SLONGDATE
        0x07..=0x0D => t(l.days[(base - 0x07) as usize]),
        0x0E..=0x14 => t(l.abbrev_days[(base - 0x0E) as usize]),
        0x15..=0x20 => {
            let i = (base - 0x15) as usize;
            t(if genitive {
                l.genitive_months.unwrap_or(l.months)[i]
            } else {
                l.months[i]
            })
        }
        0x21 | 0x2E => t(""), // CAL_SMONTHNAME13, CAL_SABBREVMONTHNAME13
        0x22..=0x2D => t(l.abbrev_months[(base - 0x22) as usize]),
        0x2F => t(l.year_month), // CAL_SYEARMONTH
        0x30 => Some(N(2049)),   // CAL_ITWODIGITYEARMAX
        0x31..=0x37 => t(l.shortest_days[(base - 0x31) as usize]),
        0x38 => t(l.month_day),     // CAL_SMONTHDAY
        0x39 => t(l.abbrev_era),    // CAL_SABBREVERASTRING
        0x3A => t(l.long_dates[0]), // CAL_SRELATIVELONGDATE
        0x3B => t("A.D."),          // CAL_SENGLISHERANAME
        0x3C => t("AD"),            // CAL_SENGLISHABBREVERANAME
        _ => None,
    }
}

/// GetCalendarInfoEx : seul le calendrier grégorien (1) est décrit.
pub unsafe extern "win64" fn get_calendar_info_ex(
    name: *const u16,
    calendar: u32,
    _reserved: *const u16,
    cal_type: u32,
    buf: *mut u16,
    cap: i32,
    value: *mut u32,
) -> i32 {
    let Some(l) = by_name(opt_name(name).as_deref()) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    calendar_value(l, calendar, cal_type, buf, cap, value)
}

pub unsafe extern "win64" fn get_calendar_info_w(
    lcid: u32,
    calendar: u32,
    cal_type: u32,
    buf: *mut u16,
    cap: i32,
    value: *mut u32,
) -> i32 {
    let Some(l) = by_lcid(lcid) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    calendar_value(l, calendar, cal_type, buf, cap, value)
}

unsafe fn calendar_value(
    l: &Locale,
    calendar: u32,
    cal_type: u32,
    buf: *mut u16,
    cap: i32,
    value: *mut u32,
) -> i32 {
    if calendar != CAL_GREGORIAN {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let Some(v) = calendar_info(l, cal_type) else {
        set_last_error(ERROR_INVALID_FLAGS);
        return 0;
    };
    if cal_type & CAL_RETURN_NUMBER != 0 {
        let n = match v {
            Info::Number(n) => n,
            Info::Text(s) => match s.parse() {
                Ok(n) => n,
                Err(_) => {
                    set_last_error(ERROR_INVALID_FLAGS);
                    return 0;
                }
            },
        };
        if value.is_null() {
            set_last_error(ERROR_INVALID_PARAMETER);
            return 0;
        }
        *value = n;
        return 2;
    }
    match v {
        Info::Number(n) => put_text(&n.to_string(), buf, cap),
        Info::Text(s) => put_text(&s, buf, cap),
    }
}

type CalEnumProcExEx = unsafe extern "win64" fn(*const u16, u32, *const u16, isize) -> i32;

/// Valeurs d'un CALTYPE à énumérer : plusieurs formats possibles pour les dates.
fn calendar_values(l: &Locale, cal_type: u32) -> Vec<String> {
    match cal_type & 0xFFFF {
        0x05 => l.short_dates.iter().map(|s| s.to_string()).collect(),
        0x06 => l.long_dates.iter().map(|s| s.to_string()).collect(),
        _ => match calendar_info(l, cal_type) {
            Some(Info::Text(s)) => vec![s],
            Some(Info::Number(n)) => vec![n.to_string()],
            None => Vec::new(),
        },
    }
}

pub unsafe extern "win64" fn enum_calendar_info_ex_ex(
    proc_: Option<CalEnumProcExEx>,
    name: *const u16,
    calendar: u32,
    _reserved: *const u16,
    cal_type: u32,
    param: isize,
) -> i32 {
    let (Some(f), Some(l)) = (proc_, by_name(opt_name(name).as_deref())) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    if calendar != CAL_GREGORIAN && calendar != ENUM_ALL_CALENDARS {
        return 1;
    }
    for v in calendar_values(l, cal_type) {
        let w: Vec<u16> = v.encode_utf16().chain(Some(0)).collect();
        if f(w.as_ptr(), CAL_GREGORIAN, std::ptr::null(), param) == 0 {
            break;
        }
    }
    1
}

type DateEnumProcExEx = unsafe extern "win64" fn(*const u16, u32, isize) -> i32;

/// EnumDateFormatsExEx : DATE_SHORTDATE (1), DATE_LONGDATE (2),
/// DATE_YEARMONTH (8), DATE_MONTHDAY (0x80).
pub unsafe extern "win64" fn enum_date_formats_ex_ex(
    proc_: Option<DateEnumProcExEx>,
    name: *const u16,
    flags: u32,
    param: isize,
) -> i32 {
    let (Some(f), Some(l)) = (proc_, by_name(opt_name(name).as_deref())) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let formats: &[&str] = match flags & 0xFF {
        1 => l.short_dates,
        2 => l.long_dates,
        8 => std::slice::from_ref(&l.year_month),
        0x80 => std::slice::from_ref(&l.month_day),
        _ => {
            set_last_error(ERROR_INVALID_FLAGS);
            return 0;
        }
    };
    for s in formats {
        let w: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
        if f(w.as_ptr(), CAL_GREGORIAN, param) == 0 {
            break;
        }
    }
    1
}

type TimeEnumProcEx = unsafe extern "win64" fn(*const u16, isize) -> i32;

/// EnumTimeFormatsEx : TIME_NOSECONDS (2) donne les formats courts.
pub unsafe extern "win64" fn enum_time_formats_ex(
    proc_: Option<TimeEnumProcEx>,
    name: *const u16,
    flags: u32,
    param: isize,
) -> i32 {
    let (Some(f), Some(l)) = (proc_, by_name(opt_name(name).as_deref())) else {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    };
    let formats = if flags & 2 != 0 {
        l.short_times
    } else {
        l.long_times
    };
    for s in formats {
        let w: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
        if f(w.as_ptr(), param) == 0 {
            break;
        }
    }
    1
}

// --- Comparaison et recherche -----------------------------------------------------------

const NORM_IGNORECASE: u32 = 0x1;
const NORM_IGNORENONSPACE: u32 = 0x2;
const LINGUISTIC_IGNORECASE: u32 = 0x10;
const LINGUISTIC_IGNOREDIACRITIC: u32 = 0x20;
const NORM_IGNOREWIDTH: u32 = 0x2_0000;

unsafe fn slice_of(p: *const u16, n: i32) -> &'static [u16] {
    if p.is_null() {
        return &[];
    }
    let n = if n < 0 {
        crate::printf::wide_len(p)
    } else {
        n as usize
    };
    std::slice::from_raw_parts(p, n)
}

/// Forme de comparaison d'un caractère selon les options NORM_*.
fn fold(c: char, flags: u32) -> String {
    let mut s = c.to_string();
    if flags & (NORM_IGNORENONSPACE | LINGUISTIC_IGNOREDIACRITIC) != 0 {
        s = strip_accent(c).to_string();
    }
    if flags & NORM_IGNOREWIDTH != 0 {
        s = s
            .chars()
            .map(|c| match c as u32 {
                0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
                0x3000 => ' ',
                _ => c,
            })
            .collect();
    }
    if flags & (NORM_IGNORECASE | LINGUISTIC_IGNORECASE) != 0 {
        s = s.to_lowercase();
    }
    s
}

/// Lettre latine sans son diacritique (é → e, ß inchangé).
fn strip_accent(c: char) -> char {
    const FROM: &str = "ÀÁÂÃÄÅàáâãäåÇçÈÉÊËèéêëÌÍÎÏìíîïÑñÒÓÔÕÖòóôõöÙÚÛÜùúûüÝýÿ";
    const TO: &str = "AAAAAAaaaaaaCcEEEEeeeeIIIIiiiiNnOOOOOoooooUUUUuuuuYyy";
    FROM.chars()
        .position(|f| f == c)
        .and_then(|i| TO.chars().nth(i))
        .unwrap_or(c)
}

/// Clé de tri : lettres sans accent d'abord, puis accents, puis casse
/// (minuscule avant majuscule), comme l'ordre linguistique de Windows.
fn sort_key(s: &[u16], flags: u32) -> (Vec<String>, Vec<char>, Vec<bool>) {
    let text = String::from_utf16_lossy(s);
    let mut primary = Vec::new();
    let mut accents = Vec::new();
    let mut cases = Vec::new();
    for c in text.chars() {
        primary.push(fold(strip_accent(c), flags | NORM_IGNORECASE));
        accents.push(c.to_lowercase().next().unwrap_or(c));
        cases.push(c.is_uppercase() && flags & (NORM_IGNORECASE | LINGUISTIC_IGNORECASE) == 0);
    }
    if flags & (NORM_IGNORENONSPACE | LINGUISTIC_IGNOREDIACRITIC) != 0 {
        accents.clear();
    }
    (primary, accents, cases)
}

pub fn compare(a: &[u16], b: &[u16], flags: u32) -> std::cmp::Ordering {
    sort_key(a, flags).cmp(&sort_key(b, flags))
}

pub unsafe extern "win64" fn compare_string_ex(
    name: *const u16,
    flags: u32,
    s1: *const u16,
    n1: i32,
    s2: *const u16,
    n2: i32,
    _version: *mut c_void,
    _reserved: *mut c_void,
    _param: isize,
) -> i32 {
    if s1.is_null() || s2.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    if by_name(opt_name(name).as_deref()).is_none() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let (a, b) = (slice_of(s1, n1), slice_of(s2, n2));
    match compare(a, b, flags) {
        std::cmp::Ordering::Less => 1,
        std::cmp::Ordering::Equal => 2,
        std::cmp::Ordering::Greater => 3,
    }
}

const FIND_STARTSWITH: u32 = 0x0010_0000;
const FIND_ENDSWITH: u32 = 0x0020_0000;
const FIND_FROMEND: u32 = 0x0080_0000;

/// FindNLSStringEx : position de `value` dans `source` (en caractères), -1
/// sinon ; `found` reçoit la longueur trouvée.
pub unsafe extern "win64" fn find_nls_string_ex(
    _name: *const u16,
    flags: u32,
    source: *const u16,
    n_source: i32,
    value: *const u16,
    n_value: i32,
    found: *mut i32,
    _version: *mut c_void,
    _reserved: *mut c_void,
    _sort: isize,
) -> i32 {
    if source.is_null() || value.is_null() {
        set_last_error(ERROR_INVALID_PARAMETER);
        return -1;
    }
    let (src, val) = (slice_of(source, n_source), slice_of(value, n_value));
    let key = |s: &[u16]| -> Vec<String> {
        String::from_utf16_lossy(s)
            .chars()
            .map(|c| fold(c, flags))
            .collect()
    };
    // Recherche sur des unités UTF-16 (les textes .NET sont en BMP le plus souvent).
    let fold_units = |s: &[u16]| -> Vec<String> {
        s.iter()
            .map(|&u| char::from_u32(u as u32).map_or(String::from("\u{FFFD}"), |c| fold(c, flags)))
            .collect()
    };
    let (hay, needle) = if src.iter().all(|&u| !(0xD800..0xE000).contains(&u)) {
        (fold_units(src), fold_units(val))
    } else {
        (key(src), key(val))
    };
    let n = needle.len();
    let candidates: Vec<usize> = if n > hay.len() {
        Vec::new()
    } else if flags & FIND_STARTSWITH != 0 {
        vec![0]
    } else if flags & FIND_ENDSWITH != 0 {
        vec![hay.len() - n]
    } else if flags & FIND_FROMEND != 0 {
        (0..=hay.len() - n).rev().collect()
    } else {
        (0..=hay.len() - n).collect()
    };
    for i in candidates {
        if hay[i..i + n] == needle[..] {
            if !found.is_null() {
                *found = n as i32;
            }
            return i as i32;
        }
    }
    -1
}

/// GetNLSVersionEx : NLSVERSIONINFOEX (32 octets) avec une version fixe.
pub unsafe extern "win64" fn get_nls_version_ex(
    _function: u32,
    _name: *const u16,
    info: *mut u32,
) -> i32 {
    if info.is_null() || *info < 32 {
        set_last_error(ERROR_INSUFFICIENT_BUFFER);
        return 0;
    }
    *info.add(1) = 0x0006_0000; // dwNLSVersion
    *info.add(2) = 0x0006_0000; // dwDefinedVersion
    *info.add(3) = 0; // dwEffectiveId
    std::ptr::write_bytes(info.add(4), 0, 4); // guidCustomVersion
    1
}

/// IsNLSDefinedString : tout caractère attribué (hors moitiés de paire
/// isolées et non-caractères) est défini.
pub unsafe extern "win64" fn is_nls_defined_string(
    _function: u32,
    _flags: u32,
    _version: *mut c_void,
    s: *const u16,
    n: i32,
) -> i32 {
    let units = slice_of(s, n);
    i32::from(
        char::decode_utf16(units.iter().copied())
            .all(|c| c.is_ok_and(|c| !matches!(c as u32, 0xFFFE | 0xFFFF | 0xFDD0..=0xFDEF))),
    )
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "GetLocaleInfoA" => fn get_locale_info_a,
        "GetLocaleInfoW" => fn get_locale_info_w,
        "GetLocaleInfoEx" => fn get_locale_info_ex,
        "GetUserDefaultLocaleName" => fn get_user_default_locale_name,
        "GetSystemDefaultLocaleName" => fn get_user_default_locale_name,
        "LocaleNameToLCID" => fn locale_name_to_lcid,
        "LCIDToLocaleName" => fn lcid_to_locale_name,
        "IsValidLocaleName" => fn is_valid_locale_name,
        "IsValidLocale" => fn is_valid_locale,
        "ResolveLocaleName" => fn resolve_locale_name,
        "EnumSystemLocalesEx" => fn enum_system_locales_ex,
        "GetUserGeoID" => fn get_user_geo_id,
        "GetCalendarInfoEx" => fn get_calendar_info_ex,
        "GetCalendarInfoW" => fn get_calendar_info_w,
        "EnumCalendarInfoExEx" => fn enum_calendar_info_ex_ex,
        "EnumDateFormatsExEx" => fn enum_date_formats_ex_ex,
        "EnumTimeFormatsEx" => fn enum_time_formats_ex,
        "CompareStringEx" => fn compare_string_ex,
        "FindNLSStringEx" => fn find_nls_string_ex,
        "GetNLSVersionEx" => fn get_nls_version_ex,
        "IsNLSDefinedString" => fn is_nls_defined_string,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    unsafe fn text(name: &str, lc_type: u32) -> String {
        let n = w(name);
        let mut buf = [0u16; 128];
        let r = get_locale_info_ex(n.as_ptr(), lc_type, buf.as_mut_ptr(), 128);
        assert!(r > 0, "{name} 0x{lc_type:x}");
        String::from_utf16_lossy(&buf[..r as usize - 1])
    }

    unsafe fn number(name: &str, lc_type: u32) -> u32 {
        let n = w(name);
        let mut buf = [0u16; 2];
        assert_eq!(
            get_locale_info_ex(
                n.as_ptr(),
                lc_type | LOCALE_RETURN_NUMBER,
                buf.as_mut_ptr(),
                2
            ),
            2
        );
        (buf.as_ptr() as *const u32).read_unaligned()
    }

    #[test]
    fn decrit_les_locales_de_l_interface() {
        unsafe {
            assert_eq!(text("fr-FR", 0x5C), "fr-FR");
            assert_eq!(text("FR_fr", 0x6D), "fr");
            assert_eq!(number("fr", 0x71), 1);
            assert_eq!(number("fr-FR", 0x71), 0);
            assert_eq!(text("fr", 0x6D), "");
            assert_eq!(text("de-DE", 0x2A), "Montag");
            assert_eq!(text("es-ES", 0x4F), "dic.");
            assert_eq!(text("en-US", 0x1F), "M/d/yyyy");
            assert_eq!(text("en-US", 0x72), "English (United States)");
            assert_eq!(text("fr-FR", 0x73), "français (France)");
            assert_eq!(number("en-US", 0x100C), 6);
            assert_eq!(number("de-DE", 0x100C), 0);
            assert_eq!(number("en-US", 0x01), 0x0409);
            assert_eq!(text("", 0x15), "XDR");
            assert_eq!(text("", 0x6A), "Infinity");
            // Taille demandée, tampon trop petit, LCTYPE inconnu, locale inconnue.
            let n = w("fr-FR");
            assert_eq!(
                get_locale_info_ex(n.as_ptr(), 0x14, std::ptr::null_mut(), 0),
                2
            );
            let mut one = [0u16; 1];
            assert_eq!(get_locale_info_ex(n.as_ptr(), 0x5C, one.as_mut_ptr(), 1), 0);
            assert_eq!(
                get_locale_info_ex(n.as_ptr(), 0x7777, one.as_mut_ptr(), 1),
                0
            );
            let x = w("xx-YY");
            assert_eq!(get_locale_info_ex(x.as_ptr(), 0x5C, one.as_mut_ptr(), 1), 0);
            // Locale de l'utilisateur et API par LCID.
            let mut buf = [0u16; 16];
            assert_eq!(
                get_locale_info_ex(std::ptr::null(), 0x5C, buf.as_mut_ptr(), 16),
                6
            );
            assert_eq!(get_locale_info_w(0x0400, 0x0E, buf.as_mut_ptr(), 16), 2);
            assert_eq!(buf[0], b',' as u16);
            let mut a = [0u8; 16];
            assert_eq!(get_locale_info_a(0x0409, 0x14, a.as_mut_ptr(), 16), 2);
            assert_eq!(&a[..2], b"$\0");
        }
    }

    #[test]
    fn noms_et_identifiants() {
        unsafe {
            assert_eq!(locale_name_to_lcid(w("de-de").as_ptr(), 0), 0x0407);
            let mut buf = [0u16; 16];
            assert_eq!(lcid_to_locale_name(0x0C0A, buf.as_mut_ptr(), 16, 0), 6);
            assert_eq!(String::from_utf16_lossy(&buf[..5]), "es-ES");
            assert_eq!(
                resolve_locale_name(w("fr").as_ptr(), buf.as_mut_ptr(), 16),
                6
            );
            assert_eq!(String::from_utf16_lossy(&buf[..5]), "fr-FR");
            assert_eq!(
                resolve_locale_name(w("en-GB").as_ptr(), buf.as_mut_ptr(), 16),
                6
            );
            assert_eq!(String::from_utf16_lossy(&buf[..5]), "en-US");
            assert_eq!(
                resolve_locale_name(w("ja-JP").as_ptr(), buf.as_mut_ptr(), 16),
                0
            );
            assert_eq!(is_valid_locale_name(w("es").as_ptr()), 1);
            assert_eq!(is_valid_locale_name(w("xx").as_ptr()), 0);

            unsafe extern "win64" fn collect(name: *const u16, flags: u32, out: isize) -> i32 {
                let v = &mut *(out as *mut Vec<(String, u32)>);
                v.push((crate::dlls::wstr(name), flags));
                1
            }
            let mut all: Vec<(String, u32)> = Vec::new();
            assert_eq!(
                enum_system_locales_ex(
                    Some(collect),
                    0x20,
                    &mut all as *mut _ as isize,
                    std::ptr::null_mut()
                ),
                1
            );
            assert_eq!(all.len(), 4);
            assert!(all.iter().all(|(n, f)| n.contains('-') && f & 0x20 != 0));
        }
    }

    #[test]
    fn calendrier_gregorien() {
        unsafe {
            let fr = w("fr-FR");
            let mut buf = [0u16; 64];
            let mut v = 0u32;
            let r = get_calendar_info_ex(
                fr.as_ptr(),
                1,
                std::ptr::null(),
                0x15,
                buf.as_mut_ptr(),
                64,
                &mut v,
            );
            assert_eq!(String::from_utf16_lossy(&buf[..r as usize - 1]), "janvier");
            assert_eq!(
                get_calendar_info_ex(
                    fr.as_ptr(),
                    1,
                    std::ptr::null(),
                    0x30 | CAL_RETURN_NUMBER,
                    std::ptr::null_mut(),
                    0,
                    &mut v
                ),
                2
            );
            assert_eq!(v, 2049);
            assert_eq!(
                get_calendar_info_ex(
                    fr.as_ptr(),
                    3,
                    std::ptr::null(),
                    0x15,
                    buf.as_mut_ptr(),
                    64,
                    &mut v
                ),
                0
            );

            unsafe extern "win64" fn cal(
                s: *const u16,
                id: u32,
                _r: *const u16,
                out: isize,
            ) -> i32 {
                (*(out as *mut Vec<(String, u32)>)).push((crate::dlls::wstr(s), id));
                1
            }
            let mut got: Vec<(String, u32)> = Vec::new();
            enum_calendar_info_ex_ex(
                Some(cal),
                fr.as_ptr(),
                ENUM_ALL_CALENDARS,
                std::ptr::null(),
                1,
                &mut got as *mut _ as isize,
            );
            assert_eq!(got, vec![("1".to_string(), 1)]);
            got.clear();
            enum_calendar_info_ex_ex(
                Some(cal),
                fr.as_ptr(),
                1,
                std::ptr::null(),
                5,
                &mut got as *mut _ as isize,
            );
            assert_eq!(got[0].0, "dd/MM/yyyy");
            assert!(got.len() > 1);

            unsafe extern "win64" fn time(s: *const u16, out: isize) -> i32 {
                (*(out as *mut Vec<String>)).push(crate::dlls::wstr(s));
                1
            }
            let mut times: Vec<String> = Vec::new();
            enum_time_formats_ex(
                Some(time),
                w("en-US").as_ptr(),
                2,
                &mut times as *mut _ as isize,
            );
            assert_eq!(times[0], "h:mm tt");

            unsafe extern "win64" fn date(s: *const u16, _id: u32, out: isize) -> i32 {
                (*(out as *mut Vec<String>)).push(crate::dlls::wstr(s));
                0 // arrête après le premier
            }
            let mut dates: Vec<String> = Vec::new();
            enum_date_formats_ex_ex(
                Some(date),
                w("de-DE").as_ptr(),
                2,
                &mut dates as *mut _ as isize,
            );
            assert_eq!(dates, vec!["dddd, d. MMMM yyyy".to_string()]);
        }
    }

    #[test]
    fn comparaison_et_recherche() {
        let s = |t: &str| t.encode_utf16().collect::<Vec<u16>>();
        use std::cmp::Ordering::*;
        assert_eq!(
            compare(&s("été"), &s("ETE"), NORM_IGNORECASE | NORM_IGNORENONSPACE),
            Equal
        );
        assert_eq!(compare(&s("ete"), &s("été"), 0), Less);
        assert_eq!(compare(&s("été"), &s("etre"), 0), Less); // l'accent compte après les lettres
        assert_eq!(compare(&s("a"), &s("A"), 0), Less);
        assert_eq!(compare(&s("B"), &s("a"), 0), Greater);
        unsafe {
            let fr = w("fr-FR");
            let (a, b) = (s("Paint"), s("paint"));
            assert_eq!(
                compare_string_ex(
                    fr.as_ptr(),
                    1,
                    a.as_ptr(),
                    5,
                    b.as_ptr(),
                    5,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0
                ),
                2
            );
            let hay = s("Image Éditée.png");
            let mut found = 0;
            let needle = s("éditée");
            assert_eq!(
                find_nls_string_ex(
                    fr.as_ptr(),
                    1,
                    hay.as_ptr(),
                    hay.len() as i32,
                    needle.as_ptr(),
                    6,
                    &mut found,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0
                ),
                6
            );
            assert_eq!(found, 6);
            let ext = s(".PNG");
            assert_eq!(
                find_nls_string_ex(
                    fr.as_ptr(),
                    1 | FIND_ENDSWITH,
                    hay.as_ptr(),
                    hay.len() as i32,
                    ext.as_ptr(),
                    4,
                    &mut found,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0
                ),
                12
            );
            assert_eq!(
                find_nls_string_ex(
                    fr.as_ptr(),
                    FIND_STARTSWITH,
                    hay.as_ptr(),
                    hay.len() as i32,
                    ext.as_ptr(),
                    4,
                    &mut found,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0
                ),
                -1
            );
            let ok = s("abc");
            assert_eq!(
                is_nls_defined_string(1, 0, std::ptr::null_mut(), ok.as_ptr(), 3),
                1
            );
            let bad = [0xD800u16];
            assert_eq!(
                is_nls_defined_string(1, 0, std::ptr::null_mut(), bad.as_ptr(), 1),
                0
            );
        }
    }
}
