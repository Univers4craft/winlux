# Le délai « Éteindre l'écran » de Paramètres > Alimentation est le seul qui
# compte : le serveur X n'éteint plus l'écran de lui-même à 10 min (#2148).
_d = (ROOT / "runtime/src/power/daemon.rs").read_text(encoding="utf-8")
_x = (ROOT / "integration/native/xscreensaver").read_text(encoding="utf-8")
_n = (ROOT / "scripts/winlux-native-config.py").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/power/tests.rs").read_text(encoding="utf-8")
ok("fn hold_server_dpms(" in _d and "pub fn server_dpms_needs_reset(" in _d and "fn delais_dpms_du_serveur_remis_a_zero(" in _t,
   "démon d'alimentation : délais DPMS du serveur X remis à zéro (#2148)")
ok(_x.count("18:12:15") == 3 and "0:10:00\ndpms" not in _x, "modèle ~/.xscreensaver : délais DPMS au maximum (#2148)")
# Migration vérifiée par test-ecran-veille-retour.py (CI) : 0:10:00 des comptes migrés → 18:12:15.
ok("def migrate_xscreensaver(" in _n and "#2148" in (ROOT / "scripts/test-ecran-veille-retour.py").read_text(encoding="utf-8")
   and "scripts/test-ecran-veille-retour.py" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"), "anciens délais DPMS 0:10:00 des comptes existants migrés (#2148)")
