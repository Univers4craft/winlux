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
            _ => {}
        }
    }
    profile
}

pub fn apply(exe: &Path) {
    let path = path(exe);
    // Bound parsing even if someone replaces the small settings file.
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    use std::io::Read;
    let mut text = String::new();
    if file.take(4097).read_to_string(&mut text).is_err() || text.len() > 4096 {
        return;
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
                trace: Some(true)
            }
        );
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
