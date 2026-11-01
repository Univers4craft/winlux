//! Tests sur des sorties `xrandr --query` réelles (portable + écran HDMI).

use super::*;

const LAPTOP_AND_HDMI: &str = "\
Screen 0: minimum 320 x 200, current 1920 x 1080, maximum 16384 x 16384
eDP-1 connected primary 1920x1080+0+0 (normal left inverted right x axis y axis) 309mm x 174mm
   1920x1080     60.02*+  59.97    48.00
   1680x1050     59.95
   1280x720      60.00
HDMI-1 connected (normal left inverted right x axis y axis) 597mm x 336mm
   3840x2160     60.00 +  30.00
   2560x1440     59.95
   1920x1080     60.00    50.00
   1920x1080i    60.00
DP-1 disconnected (normal left inverted right x axis y axis)
";

const VM: &str = "\
Screen 0: minimum 320 x 200, current 1280 x 800, maximum 8192 x 8192
Virtual-1 connected primary 1280x800+0+0 (normal left inverted right x axis y axis) 0mm x 0mm
   1920x1080     60.00 +
   1280x800      59.81*
   1024x768      60.00
";

fn outputs() -> Vec<Output> {
    parse_query(LAPTOP_AND_HDMI)
}

#[test]
fn query_is_parsed() {
    let o = outputs();
    assert_eq!(o.len(), 3);
    assert_eq!(o[0].name, "eDP-1");
    assert!(o[0].connected && o[0].primary && o[0].internal());
    assert_eq!(o[0].geometry, Some((1920, 1080, 0, 0)));
    assert_eq!(o[0].mm, Some((309, 174)));
    assert_eq!(o[0].current_mode().unwrap().name(), "1920x1080");
    assert!(o[1].connected && !o[1].primary && !o[1].internal());
    assert_eq!(o[1].geometry, None, "branché mais éteint");
    assert_eq!(o[1].preferred_mode().unwrap().name(), "3840x2160");
    assert_eq!(o[1].modes.len(), 3, "entrelacé fusionné avec le progressif");
    assert!(!o[2].connected);
    let vm = parse_query(VM);
    assert_eq!(vm[0].mm, None, "0 mm : taille inconnue");
    assert_eq!(profile_id(&o), "HDMI-1+eDP-1");
}

#[test]
fn recommended_scale_follows_density() {
    // Portable 14" Full HD (≈158 ppp) : 150 %, comme Windows.
    assert_eq!(auto_scale((1920, 1080), Some((309, 174))), 1.5);
    // Écran 27" 4K (≈163 ppp) : 150 %.
    assert_eq!(auto_scale((3840, 2160), Some((597, 336))), 1.5);
    // 24" Full HD : 100 %.
    assert_eq!(auto_scale((1920, 1080), Some((527, 296))), 1.0);
    // VM (taille inconnue) : ancienne règle selon la hauteur.
    assert_eq!(auto_scale((1280, 800), None), 1.0);
    assert_eq!(auto_scale((3840, 2160), None), 2.0);
    // EDID farfelu (téléviseur annonçant 16 mm) ignoré.
    assert_eq!(auto_scale((1920, 1080), Some((16, 9))), 1.25);
}

#[test]
fn hotplugged_screen_extends_to_the_right_by_default() {
    let o = outputs();
    let w = wanted(&Config::default(), &o);
    assert_eq!(w.layout, Layout::Extend);
    let g = session_scale(&w, &o, false);
    assert_eq!(g, 1.5);
    let p = plan(&w, &o, false, g);
    let edp = p[0].on.as_ref().unwrap();
    let hdmi = p[1].on.as_ref().unwrap();
    assert!(edp.primary, "la dalle interne reste principale");
    assert_eq!((edp.x, edp.y, edp.mode), (0, 0, (1920, 1080)));
    assert_eq!((hdmi.x, hdmi.y, hdmi.mode), (1920, 0, (3840, 2160)));
    assert_eq!(p.len(), 2, "DP-1 débranché et éteint : rien à faire");
    let args = xrandr_args(&p).join(" ");
    assert_eq!(
        args,
        "--output eDP-1 --mode 1920x1080 --pos 0x0 --rotate normal --scale 1x1 --primary \
         --output HDMI-1 --mode 3840x2160 --pos 1920x0 --rotate normal --scale 1x1"
    );
}

#[test]
fn per_screen_scale_reduces_the_less_dense_screen() {
    let o = outputs();
    let mut c = Config::default();
    apply_command(&mut c, &o, &["scale", "HDMI-1", "2"]).unwrap();
    apply_command(&mut c, &o, &["scale", "eDP-1", "1"]).unwrap();
    let w = wanted(&c, &o);
    let g = session_scale(&w, &o, false);
    assert_eq!(g, 2.0, "le bureau dessine à la plus grande échelle");
    let p = plan(&w, &o, false, g);
    let edp = p[0].on.as_ref().unwrap();
    assert_eq!(edp.factor, 2.0);
    assert_eq!(edp.size(), (3840, 2160), "réduit de moitié : 100 % réel");
    let hdmi = p[1].on.as_ref().unwrap();
    assert_eq!((hdmi.factor, hdmi.x), (1.0, 3840));
    assert!(xrandr_args(&p).join(" ").contains("--scale 2x2"));
    // Bureau lancé à 1,5 : on applique tout de suite, en signalant qu'une
    // nouvelle session donnera une image plus nette.
    let p = plan(&w, &o, false, 1.5);
    assert_eq!(p[0].on.as_ref().unwrap().factor, 1.5);
    assert_eq!(p[1].on.as_ref().unwrap().factor, 0.75);
    assert!(xrandr_args(&p).join(" ").contains("--scale 0.75x0.75"));
}

#[test]
fn placement_primary_and_only_one_screen() {
    let o = outputs();
    let mut c = Config::default();
    apply_command(&mut c, &o, &["place", "HDMI-1", "left"]).unwrap();
    apply_command(&mut c, &o, &["mode", "HDMI-1", "2560x1440"]).unwrap();
    apply_command(&mut c, &o, &["primary", "HDMI-1"]).unwrap();
    let w = wanted(&c, &o);
    let p = plan(&w, &o, false, 1.0);
    let edp = p[0].on.as_ref().unwrap();
    let hdmi = p[1].on.as_ref().unwrap();
    assert!(hdmi.primary && !edp.primary);
    // Le principal est la référence ; l'interne est mis à sa droite.
    assert_eq!((hdmi.x, edp.x), (0, 2560));
    // Écran à gauche du principal (configuration inverse).
    let mut c = Config::default();
    apply_command(&mut c, &o, &["place", "HDMI-1", "left"]).unwrap();
    apply_command(&mut c, &o, &["mode", "HDMI-1", "1920x1080"]).unwrap();
    let p = plan(&wanted(&c, &o), &o, false, 1.0);
    assert_eq!(p[1].on.as_ref().unwrap().x, 0);
    assert_eq!(p[0].on.as_ref().unwrap().x, 1920);
    // Un seul écran : l'autre est éteint.
    apply_command(&mut c, &o, &["layout", "only", "HDMI-1"]).unwrap();
    let p = plan(&wanted(&c, &o), &o, false, 1.0);
    assert!(p[0].on.is_none());
    assert!(p[1].on.as_ref().unwrap().primary);
    assert!(xrandr_args(&p)
        .join(" ")
        .starts_with("--output eDP-1 --off"));
    // Les erreurs ne touchent pas la configuration.
    let before = c.clone();
    assert!(apply_command(&mut c, &o, &["mode", "HDMI-1", "9999x9999"]).is_err());
    assert!(
        apply_command(&mut c, &o, &["primary", "DP-1"]).is_err(),
        "débranché"
    );
    assert!(apply_command(&mut c, &o, &["scale", "eDP-1", "1;reboot"]).is_err());
    assert!(apply_command(&mut c, &o, &["place", "eDP-1", "dessous"]).is_err());
    assert_eq!(c, before);
}

#[test]
fn duplicate_uses_the_largest_common_resolution() {
    let o = outputs();
    let mut c = Config::default();
    apply_command(&mut c, &o, &["layout", "duplicate"]).unwrap();
    apply_command(&mut c, &o, &["scale", "auto"]).unwrap();
    let w = wanted(&c, &o);
    let p = plan(&w, &o, false, 1.5);
    for t in &p[..2] {
        let on = t.on.as_ref().unwrap();
        assert_eq!((on.mode, on.x, on.y), ((1920, 1080), 0, 0));
    }
}

#[test]
fn closing_the_lid_moves_everything_to_the_external_screen() {
    let o = outputs();
    let w = wanted(&Config::default(), &o);
    let p = plan(&w, &o, true, 1.5);
    assert!(p[0].on.is_none(), "dalle interne éteinte");
    let hdmi = p[1].on.as_ref().unwrap();
    assert!(hdmi.primary);
    assert_eq!((hdmi.x, hdmi.y), (0, 0));
    // Sans écran externe, le capot fermé ne change rien.
    let alone = parse_query(VM);
    let p = plan(&wanted(&Config::default(), &alone), &alone, true, 1.0);
    assert!(p[0].on.is_some());
}

#[test]
fn settings_are_remembered_per_set_of_screens() {
    let o = outputs();
    let mut c = Config::default();
    apply_command(&mut c, &o, &["place", "HDMI-1", "above"]).unwrap();
    assert_eq!(c.get("display.HDMI-1+eDP-1.place.HDMI-1"), Some("above"));
    // Écran débranché : l'interne seul retrouve ses propres réglages.
    let alone: Vec<Output> = o
        .iter()
        .cloned()
        .map(|mut x| {
            x.connected &= x.name == "eDP-1";
            x
        })
        .collect();
    let w = wanted(&c, &alone);
    assert!(w.places.is_empty());
    let p = plan(&w, &alone, false, 1.5);
    assert!(p.iter().all(|t| t.name != "HDMI-1" || t.on.is_none()));
    // Rebranché : la position au-dessus revient.
    let w = wanted(&Config::parse(&c.render()), &o);
    let p = plan(&w, &o, false, 1.0);
    let hdmi = p[1].on.as_ref().unwrap();
    let edp = p[0].on.as_ref().unwrap();
    assert_eq!((hdmi.x, hdmi.y), (0, 0));
    assert_eq!(edp.y as u32, hdmi.size().1);
}

#[test]
fn single_vm_screen_keeps_the_old_follow_and_fixed_policies() {
    let o = parse_query(VM);
    let mut c = Config::default();
    // Suivre la fenêtre : mode préféré.
    let p = plan(&wanted(&c, &o), &o, false, 1.0);
    assert_eq!(p[0].on.as_ref().unwrap().mode, (1920, 1080));
    assert!(!matches(&o, &p));
    apply_command(&mut c, &o, &["mode", "1024x768"]).unwrap();
    assert_eq!(c.get("POLICY"), Some("fixed"));
    assert_eq!(c.get("MODE"), Some("1024x768"));
    let p = plan(&wanted(&c, &o), &o, false, 1.0);
    assert_eq!(p[0].on.as_ref().unwrap().mode, (1024, 768));
    // Ancienne configuration (POLICY/MODE seuls) toujours comprise.
    let old = Config::parse("POLICY=fixed\nMODE=1280x800\nUI_SCALE=1.5\n");
    let w = wanted(&old, &o);
    let p = plan(&w, &o, false, session_scale(&w, &o, false));
    assert_eq!(session_scale(&w, &o, false), 1.5);
    assert_eq!(p[0].on.as_ref().unwrap().factor, 1.0);
    assert!(matches(&o, &p), "déjà dans cet état : aucun appel xrandr");
    apply_command(&mut c, &o, &["scale-step", "Virtual-1", "up"]).unwrap();
    assert_eq!(c.get("UI_SCALE"), Some("1.25"));
}

#[test]
fn status_lists_every_screen() {
    let o = outputs();
    let root = std::env::temp_dir().join(format!("winlux-display-{}", std::process::id()));
    let s = status_lines(&root, &o, &Config::default());
    for line in [
        "display_count=2",
        "display_output=eDP-1",
        "display_layout=extend",
        "display_1_name=eDP-1",
        "display_1_internal=1",
        "display_1_primary=1",
        "display_2_name=HDMI-1",
        "display_2_modes=3840x2160,2560x1440,1920x1080",
        "display_2_scale=1.5",
        "display_2_scale_auto=1",
        "display_2_rect=1920,0,3840,2160",
    ] {
        assert!(s.lines().any(|l| l == line), "{line} absent de :\n{s}");
    }
    assert!(s.is_ascii());
}
