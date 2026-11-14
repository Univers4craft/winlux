# Avant la veille, chaque session ouverte est verrouillée avec son compte,
# pas seulement :0 : le bureau d'une seconde session n'est plus visible au
# réveil (#2152).
_s = (ROOT / "runtime/src/power/sleep.rs").read_text(encoding="utf-8")
ok("pub fn lock_targets_at" in _s and 'join("sessions")' in _s, "écrans de session lus dans /run/winlux/sessions avant la veille (#2152)")
ok('command.env("WINLUX_SESSION_USER", account)' in _s and "waiting.iter().all(|shown| shown.is_file())" in _s,
   "winlux-lock lancé par écran avec son compte, attente de chaque marqueur shown (#2152)")
