# reboot, poweroff, halt et shutdown en ligne de commande arrêtent vraiment la machine (#2273).
_s = (ROOT / "scripts/winlux-systemctl.sh").read_text(encoding="utf-8")
ok('invoked=${0##*/}' in _s and "winlux-power execute" in _s, "systemctl : nom d'appel reconnu, arrêt confié à winlux-power (#2273)")
ok((ROOT / "scripts/test-all.d/2273-reboot-ligne-de-commande.sh").exists(), "test hors ligne de reboot/poweroff/halt/shutdown (#2273)")
