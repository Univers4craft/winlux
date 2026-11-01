//! Heure et fuseau horaire : les .exe lisent le même fuseau que Linux
//! (`TZ`, /etc/localtime réglé par Paramètres ou l'installateur) et l'horloge
//! système UTC. Le fichier TZif est lu de zéro ; sa règle POSIX finale
//! (« CET-1CEST,M3.5.0,M10.5.0/3 ») donne la structure
//! TIME_ZONE_INFORMATION de Windows.

use std::sync::OnceLock;

/// 100 ns entre le 1er janvier 1601 (FILETIME) et le 1er janvier 1970.
pub const FILETIME_UNIX_EPOCH: i64 = 116_444_736_000_000_000;
pub const TICKS_PER_SECOND: i64 = 10_000_000;
pub const TICKS_PER_MINUTE: i64 = 60 * TICKS_PER_SECOND;

/// SYSTEMTIME (huit mots de 16 bits).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SystemTime {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub millis: u16,
}

/// Règle d'un changement d'heure : mois, n-ième jour de semaine (5 =
/// dernier), heure locale. Même codage que les SYSTEMTIME de Windows
/// (`year` = 0, `day` = semaine).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rule {
    pub month: u16,
    pub week: u16,
    pub weekday: u16,
    /// Secondes après minuit (peut dépasser 24 h en POSIX).
    pub seconds: i32,
}

/// Fuseau horaire au sens de Windows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Zone {
    /// Nom IANA (« Europe/Paris »), vide s'il est inconnu.
    pub iana: String,
    /// UTC = heure locale + biais, en minutes (Paris : -60).
    pub bias: i32,
    pub std_abbr: String,
    pub dst_abbr: String,
    /// Biais ajouté en heure d'été (-60 en général).
    pub dst_bias: i32,
    /// Passage à l'heure d'été, puis retour à l'heure normale.
    pub dst_start: Option<Rule>,
    pub dst_end: Option<Rule>,
}

// ---------------------------------------------------------------------
// Calendrier
// ---------------------------------------------------------------------

/// Jours depuis le 1er janvier 1970 (calendrier grégorien proleptique).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Date civile d'un nombre de jours depuis 1970.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

/// SYSTEMTIME → FILETIME (100 ns depuis 1601). `None` si un champ est
/// invalide (Windows renvoie alors ERROR_INVALID_PARAMETER).
pub fn system_to_filetime(st: &SystemTime) -> Option<i64> {
    if !(1601..=30827).contains(&st.year)
        || !(1..=12).contains(&st.month)
        || st.day == 0
        || st.day as u32 > days_in_month(st.year as i64, st.month as u32)
        || st.hour > 23
        || st.minute > 59
        || st.second > 59
        || st.millis > 999
    {
        return None;
    }
    let days = days_from_civil(st.year as i64, st.month as u32, st.day as u32);
    let secs = days * 86_400 + st.hour as i64 * 3600 + st.minute as i64 * 60 + st.second as i64;
    Some(FILETIME_UNIX_EPOCH + secs * TICKS_PER_SECOND + st.millis as i64 * 10_000)
}

/// FILETIME → SYSTEMTIME ; `None` au-delà de 0x7FFF_FFFF_FFFF_FFFF.
pub fn filetime_to_system(ft: i64) -> Option<SystemTime> {
    if ft < 0 {
        return None;
    }
    let rel = ft - FILETIME_UNIX_EPOCH;
    let secs = rel.div_euclid(TICKS_PER_SECOND);
    let millis = (rel.rem_euclid(TICKS_PER_SECOND) / 10_000) as u16;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    Some(SystemTime {
        year: y as u16,
        month: m as u16,
        // 1er janvier 1970 : jeudi (4).
        day_of_week: (days + 4).rem_euclid(7) as u16,
        day: d as u16,
        hour: (tod / 3600) as u16,
        minute: (tod % 3600 / 60) as u16,
        second: (tod % 60) as u16,
        millis,
    })
}

/// Secondes locales (depuis 1970, sans fuseau) du changement d'heure de
/// l'année `year`.
fn rule_local_seconds(rule: &Rule, year: i64) -> i64 {
    let month = rule.month as u32;
    let first = days_from_civil(year, month, 1);
    let first_wd = (first + 4).rem_euclid(7);
    let mut day = 1 + (rule.weekday as i64 - first_wd).rem_euclid(7);
    day += 7 * (rule.week.clamp(1, 5) as i64 - 1);
    while day > days_in_month(year, month) as i64 {
        day -= 7;
    }
    (first + day - 1) * 86_400 + rule.seconds as i64
}

impl Zone {
    pub fn utc() -> Zone {
        Zone {
            iana: "Etc/UTC".into(),
            std_abbr: "UTC".into(),
            dst_abbr: "UTC".into(),
            ..Zone::default()
        }
    }

    pub fn has_dst(&self) -> bool {
        self.dst_start.is_some() && self.dst_end.is_some()
    }

    /// L'instant UTC (secondes depuis 1970) est-il en heure d'été ?
    /// Algorithme de Windows : changements évalués dans l'année locale.
    pub fn is_dst_utc(&self, utc: i64) -> bool {
        let (Some(start), Some(end)) = (self.dst_start, self.dst_end) else {
            return false;
        };
        let local_std = utc - self.bias as i64 * 60;
        let (year, _, _) = civil_from_days(local_std.div_euclid(86_400));
        // Début exprimé en heure normale, fin en heure d'été.
        let start_utc = rule_local_seconds(&start, year) + self.bias as i64 * 60;
        let end_utc = rule_local_seconds(&end, year) + (self.bias + self.dst_bias) as i64 * 60;
        if start_utc < end_utc {
            utc >= start_utc && utc < end_utc
        } else {
            !(utc >= end_utc && utc < start_utc)
        }
    }

    /// Biais total (minutes) en vigueur à l'instant UTC donné.
    pub fn bias_at_utc(&self, utc: i64) -> i32 {
        self.bias
            + if self.is_dst_utc(utc) {
                self.dst_bias
            } else {
                0
            }
    }

    /// Heure locale (secondes « murales » depuis 1970) → UTC. Une heure
    /// ambiguë (retour à l'heure normale) est lue en heure d'été.
    pub fn local_to_utc(&self, local: i64) -> i64 {
        let std = local + self.bias as i64 * 60;
        let dst = std + self.dst_bias as i64 * 60;
        if self.is_dst_utc(dst) {
            dst
        } else {
            std
        }
    }

    /// Nom de clé Windows (« Romance Standard Time »).
    pub fn windows_key(&self) -> String {
        windows_zone_name(&self.iana)
            .map(str::to_string)
            .unwrap_or_else(|| {
                if self.bias == 0 && !self.has_dst() {
                    "UTC".into()
                } else {
                    let off = -self.bias;
                    format!(
                        "UTC{}{:02}:{:02}",
                        if off < 0 { '-' } else { '+' },
                        off.abs() / 60,
                        off.abs() % 60
                    )
                }
            })
    }

    /// Nom de la période d'été (« Romance Daylight Time »).
    pub fn windows_daylight_name(&self) -> String {
        let key = self.windows_key();
        match key.strip_suffix(" Standard Time") {
            Some(base) => format!("{base} Daylight Time"),
            None => key,
        }
    }
}

// ---------------------------------------------------------------------
// Règle POSIX (pied de fichier TZif, variable TZ)
// ---------------------------------------------------------------------

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn number(&mut self) -> Option<i64> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()?
            .parse()
            .ok()
    }
    fn name(&mut self) -> Option<String> {
        let start = self.i;
        if self.eat(b'<') {
            while self.peek().is_some_and(|c| c != b'>') {
                self.i += 1;
            }
            let n = String::from_utf8_lossy(&self.s[start + 1..self.i]).into_owned();
            self.eat(b'>');
            return Some(n);
        }
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        (self.i - start >= 3).then(|| String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }
    /// [+-]hh[:mm[:ss]] en secondes.
    fn offset(&mut self) -> Option<i64> {
        let neg = if self.eat(b'-') {
            true
        } else {
            self.eat(b'+');
            false
        };
        let mut secs = self.number()? * 3600;
        if self.eat(b':') {
            secs += self.number()? * 60;
            if self.eat(b':') {
                secs += self.number()?;
            }
        }
        Some(if neg { -secs } else { secs })
    }
    fn rule(&mut self) -> Option<Rule> {
        if !self.eat(b'M') {
            return None; // Jn et n (jour de l'année) : non gérés par Windows non plus.
        }
        let month = self.number()? as u16;
        self.eat(b'.').then_some(())?;
        let week = self.number()? as u16;
        self.eat(b'.').then_some(())?;
        let weekday = self.number()? as u16;
        let seconds = if self.eat(b'/') {
            self.offset()? as i32
        } else {
            7200
        };
        ((1..=12).contains(&month) && (1..=5).contains(&week) && weekday <= 6).then_some(Rule {
            month,
            week,
            weekday,
            seconds,
        })
    }
}

/// Analyse une règle POSIX (« CET-1CEST,M3.5.0,M10.5.0/3 »).
pub fn parse_posix(spec: &str) -> Option<Zone> {
    let mut c = Cursor {
        s: spec.trim().as_bytes(),
        i: 0,
    };
    let std_abbr = c.name()?;
    let std_off = c.offset()?;
    let mut zone = Zone {
        bias: (std_off / 60) as i32,
        std_abbr: std_abbr.clone(),
        dst_abbr: std_abbr,
        ..Zone::default()
    };
    if c.peek().is_none() {
        return Some(zone);
    }
    zone.dst_abbr = c.name()?;
    let dst_off = if c.peek().is_some_and(|ch| ch != b',') {
        c.offset()?
    } else {
        std_off - 3600
    };
    zone.dst_bias = ((dst_off - std_off) / 60) as i32;
    if c.eat(b',') {
        zone.dst_start = c.rule();
        if c.eat(b',') {
            zone.dst_end = c.rule();
        }
    } else {
        // Règle par défaut de POSIX (américaine).
        zone.dst_start = Some(Rule {
            month: 3,
            week: 2,
            weekday: 0,
            seconds: 7200,
        });
        zone.dst_end = Some(Rule {
            month: 11,
            week: 1,
            weekday: 0,
            seconds: 7200,
        });
    }
    if !zone.has_dst() {
        zone.dst_start = None;
        zone.dst_end = None;
        zone.dst_bias = 0;
    }
    Some(zone)
}

/// Règle POSIX finale d'un fichier TZif (version 2 ou plus).
pub fn tzif_footer(data: &[u8]) -> Option<String> {
    if data.len() < 44 || &data[..4] != b"TZif" || data[4] < b'2' {
        return None;
    }
    let be = |at: usize| -> Option<usize> {
        Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    // Bloc version 1 (temps sur 32 bits) à sauter.
    let skip = |at: usize, time: usize| -> Option<usize> {
        let (isut, isstd, leap, times, types, chars) = (
            be(at + 20)?,
            be(at + 24)?,
            be(at + 28)?,
            be(at + 32)?,
            be(at + 36)?,
            be(at + 40)?,
        );
        Some(at + 44 + times * time + times + types * 6 + chars + leap * (time + 4) + isstd + isut)
    };
    let v2 = skip(0, 4)?;
    if data.get(v2..v2 + 4)? != b"TZif" {
        return None;
    }
    let end = skip(v2, 8)?;
    let rest = data.get(end..)?;
    let rest = rest.strip_prefix(b"\n")?;
    let close = rest.iter().position(|&b| b == b'\n')?;
    Some(String::from_utf8_lossy(&rest[..close]).into_owned())
}

/// Nom IANA du fuseau : `TZ`, /etc/timezone, ou la cible du lien
/// /etc/localtime.
fn zone_name() -> Option<String> {
    if let Ok(tz) = std::env::var("TZ") {
        let tz = tz.trim_start_matches(':').to_string();
        if !tz.is_empty() && !tz.starts_with('/') && !tz.contains(',') {
            return Some(tz);
        }
    }
    if let Ok(t) = std::fs::read_to_string("/etc/timezone") {
        let t = t.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let link = std::fs::read_link("/etc/localtime").ok()?;
    let s = link.to_string_lossy();
    s.split_once("zoneinfo/").map(|(_, n)| n.to_string())
}

/// Fuseau du système, lu une fois par processus.
pub fn current() -> &'static Zone {
    static ZONE: OnceLock<Zone> = OnceLock::new();
    ZONE.get_or_init(load)
}

fn load() -> Zone {
    let name = zone_name();
    // TZ peut porter directement une règle POSIX.
    if let Ok(tz) = std::env::var("TZ") {
        let tz = tz.trim_start_matches(':');
        if tz.contains(',')
            || (!tz.is_empty() && !tz.contains('/') && tz.chars().any(|c| c.is_ascii_digit()))
        {
            if let Some(z) = parse_posix(tz) {
                return z;
            }
        }
    }
    let file = match &name {
        Some(n) if !n.contains("..") => {
            let p = std::path::Path::new("/usr/share/zoneinfo").join(n);
            std::fs::read(&p).or_else(|_| std::fs::read("/etc/localtime"))
        }
        _ => std::fs::read("/etc/localtime"),
    };
    let mut zone = file
        .ok()
        .and_then(|d| tzif_footer(&d))
        .and_then(|f| parse_posix(&f))
        .unwrap_or_else(libc_zone);
    zone.iana = name.unwrap_or_default();
    zone
}

/// Repli sans fichier TZif lisible : décalage actuel donné par la libc.
fn libc_zone() -> Zone {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return Zone::utc();
        }
        let off = tm.tm_gmtoff as i32 / 60;
        let abbr = if tm.tm_zone.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(tm.tm_zone)
                .to_string_lossy()
                .into_owned()
        };
        Zone {
            bias: -off,
            std_abbr: abbr.clone(),
            dst_abbr: abbr,
            ..Zone::default()
        }
    }
}

/// Correspondance IANA → Windows (sous-ensemble de la table CLDR
/// « windowsZones » : Europe, Amériques, DOM-TOM et grandes villes).
pub fn windows_zone_name(iana: &str) -> Option<&'static str> {
    Some(match iana {
        "Europe/Paris" | "Europe/Brussels" | "Europe/Madrid" | "Europe/Copenhagen"
        | "Africa/Ceuta" => "Romance Standard Time",
        "Europe/Berlin" | "Europe/Amsterdam" | "Europe/Rome" | "Europe/Vienna"
        | "Europe/Zurich" | "Europe/Stockholm" | "Europe/Oslo" | "Europe/Luxembourg"
        | "Europe/Monaco" | "Europe/Andorra" | "Europe/Malta" | "Europe/Vaduz"
        | "Europe/San_Marino" | "Europe/Vatican" | "Europe/Gibraltar" | "Europe/Busingen" => {
            "W. Europe Standard Time"
        }
        "Europe/London" | "Europe/Dublin" | "Europe/Lisbon" | "Atlantic/Madeira"
        | "Atlantic/Canary" | "Atlantic/Faroe" | "Europe/Guernsey" | "Europe/Jersey"
        | "Europe/Isle_of_Man" => "GMT Standard Time",
        "Europe/Warsaw" | "Europe/Sarajevo" | "Europe/Zagreb" | "Europe/Skopje" => {
            "Central European Standard Time"
        }
        "Europe/Prague" | "Europe/Budapest" | "Europe/Bratislava" | "Europe/Ljubljana"
        | "Europe/Belgrade" | "Europe/Tirane" | "Europe/Podgorica" => {
            "Central Europe Standard Time"
        }
        "Europe/Athens" | "Europe/Bucharest" | "Asia/Nicosia" => "GTB Standard Time",
        "Europe/Helsinki" | "Europe/Kiev" | "Europe/Kyiv" | "Europe/Riga" | "Europe/Sofia"
        | "Europe/Tallinn" | "Europe/Vilnius" => "FLE Standard Time",
        "Europe/Moscow" | "Europe/Simferopol" => "Russian Standard Time",
        "Europe/Istanbul" => "Turkey Standard Time",
        "Africa/Algiers" | "Africa/Tunis" | "Africa/Lagos" | "Africa/Douala"
        | "Africa/Kinshasa" | "Africa/Libreville" => "W. Central Africa Standard Time",
        "Africa/Casablanca" | "Africa/El_Aaiun" => "Morocco Standard Time",
        "Africa/Abidjan" | "Africa/Dakar" | "Africa/Bamako" | "Africa/Conakry"
        | "Africa/Ouagadougou" | "Africa/Lome" | "Atlantic/Reykjavik" => "Greenwich Standard Time",
        "Africa/Cairo" => "Egypt Standard Time",
        "Africa/Johannesburg" => "South Africa Standard Time",
        "Indian/Reunion" | "Indian/Mauritius" => "Mauritius Standard Time",
        "Indian/Mayotte" | "Africa/Nairobi" => "E. Africa Standard Time",
        "America/New_York" | "America/Toronto" | "America/Montreal" | "America/Detroit"
        | "America/Nassau" => "Eastern Standard Time",
        "America/Chicago" | "America/Winnipeg" => "Central Standard Time",
        "America/Denver" | "America/Edmonton" | "America/Boise" => "Mountain Standard Time",
        "America/Phoenix" => "US Mountain Standard Time",
        "America/Los_Angeles" | "America/Vancouver" | "America/Tijuana" => "Pacific Standard Time",
        "America/Anchorage" => "Alaskan Standard Time",
        "Pacific/Honolulu" => "Hawaiian Standard Time",
        "America/Halifax" => "Atlantic Standard Time",
        "America/St_Johns" => "Newfoundland Standard Time",
        "America/Mexico_City" | "America/Monterrey" => "Central Standard Time (Mexico)",
        "America/Sao_Paulo" => "E. South America Standard Time",
        "America/Argentina/Buenos_Aires" | "America/Buenos_Aires" => "Argentina Standard Time",
        "America/Bogota" | "America/Lima" => "SA Pacific Standard Time",
        "America/Martinique"
        | "America/Guadeloupe"
        | "America/Puerto_Rico"
        | "America/Santo_Domingo"
        | "America/St_Barthelemy"
        | "America/Marigot" => "SA Western Standard Time",
        "America/Cayenne" => "SA Eastern Standard Time",
        "America/Miquelon" => "Saint Pierre Standard Time",
        "Pacific/Noumea" => "Central Pacific Standard Time",
        "Pacific/Tahiti" => "Hawaiian Standard Time",
        "Pacific/Wallis" | "Pacific/Auckland" => "New Zealand Standard Time",
        "Asia/Tokyo" => "Tokyo Standard Time",
        "Asia/Seoul" => "Korea Standard Time",
        "Asia/Shanghai" | "Asia/Hong_Kong" => "China Standard Time",
        "Asia/Singapore" => "Singapore Standard Time",
        "Asia/Kolkata" | "Asia/Calcutta" => "India Standard Time",
        "Asia/Dubai" => "Arabian Standard Time",
        "Asia/Jerusalem" => "Israel Standard Time",
        "Australia/Sydney" | "Australia/Melbourne" => "AUS Eastern Standard Time",
        "Etc/UTC" | "UTC" | "Etc/Universal" | "Etc/Zulu" | "Etc/GMT" | "GMT" => "UTC",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paris() -> Zone {
        let mut z = parse_posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        z.iana = "Europe/Paris".into();
        z
    }

    fn utc(y: i64, m: u32, d: u32, h: i64, min: i64) -> i64 {
        days_from_civil(y, m, d) * 86_400 + h * 3600 + min * 60
    }

    #[test]
    fn regle_posix_de_paris() {
        let z = paris();
        assert_eq!((z.bias, z.dst_bias), (-60, -60));
        assert_eq!(
            z.dst_start,
            Some(Rule {
                month: 3,
                week: 5,
                weekday: 0,
                seconds: 7200
            })
        );
        assert_eq!(z.dst_end.unwrap().seconds, 3 * 3600);
        assert_eq!(z.windows_key(), "Romance Standard Time");
        assert_eq!(z.windows_daylight_name(), "Romance Daylight Time");
        // 29 mars 2026, 01:00 UTC = 02:00 locale → passage à 03:00.
        assert!(!z.is_dst_utc(utc(2026, 3, 29, 0, 59)));
        assert!(z.is_dst_utc(utc(2026, 3, 29, 1, 0)));
        // 25 octobre 2026, 01:00 UTC = 03:00 d'été → retour à 02:00.
        assert!(z.is_dst_utc(utc(2026, 10, 25, 0, 59)));
        assert!(!z.is_dst_utc(utc(2026, 10, 25, 1, 0)));
        assert_eq!(z.bias_at_utc(utc(2026, 7, 14, 12, 0)), -120);
        assert_eq!(z.bias_at_utc(utc(2026, 12, 25, 12, 0)), -60);
        // 14 juillet, 14:00 locale = 12:00 UTC.
        assert_eq!(
            z.local_to_utc(utc(2026, 7, 14, 14, 0)),
            utc(2026, 7, 14, 12, 0)
        );
    }

    #[test]
    fn hemisphere_sud_et_sans_heure_d_ete() {
        let syd = parse_posix("AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(syd.bias, -600);
        assert!(syd.is_dst_utc(utc(2026, 1, 10, 0, 0)));
        assert!(!syd.is_dst_utc(utc(2026, 7, 10, 0, 0)));
        let reunion = parse_posix("<+04>-4").unwrap();
        assert_eq!((reunion.bias, reunion.has_dst()), (-240, false));
        assert_eq!(reunion.std_abbr, "+04");
        let ny = parse_posix("EST5EDT,M3.2.0,M11.1.0").unwrap();
        assert_eq!(ny.bias, 300);
        assert!(ny.is_dst_utc(utc(2026, 7, 1, 0, 0)));
        let unknown = Zone {
            bias: -330,
            ..Zone::default()
        };
        assert_eq!(unknown.windows_key(), "UTC+05:30");
        assert_eq!(Zone::utc().windows_key(), "UTC");
    }

    #[test]
    fn filetime_et_systemtime() {
        let st = SystemTime {
            year: 2026,
            month: 10,
            day_of_week: 6,
            day: 31,
            hour: 16,
            minute: 35,
            second: 14,
            millis: 250,
        };
        let ft = system_to_filetime(&st).unwrap();
        assert_eq!(filetime_to_system(ft), Some(st));
        assert_eq!(filetime_to_system(0).unwrap().year, 1601);
        assert_eq!(
            filetime_to_system(FILETIME_UNIX_EPOCH).unwrap().day_of_week,
            4
        );
        let bad = SystemTime {
            month: 2,
            day: 29,
            year: 2027,
            ..st
        };
        assert!(system_to_filetime(&bad).is_none());
        let leap = SystemTime {
            month: 2,
            day: 29,
            year: 2028,
            ..st
        };
        assert!(system_to_filetime(&leap).is_some());
        assert_eq!(civil_from_days(days_from_civil(1601, 1, 1)), (1601, 1, 1));
    }

    #[test]
    fn pied_de_fichier_tzif_systeme() {
        // Le conteneur de test a parfois la base IANA ; sinon rien à lire.
        let Ok(data) = std::fs::read("/usr/share/zoneinfo/Europe/Paris") else {
            return;
        };
        assert_eq!(
            tzif_footer(&data).as_deref(),
            Some("CET-1CEST,M3.5.0,M10.5.0/3")
        );
    }
}
