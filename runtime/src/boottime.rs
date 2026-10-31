//! Temps de démarrage mesuré : l'init note l'instant (`/proc/uptime`) de
//! chaque étape dans `/run/winlux/boot-times`, puis `winlux --boot-time record`
//! garde le total dans `/var/lib/winlux/boot-history` quand le bureau est
//! lancé. Les Paramètres (page Processeur) affichent le rapport `--boot-time`.

use std::fs;
use std::path::Path;

/// Démarrages gardés dans l'historique.
const HISTORY: usize = 20;
/// Dernière étape : le bureau est lancé, le démarrage est terminé.
pub const DESKTOP_MARK: &str = "desktop";

/// Étapes notées par l'init : `(nom, secondes depuis l'allumage du noyau)`.
pub fn parse_marks(text: &str) -> Vec<(String, f64)> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let name = it.next()?;
            let at: f64 = it.next()?.parse().ok()?;
            (at.is_finite() && at >= 0.0 && name.bytes().all(|b| b.is_ascii_graphic()))
                .then(|| (name.to_string(), at))
        })
        .collect()
}

/// Durée de chaque étape (ms) : du repère précédent au sien. La première
/// étape, `init`, couvre le micrologiciel déjà terminé, le noyau et l'initramfs.
pub fn phases(marks: &[(String, f64)]) -> Vec<(String, u64)> {
    let mut previous = 0.0;
    marks
        .iter()
        .map(|(name, at)| {
            let ms = ((at - previous).max(0.0) * 1000.0).round() as u64;
            previous = previous.max(*at);
            (name.clone(), ms)
        })
        .collect()
}

pub fn total_ms(marks: &[(String, f64)]) -> Option<u64> {
    marks
        .iter()
        .find(|(name, _)| name == DESKTOP_MARK)
        .map(|(_, at)| (at * 1000.0).round() as u64)
}

pub fn parse_history(text: &str) -> Vec<(u64, u64)> {
    text.lines()
        .filter_map(|line| {
            let (when, ms) = line.trim().split_once(' ')?;
            Some((when.parse().ok()?, ms.trim().parse().ok()?))
        })
        .collect()
}

/// Rapport `clé=valeur` lu par les Paramètres et `winlux-control`.
pub fn report(marks: &[(String, f64)], history: &[(u64, u64)]) -> String {
    let mut out = String::new();
    let Some(total) = total_ms(marks) else {
        // Démarrage pas terminé (ou init sans repères) : rien à montrer.
        out.push_str("boot_known=0\n");
        return out;
    };
    let steps = phases(marks);
    out.push_str("boot_known=1\n");
    out.push_str(&format!("boot_total_ms={total}\n"));
    if let Some((_, kernel)) = steps.first() {
        out.push_str(&format!("boot_kernel_ms={kernel}\n"));
    }
    let list: Vec<String> = steps.iter().map(|(n, ms)| format!("{n}:{ms}")).collect();
    out.push_str(&format!("boot_phases={}\n", list.join(",")));
    // L'étape la plus longue après le noyau : c'est elle qu'il faut regarder.
    if let Some((name, ms)) = steps.iter().skip(1).max_by_key(|(_, ms)| *ms) {
        out.push_str(&format!("boot_slowest={name}\nboot_slowest_ms={ms}\n"));
    }
    // L'historique contient déjà ce démarrage une fois enregistré.
    let earlier = match history.last() {
        Some((_, ms)) if *ms == total => &history[..history.len() - 1],
        _ => history,
    };
    if let Some((_, ms)) = earlier.last() {
        out.push_str(&format!("boot_previous_ms={ms}\n"));
    }
    let best = earlier.iter().map(|(_, ms)| *ms).chain([total]).min();
    if let Some(best) = best {
        out.push_str(&format!("boot_best_ms={best}\n"));
    }
    out.push_str(&format!("boot_count={}\n", earlier.len() + 1));
    out
}

fn read(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join(rel)).unwrap_or_default()
}

/// Ajoute le démarrage en cours à l'historique (une seule fois par démarrage).
pub fn record(root: &Path) -> Result<u64, String> {
    let marks = parse_marks(&read(root, "run/winlux/boot-times"));
    let total = total_ms(&marks).ok_or("bureau pas encore lancé")?;
    let flag = root.join("run/winlux/boot-recorded");
    if flag.exists() {
        return Ok(total);
    }
    let mut history = parse_history(&read(root, "var/lib/winlux/boot-history"));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    history.push((now, total));
    let keep = history.len().saturating_sub(HISTORY);
    let text: String = history[keep..]
        .iter()
        .map(|(when, ms)| format!("{when} {ms}\n"))
        .collect();
    let dir = root.join("var/lib/winlux");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join("boot-history.tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join("boot-history")).map_err(|e| e.to_string())?;
    let _ = fs::write(flag, "1\n");
    Ok(total)
}

/// `winlux --boot-time [record] [RACINE]`.
pub fn cli(args: &[String]) -> i32 {
    let (recording, rest) = match args.first().map(String::as_str) {
        Some("record") => (true, &args[1..]),
        _ => (false, args),
    };
    let root = Path::new(rest.first().map_or("/", String::as_str));
    if recording {
        return match record(root) {
            Ok(ms) => {
                println!("WINLUX_BOOT_MS={ms}");
                0
            }
            Err(e) => {
                eprintln!("winlux --boot-time : {e}");
                1
            }
        };
    }
    let marks = parse_marks(&read(root, "run/winlux/boot-times"));
    let history = parse_history(&read(root, "var/lib/winlux/boot-history"));
    print!("{}", report(&marks, &history));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKS: &str = "init 1.50\nrecovery 1.75\ndrivers 4.90\nhardware 6.00\nservices 6.40\nxorg 7.10\nsession 8.30\ndesktop 8.42\n";

    #[test]
    fn phases_follow_marks() {
        let marks = parse_marks(MARKS);
        let steps = phases(&marks);
        assert_eq!(steps[0], ("init".into(), 1500));
        assert_eq!(steps[2], ("drivers".into(), 3150));
        assert_eq!(total_ms(&marks), Some(8420));
        assert_eq!(steps.iter().map(|(_, ms)| ms).sum::<u64>(), 8420);
    }

    #[test]
    fn report_names_slowest_and_compares() {
        let marks = parse_marks(MARKS);
        let r = report(&marks, &[(1, 12000), (2, 8420)]);
        assert!(r.contains("boot_total_ms=8420\n"));
        assert!(r.contains("boot_kernel_ms=1500\n"));
        assert!(r.contains("boot_slowest=drivers\nboot_slowest_ms=3150\n"));
        assert!(r.contains("boot_previous_ms=12000\n"));
        assert!(r.contains("boot_best_ms=8420\n"));
        assert!(r.contains("boot_count=2\n"));
    }

    #[test]
    fn unfinished_boot_and_garbage() {
        assert_eq!(report(&parse_marks("init 1.2\n"), &[]), "boot_known=0\n");
        assert!(parse_marks("init\nx y\nok -1\ndesktop 3.5\n") == vec![("desktop".into(), 3.5)]);
        assert_eq!(parse_history("10 900\nbad\n11 x\n"), vec![(10, 900)]);
    }

    #[test]
    fn record_once_per_boot() {
        let root = std::env::temp_dir().join(format!("winlux-boot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("run/winlux")).unwrap();
        fs::create_dir_all(root.join("var/lib/winlux")).unwrap();
        let old: String = (0..HISTORY).map(|i| format!("{i} 9000\n")).collect();
        fs::write(root.join("var/lib/winlux/boot-history"), old).unwrap();
        assert!(record(&root).is_err());
        fs::write(root.join("run/winlux/boot-times"), MARKS).unwrap();
        assert_eq!(record(&root), Ok(8420));
        assert_eq!(record(&root), Ok(8420));
        let history = parse_history(&read(&root, "var/lib/winlux/boot-history"));
        assert_eq!(history.len(), HISTORY);
        assert_eq!(history.last().unwrap().1, 8420);
        let r = report(&parse_marks(MARKS), &history);
        assert!(r.contains("boot_previous_ms=9000\n"));
        let _ = fs::remove_dir_all(&root);
    }
}
