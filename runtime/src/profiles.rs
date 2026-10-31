//! Optional, non-executable per-program settings. No profile is required to run.
use std::path::{Path, PathBuf};

pub fn path(exe: &Path) -> PathBuf {
    let mut name = exe.as_os_str().to_os_string();
    name.push(".winlux.ini");
    PathBuf::from(name)
}

#[derive(Default, Debug, PartialEq)]
pub struct Profile {
    pub scale: Option<String>,
    pub trace: Option<bool>,
    /// GPU préféré sur les machines hybrides (`auto`, `integrated`, `dedicated`).
    pub gpu: Option<crate::gpu::Preference>,
    /// Priorité, cœurs, affinité et nœud NUMA choisis dans les Paramètres.
    pub cpu: crate::cpu::LaunchSettings,
}

pub fn parse(text: &str) -> Profile {
    let mut profile = Profile::default();
    for line in text.lines() {
        match line.trim().split_once('=') {
            Some(("ui_scale", scale @ ("auto" | "1" | "1.25" | "1.5" | "2"))) => {
                profile.scale = Some(scale.to_owned());
            }
            Some(("trace", "1")) => profile.trace = Some(true),
            Some(("trace", "0")) => profile.trace = Some(false),
            Some(("gpu", value)) => profile.gpu = crate::gpu::Preference::parse(value),
            Some(("cpu_priority", value)) => {
                profile.cpu.priority = crate::cpu::Priority::parse(value);
            }
            Some(("cpu_cores", "performance")) => {
                profile.cpu.cores = crate::cpu::CoreChoice::Performance;
            }
            Some(("cpu_cores", "efficiency")) => {
                profile.cpu.cores = crate::cpu::CoreChoice::Efficiency;
            }
            Some(("cpu_affinity", list)) => profile.cpu.affinity = crate::cpu::parse_cpulist(list),
            Some(("numa_node", node)) => profile.cpu.numa_node = node.parse().ok(),
            _ => {}
        }
    }
    profile
}

/// Applique le profil et renvoie les réglages processeur à appliquer une
/// fois l'image chargée (le placement dépend des DLL importées).
pub fn apply(exe: &Path) -> crate::cpu::LaunchSettings {
    let path = path(exe);
    // Bound parsing even if someone replaces the small settings file.
    let Ok(file) = std::fs::File::open(path) else {
        return Default::default();
    };
    use std::io::Read;
    let mut text = String::new();
    if file.take(4097).read_to_string(&mut text).is_err() || text.len() > 4096 {
        return Default::default();
    }
    let profile = parse(&text);
    if let Some(scale) = profile.scale {
        if scale == "auto" {
            std::env::remove_var("WINLUX_UI_SCALE");
        } else {
            std::env::set_var("WINLUX_UI_SCALE", scale);
        }
    }
    if let Some(trace) = profile.trace {
        if trace {
            std::env::set_var("WINLUX_TRACE", "1");
        } else {
            std::env::remove_var("WINLUX_TRACE");
        }
    }
    if let Some(pref) = profile.gpu {
        // Sur une machine qui n'est pas hybride, la liste est vide : le
        // réglage reste sans effet au lieu d'échouer.
        let gpus = crate::gpu::probe(&crate::gpu::system_root());
        for (key, value) in crate::gpu::prime_env(&gpus, pref) {
            std::env::set_var(key, value);
        }
    }
    profile.cpu
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_optionnel_borne_et_sans_evaluation_de_commandes() {
        assert_eq!(parse(""), Profile::default());
        assert_eq!(
            parse("ui_scale=1.5\ntrace=1\n"),
            Profile {
                scale: Some("1.5".into()),
                trace: Some(true),
                ..Default::default()
            }
        );
        let cpu =
            parse("cpu_priority=high\ncpu_cores=efficiency\ncpu_affinity=0-2,5\nnuma_node=1\n").cpu;
        assert_eq!(cpu.priority, Some(crate::cpu::Priority::High));
        assert_eq!(cpu.cores, crate::cpu::CoreChoice::Efficiency);
        assert_eq!(cpu.affinity, vec![0, 1, 2, 5]);
        assert_eq!(cpu.numa_node, Some(1));
        let bad = parse("cpu_priority=turbo\ncpu_affinity=0-99999\nnuma_node=x\n").cpu;
        assert_eq!(bad, crate::cpu::LaunchSettings::default());
        assert_eq!(
            parse("gpu=dedicated\n").gpu,
            Some(crate::gpu::Preference::Performance)
        );
        assert_eq!(parse("gpu=$(id)\n").gpu, None);
        assert_eq!(
            parse("ui_scale=$(touch /tmp/no)\ntrace=yes\nWINLUX_MISSING=ignore\n"),
            Profile::default()
        );
        assert_eq!(
            path(Path::new("/tmp/Mon jeu.EXE")),
            Path::new("/tmp/Mon jeu.EXE.winlux.ini")
        );
    }
}
