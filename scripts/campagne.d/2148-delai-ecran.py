# Le délai « Éteindre l'écran » de Paramètres > Alimentation est le seul qui
# compte : le serveur X n'éteint plus l'écran de lui-même à 10 min (#2148).
_d = (ROOT / "runtime/src/power/daemon.rs").read_text(encoding="utf-8")
_x = (ROOT / "integration/native/xscreensaver").read_text(encoding="utf-8")
_n = (ROOT / "scripts/winlux-native-config.py").read_text(encoding="utf-8")
ok("fn hold_server_dpms" in _d and "dpms_set_timeouts(0, 0, 0)" in _d and "x_idle.hold_server_dpms();" in _d,
   "démon d'alimentation : délais DPMS du serveur X remis à zéro (#2148)")
ok(_x.count("18:12:15") == 3 and "0:10:00\ndpms" not in _x, "modèle ~/.xscreensaver : délais DPMS au maximum (#2148)")
ok('value(line) == "0:10:00"' in _n, "anciens délais DPMS 0:10:00 des comptes existants migrés (#2148)")
