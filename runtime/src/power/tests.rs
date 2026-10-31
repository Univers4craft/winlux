//! Tests sur un faux sysfs construit dans un dossier temporaire.

use super::daemon::{
    acpi_inputs, brightness_key, brightness_step, decode_event, external_display_at, has_key,
    idle_step, lid_action, Hw, IdleStep,
};
use super::*;
use std::sync::atomic::AtomicUsize;

struct Fake(PathBuf);

impl Fake {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "winlux-power-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn put(&self, rel: &str, value: &str) -> &Self {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, format!("{value}\n")).unwrap();
        self
    }
    fn get(&self, rel: &str) -> String {
        read_trim(&self.0.join(rel)).unwrap_or_default()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const PS: &str = "sys/class/power_supply";

#[test]
fn no_battery_means_mains() {
    let f = Fake::new();
    let s = read_status_at(&f.0);
    assert!(s.ac_online);
    assert!(s.battery.is_none());
    let w = to_system_power_status(&s, false);
    assert_eq!(w.ac_line_status, 1);
    assert_eq!(w.battery_flag, 128);
    assert_eq!(w.battery_life_percent, 255);
    assert_eq!(w.battery_life_time, UNKNOWN_TIME);
}

#[test]
fn discharging_battery_estimates_time_left() {
    let f = Fake::new();
    f.put(&format!("{PS}/AC/type"), "Mains")
        .put(&format!("{PS}/AC/online"), "0")
        .put(&format!("{PS}/BAT0/type"), "Battery")
        .put(&format!("{PS}/BAT0/status"), "Discharging")
        .put(&format!("{PS}/BAT0/capacity"), "50")
        .put(&format!("{PS}/BAT0/energy_now"), "25000000")
        .put(&format!("{PS}/BAT0/energy_full"), "50000000")
        .put(&format!("{PS}/BAT0/energy_full_design"), "62500000")
        .put(&format!("{PS}/BAT0/power_now"), "10000000");
    let s = read_status_at(&f.0);
    assert!(!s.ac_online);
    let b = s.battery.clone().unwrap();
    assert_eq!(b.percent, 50);
    assert_eq!(b.state, Charge::Discharging);
    assert_eq!(b.seconds_left, Some(9000)); // 25 Wh / 10 W = 2 h 30
    assert_eq!(b.seconds_full_life, Some(18000));
    assert_eq!(b.health_percent, Some(80));
    assert_eq!(b.rate_mw, Some(10000));
    assert_eq!(format_duration(9000), "2 h 30 min");
    let w = to_system_power_status(&s, true);
    assert_eq!(w.ac_line_status, 0);
    assert_eq!(w.battery_flag, 0);
    assert_eq!(w.battery_life_percent, 50);
    assert_eq!(w.system_status_flag, 1);
    assert_eq!(w.battery_life_time, 9000);
    assert_eq!(w.battery_full_life_time, 18000);
    assert_eq!(std::mem::size_of::<SystemPowerStatus>(), 12);
}

#[test]
fn charging_battery_uses_charge_and_voltage() {
    let f = Fake::new();
    f.put(&format!("{PS}/ADP1/type"), "Mains")
        .put(&format!("{PS}/ADP1/online"), "1")
        .put(&format!("{PS}/BAT1/type"), "Battery")
        .put(&format!("{PS}/BAT1/status"), "Charging")
        .put(&format!("{PS}/BAT1/capacity"), "4")
        .put(&format!("{PS}/BAT1/voltage_now"), "10000000")
        .put(&format!("{PS}/BAT1/charge_now"), "200000")
        .put(&format!("{PS}/BAT1/charge_full"), "5000000")
        .put(&format!("{PS}/BAT1/current_now"), "2400000")
        // Batterie d'une souris : ignorée.
        .put(&format!("{PS}/hidpp_battery_0/type"), "Battery")
        .put(&format!("{PS}/hidpp_battery_0/scope"), "Device")
        .put(&format!("{PS}/hidpp_battery_0/capacity"), "100");
    let s = read_status_at(&f.0);
    assert!(s.ac_online);
    let b = s.battery.clone().unwrap();
    assert_eq!(b.count, 1);
    assert_eq!(b.percent, 4);
    assert_eq!(b.state, Charge::Charging);
    assert_eq!(b.seconds_to_full, Some(7200)); // 48 Wh restants à 24 W
    assert_eq!(b.seconds_left, None);
    let w = to_system_power_status(&s, false);
    assert_eq!(w.battery_flag, 2 | 4 | 8);
    assert_eq!(w.battery_life_time, UNKNOWN_TIME);
}

#[test]
fn two_batteries_are_aggregated_by_energy() {
    let f = Fake::new();
    for (name, now, full) in [
        ("BAT0", "10000000", "40000000"),
        ("BAT1", "30000000", "40000000"),
    ] {
        f.put(&format!("{PS}/{name}/type"), "Battery")
            .put(&format!("{PS}/{name}/status"), "Discharging")
            .put(&format!("{PS}/{name}/capacity"), "99")
            .put(&format!("{PS}/{name}/energy_now"), now)
            .put(&format!("{PS}/{name}/energy_full"), full);
    }
    let s = read_status_at(&f.0);
    assert!(!s.ac_online, "décharge sans adaptateur exposé");
    let b = s.battery.unwrap();
    assert_eq!(b.count, 2);
    assert_eq!(b.percent, 50);
    assert_eq!(b.seconds_left, None, "débit inconnu : pas d'estimation");
}

#[test]
fn profile_guids_roundtrip() {
    for p in Profile::ALL {
        assert_eq!(Profile::from_guid(&p.guid()), Some(p));
        assert_eq!(Profile::parse(p.key()), Some(p));
    }
    // 381b4222-f694-41f0-9685-ff5bb260df2e en mémoire.
    assert_eq!(
        Profile::Balanced.guid(),
        [
            0x22, 0x42, 0x1b, 0x38, 0x94, 0xf6, 0xf0, 0x41, 0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60,
            0xdf, 0x2e
        ]
    );
    assert_eq!(Profile::from_guid(&[0; 16]), None);
}

#[test]
fn governor_choice_follows_driver() {
    let intel = "performance powersave";
    assert_eq!(
        pick_governor(Profile::Performance, intel),
        Some("performance")
    );
    assert_eq!(pick_governor(Profile::Balanced, intel), Some("powersave"));
    assert_eq!(pick_governor(Profile::Saver, intel), Some("powersave"));
    let acpi = "conservative ondemand userspace powersave performance schedutil";
    assert_eq!(pick_governor(Profile::Balanced, acpi), Some("schedutil"));
    assert_eq!(pick_governor(Profile::Saver, "userspace"), None);
}

#[test]
fn config_roundtrip_and_defaults() {
    let c = Config::parse("PROFILE=saver\nSCREEN_DC=60\nLID=hibernate\nBRIGHTNESS=200\nJUNK\n");
    assert_eq!(c.profile, Profile::Saver);
    assert_eq!(c.screen_dc, 60);
    assert_eq!(c.screen_ac, Config::default().screen_ac);
    assert_eq!(c.lid, Action::Hibernate);
    assert_eq!(c.brightness, 100);
    assert_eq!(c.button_action(true), Action::Sleep);
    assert_eq!(c.button_action(false), Action::Shutdown);
    assert_eq!(Config::parse(&c.serialize()), c);
    let f = Fake::new();
    c.save_at(&f.0).unwrap();
    assert_eq!(Config::load_at(&f.0), c);
}

#[test]
fn saver_caps_timeouts() {
    let c = Config {
        screen_dc: 600,
        sleep_dc: 0,
        ..Config::default()
    };
    assert_eq!(c.effective_timeouts(false), (600, 0));
    let s = Config {
        profile: Profile::Saver,
        ..c
    };
    assert_eq!(s.effective_timeouts(false), (120, 300));
    assert_eq!(next_delay(60), 120);
    assert_eq!(next_delay(0), 60);
    assert_eq!(delay_label(0), "Jamais");
    assert_eq!(delay_label(7200), "2 h");
}

#[test]
fn applying_a_profile_writes_sysfs() {
    let f = Fake::new();
    let cpu = "sys/devices/system/cpu/cpufreq";
    for p in ["policy0", "policy1"] {
        f.put(
            &format!("{cpu}/{p}/scaling_available_governors"),
            "performance powersave",
        )
        .put(&format!("{cpu}/{p}/scaling_governor"), "powersave")
        .put(
            &format!("{cpu}/{p}/energy_performance_available_preferences"),
            "default performance balance_performance balance_power power",
        )
        .put(
            &format!("{cpu}/{p}/energy_performance_preference"),
            "default",
        );
    }
    f.put("sys/devices/system/cpu/intel_pstate/no_turbo", "0")
        .put("sys/class/backlight/intel_backlight/max_brightness", "1000")
        .put("sys/class/backlight/intel_backlight/brightness", "1000")
        .put("sys/class/backlight/intel_backlight/bl_power", "0")
        .put("sys/bus/usb/devices/1-1/power/control", "on")
        .put("sys/bus/usb/devices/1-2/power/control", "on")
        .put("sys/bus/usb/devices/1-2/1-2:1.0/bInterfaceClass", "03")
        .put(
            "sys/class/scsi_host/host0/link_power_management_policy",
            "max_performance",
        );
    let config = Config {
        profile: Profile::Saver,
        brightness: 80,
        ..Config::default()
    };
    let a = apply_profile_at(&f.0, &config);
    assert_eq!(a.governor, Some("powersave"));
    assert_eq!(a.cpus, 2);
    assert_eq!(a.brightness, Some(56)); // 80 % × 0,7
    assert_eq!(
        f.get("sys/class/backlight/intel_backlight/brightness"),
        "560"
    );
    assert_eq!(
        f.get(&format!("{cpu}/policy0/energy_performance_preference")),
        "power"
    );
    assert_eq!(f.get("sys/devices/system/cpu/intel_pstate/no_turbo"), "1");
    assert_eq!(f.get("sys/bus/usb/devices/1-1/power/control"), "auto");
    assert_eq!(
        f.get("sys/bus/usb/devices/1-2/power/control"),
        "on",
        "clavier/souris jamais suspendus"
    );
    assert_eq!(
        f.get("sys/class/scsi_host/host0/link_power_management_policy"),
        "min_power"
    );
    assert!(!f.0.join("sys/module/pcie_aspm/parameters/policy").exists());
    assert_eq!(brightness_at(&f.0), Some(56));
    let perf = Config {
        profile: Profile::Performance,
        ..config
    };
    apply_profile_at(&f.0, &perf);
    assert_eq!(
        f.get(&format!("{cpu}/policy1/scaling_governor")),
        "performance"
    );
    assert_eq!(f.get("sys/devices/system/cpu/intel_pstate/no_turbo"), "0");
    assert!(set_backlight_power_at(&f.0, false));
    assert_eq!(f.get("sys/class/backlight/intel_backlight/bl_power"), "4");
}

#[test]
fn inhibitors_follow_process_lifetime() {
    let f = Fake::new();
    f.put("proc/4242/stat", "x");
    set_inhibit_at(&f.0, "4242-t1", ES_SYSTEM_REQUIRED | ES_CONTINUOUS);
    set_inhibit_at(&f.0, "4242-r1", ES_DISPLAY_REQUIRED);
    set_inhibit_at(&f.0, "999999-t1", ES_SYSTEM_REQUIRED);
    assert_eq!(
        active_inhibits_at(&f.0),
        ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED | ES_CONTINUOUS
    );
    assert!(!f.0.join("run/winlux/power-inhibit/999999-t1").exists());
    set_inhibit_at(&f.0, "4242-t1", ES_CONTINUOUS);
    set_inhibit_at(&f.0, "4242-r1", 0);
    assert_eq!(active_inhibits_at(&f.0), 0);
}

#[test]
fn activity_timestamp() {
    let f = Fake::new();
    assert_eq!(last_activity_at(&f.0), None);
    fs::create_dir_all(f.0.join("run/winlux")).unwrap();
    touch_activity_at(&f.0);
    let t = last_activity_at(&f.0).unwrap();
    assert!(now_secs().abs_diff(t) < 5);
}

#[test]
fn idle_policy() {
    let t = (300, 900);
    assert_eq!(idle_step(10, t, 0, false, true), IdleStep::None);
    assert_eq!(idle_step(300, t, 0, false, true), IdleStep::ScreenOff);
    assert_eq!(idle_step(400, t, 0, true, true), IdleStep::None);
    assert_eq!(idle_step(5, t, 0, true, true), IdleStep::ScreenOn);
    assert_eq!(idle_step(900, t, 0, true, true), IdleStep::Sleep);
    assert_eq!(idle_step(900, t, 0, true, false), IdleStep::None);
    // Vidéo en cours : ni écran éteint ni veille.
    assert_eq!(
        idle_step(5000, t, ES_DISPLAY_REQUIRED, false, true),
        IdleStep::None
    );
    // Téléchargement : écran éteint mais pas de veille.
    assert_eq!(
        idle_step(5000, t, ES_SYSTEM_REQUIRED, false, true),
        IdleStep::ScreenOff
    );
    assert_eq!(idle_step(5000, (0, 0), 0, false, true), IdleStep::None);
}

#[test]
fn evdev_events() {
    let ev = |kind: u16, code: u16, value: i32| {
        let mut b = [0u8; 24];
        b[16..18].copy_from_slice(&kind.to_ne_bytes());
        b[18..20].copy_from_slice(&code.to_ne_bytes());
        b[20..24].copy_from_slice(&value.to_ne_bytes());
        b
    };
    assert_eq!(decode_event(&ev(1, 116, 1)), Some(Hw::PowerButton));
    assert_eq!(decode_event(&ev(1, 116, 0)), None, "relâchement ignoré");
    assert_eq!(decode_event(&ev(1, 142, 1)), Some(Hw::SleepButton));
    assert_eq!(decode_event(&ev(5, 0, 1)), Some(Hw::LidClosed));
    assert_eq!(decode_event(&ev(5, 0, 0)), Some(Hw::LidOpened));
    assert_eq!(decode_event(&ev(0, 0, 0)), None);
    assert_eq!(decode_event(&ev(1, 225, 1)), Some(Hw::BrightnessUp));
    assert_eq!(
        decode_event(&ev(1, 224, 2)),
        Some(Hw::BrightnessDown),
        "répétition"
    );
    assert_eq!(decode_event(&ev(1, 225, 0)), None);
}

#[test]
fn brightness_keys_devices_are_found_by_capability() {
    let f = Fake::new();
    // Bit 224 et 225 : quatrième mot de 64 bits (le plus à gauche ici).
    f.put("sys/class/input/event3/device/name", "Video Bus")
        .put(
            "sys/class/input/event3/device/capabilities/key",
            "3e000b00000000 0 0 0",
        )
        .put("sys/class/input/event5/device/name", "USB Optical Mouse")
        .put(
            "sys/class/input/event5/device/capabilities/key",
            "1f0000 0 0 0 0",
        )
        .put("sys/class/input/event0/device/name", "Lid Switch");
    assert!(has_key("3e000b00000000 0 0 0", 224));
    assert!(has_key("3e000b00000000 0 0 0", 225));
    assert!(!has_key("1f0000 0 0 0 0", 225));
    assert!(!has_key("", 225));
    let found = acpi_inputs(&f.0);
    let names: Vec<_> = found
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["event0", "event3"]);
}

#[test]
fn brightness_steps_are_finer_at_the_bottom() {
    assert_eq!(brightness_step(80, true), 90);
    assert_eq!(brightness_step(95, true), 100);
    assert_eq!(brightness_step(10, true), 15);
    assert_eq!(brightness_step(20, false), 15);
    assert_eq!(brightness_step(5, false), 5);
    assert_eq!(brightness_step(50, false), 40);
}

#[test]
fn brightness_key_steps_once_even_when_the_kernel_already_did() {
    let f = Fake::new();
    f.put("sys/class/backlight/intel_backlight/max_brightness", "100")
        .put("sys/class/backlight/intel_backlight/brightness", "50");
    // Le noyau n'a rien fait : on fait le pas.
    assert_eq!(brightness_key(&f.0, true, Some(50)), Some(60));
    assert_eq!(
        f.get("sys/class/backlight/intel_backlight/brightness"),
        "60"
    );
    // Le noyau a déjà monté de 60 à 70 : on ne rajoute rien.
    f.put("sys/class/backlight/intel_backlight/brightness", "70");
    assert_eq!(brightness_key(&f.0, true, Some(60)), Some(70));
    assert_eq!(
        f.get("sys/class/backlight/intel_backlight/brightness"),
        "70"
    );
    // La préférence suit, pour que la reprise de veille ne la remette pas.
    assert_eq!(Config::load_at(&f.0).brightness, 70);
    // Sans rétroéclairage (VM, écran externe) : rien.
    let g = Fake::new();
    assert_eq!(brightness_key(&g.0, true, None), None);
}

#[test]
fn closing_the_lid_with_an_external_screen_keeps_the_machine_awake() {
    let f = Fake::new();
    f.put("sys/class/drm/card0-eDP-1/status", "connected")
        .put("sys/class/drm/card0-HDMI-A-1/status", "disconnected");
    assert!(!external_display_at(&f.0));
    assert_eq!(lid_action(Action::Sleep, false), Action::Sleep);
    f.put("sys/class/drm/card0-HDMI-A-1/status", "connected");
    assert!(external_display_at(&f.0));
    assert_eq!(lid_action(Action::Sleep, true), Action::Nothing);
    assert_eq!(lid_action(Action::Hibernate, true), Action::Nothing);
    assert_eq!(
        lid_action(Action::Shutdown, true),
        Action::Shutdown,
        "choix explicite gardé"
    );
}

#[test]
fn suspend_follows_wakeup_count_and_runs_hooks() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fake::new();
    f.put("sys/power/state", "freeze mem")
        .put("sys/power/wakeup_count", "42");
    let log = f.0.join("hooks.log");
    for (dir, name) in [
        ("usr/lib/winlux/sleep.d", "10-ecrans"),
        ("usr/lib/winlux/sleep.d", "20-pave"),
        ("etc/winlux/sleep.d", "20-pave"),
    ] {
        let p = f.0.join(dir).join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(
            &p,
            format!(
                "#!/bin/sh\necho \"{dir}/{name} $1 $2\" >> '{}'\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    // Un fichier non exécutable est ignoré.
    f.put("etc/winlux/sleep.d/README", "notes");
    assert_eq!(sleep::hooks_at(&f.0).len(), 2);
    sleep::suspend_at(&f.0).unwrap();
    // Faux sysfs : fichier ordinaire écrit sans troncature.
    assert!(f.get("sys/power/state").starts_with("mem"));
    assert_eq!(f.get("sys/power/wakeup_count"), "42");
    let ran = fs::read_to_string(&log).unwrap();
    assert_eq!(
        ran,
        "usr/lib/winlux/sleep.d/10-ecrans pre suspend\n\
         etc/winlux/sleep.d/20-pave pre suspend\n\
         usr/lib/winlux/sleep.d/10-ecrans post suspend\n\
         etc/winlux/sleep.d/20-pave post suspend\n"
    );
    assert!(f.get("run/winlux/power-event").starts_with("resume "));
}

#[test]
fn sleep_capabilities() {
    let f = Fake::new();
    assert_eq!(sleep::capabilities_at(&f.0), sleep::Capabilities::default());
    f.put("sys/power/state", "freeze mem disk")
        .put("sys/power/resume", "0:0");
    let c = sleep::capabilities_at(&f.0);
    assert!(c.suspend);
    assert!(!c.hibernate, "pas de zone de reprise");
    f.put("sys/power/resume", "259:2");
    assert!(sleep::capabilities_at(&f.0).hibernate);
    f.put("proc/mounts", "rootfs / rootfs rw 0 0\nnone / ramfs rw 0 0");
    assert!(!sleep::disk_root(&f.0));
    f.put("sys/power/resume", "0:0");
    let err = sleep::hibernate_at(&f.0).unwrap_err();
    assert!(err.contains("Live"), "{err}");
    assert!(sleep::hibernate_setup_at(&f.0)
        .unwrap_err()
        .contains("Live"));
    f.put("proc/mounts", "/dev/vda2 / ext4 rw,relatime 0 0");
    assert!(sleep::disk_root(&f.0));
    let err = sleep::hibernate_at(&f.0).unwrap_err();
    assert!(err.contains("hibernate-setup"), "{err}");
}

#[test]
fn swap_header_layout() {
    let h = sleep::swap_header(262_144);
    assert_eq!(h.len(), 4096);
    assert_eq!(&h[4086..], b"SWAPSPACE2");
    assert_eq!(u32::from_le_bytes(h[1024..1028].try_into().unwrap()), 1);
    assert_eq!(
        u32::from_le_bytes(h[1028..1032].try_into().unwrap()),
        262_143
    );
    assert_eq!(u32::from_le_bytes(h[1032..1036].try_into().unwrap()), 0);
}

#[test]
fn grub_entries_gain_resume() {
    let cfg = "menuentry \"Winlux\" {\n    linux /boot/vmlinuz root=PARTUUID=ab-01 rw init=/init quiet\n}\n\
menuentry \"Secours\" {\n    linux /boot/vmlinuz root=PARTUUID=ab-01 rw init=/init winlux.recovery=1\n}\n";
    let once = sleep::patch_grub(cfg, 34816);
    assert!(once.contains("init=/init quiet resume=PARTUUID=ab-01 resume_offset=34816\n"));
    assert!(!once.contains("winlux.recovery=1 resume="));
    let twice = sleep::patch_grub(&sleep::patch_grub(&once, 1), 34816);
    assert_eq!(twice, once, "idempotent");
}

#[test]
fn status_lines_for_shell() {
    let f = Fake::new();
    f.put(&format!("{PS}/BAT0/type"), "Battery")
        .put(&format!("{PS}/BAT0/status"), "Discharging")
        .put(&format!("{PS}/BAT0/capacity"), "42")
        .put(&format!("{PS}/BAT0/energy_now"), "21000000")
        .put(&format!("{PS}/BAT0/energy_full"), "50000000")
        .put(&format!("{PS}/BAT0/power_now"), "7000000");
    let text = status_lines(&read_status_at(&f.0), &Config::default(), &f.0);
    assert!(text.is_ascii(), "{text}");
    for line in [
        "power_ac=0",
        "power_percent=42",
        "power_state=discharging",
        "power_seconds_left=10800",
        "power_profile=balanced",
        "power_screen_now=5 min",
        "power_lid=sleep",
        "power_button=sleep",
        "power_brightness=",
        "power_can_sleep=0",
    ] {
        assert!(text.lines().any(|l| l == line), "{line} absent de\n{text}");
    }
}

#[test]
fn virtual_machine_never_idles_to_sleep() {
    let f = Fake::new();
    assert!(!is_virtual_at(&f.0));
    f.put(
        "proc/cpuinfo",
        "processor\t: 0\nflags\t\t: fpu sse2 hypervisor lm\n",
    );
    assert!(is_virtual_at(&f.0));
    let text = status_lines(&read_status_at(&f.0), &Config::default(), &f.0);
    assert!(
        text.contains("power_sleep_now=Jamais (machine virtuelle)\n"),
        "{text}"
    );
}

#[test]
fn cli_set_values() {
    let mut c = Config::default();
    assert!(cli::set(&mut c, "screen-ac", "next", true).is_ok());
    assert_eq!(c.screen_ac, 900);
    cli::set(&mut c, "lid", "shutdown", true).unwrap();
    assert_eq!(c.lid, Action::Shutdown);
    cli::set(&mut c, "button", "next", true).unwrap();
    assert_eq!(c.button, Some(Action::Hibernate));
    cli::set(&mut c, "brightness", "+", true).unwrap();
    assert_eq!(c.brightness, 90);
    cli::set(&mut c, "brightness", "1", true).unwrap();
    assert_eq!(c.brightness, 5);
    assert!(cli::set(&mut c, "lid", "explode", true).is_err());
    assert!(cli::set(&mut c, "unknown", "1", true).is_err());
}
