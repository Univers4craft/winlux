# Avant la veille, chaque session ouverte est verrouillée avec son compte,
# pas seulement :0 : le bureau d'une seconde session n'est plus visible au
# réveil (#2152).
_s = (ROOT / "runtime/src/power/sleep.rs").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/power/tests.rs").read_text(encoding="utf-8")
ok("pub fn lock_targets_at(" in _s and "fn verrouillage_de_toutes_les_sessions_avant_la_veille(" in _t, "écrans de session lus dans /run/winlux/sessions avant la veille (#2152)")
# Le même test lance un faux winlux-lock qui note DISPLAY et WINLUX_SESSION_USER et pose le marqueur.
ok("fn lock_before_sleep(" in _s and "pub fn lock_shown_marker(" in _s and '"WINLUX_SESSION_USER"' in _s,
   "winlux-lock lancé par écran avec son compte, attente de chaque marqueur shown (#2152)")
