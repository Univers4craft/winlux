# Changer d'utilisateur : l'inactivité et l'extinction de l'écran suivent tous
# les écrans X des sessions ouvertes, pas seulement :0 (#2149).
_d = (ROOT / "runtime/src/power/daemon.rs").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/power/tests.rs").read_text(encoding="utf-8")
ok("pub fn session_displays_at(" in _d and "fn ecrans_des_sessions_du_registre(" in _t, "écrans des sessions lus dans /run/winlux/sessions (#2149)")
ok("fn secs(" in _d and "fn force_dpms(" in _d, "plus petite inactivité de toutes les sessions, écran éteint et rallumé sur chacune (#2149)")
