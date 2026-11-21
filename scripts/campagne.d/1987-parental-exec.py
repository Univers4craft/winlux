# test-parental.py : le faux jeu.exe n'est soumis à enforce_apps qu'une fois son exec fini (#1987).
_p = (ROOT / "scripts/test-parental.py").read_text(encoding="utf-8")
# Le test lui-même porte l'attente, annotée par son numéro.
ok("(#1987)" in _p,
   "test-parental : attente de /proc/PID/cmdline avant enforce_apps (#1987)")
