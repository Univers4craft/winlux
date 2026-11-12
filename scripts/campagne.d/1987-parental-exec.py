# test-parental.py : le faux jeu.exe n'est soumis à enforce_apps qu'une fois son exec fini (#1987).
_p = (ROOT / "scripts/test-parental.py").read_text(encoding="utf-8")
_v = _p[_p.index('victim = subprocess.Popen('):]
ok('b"jeu.exe" in Path(f"/proc/{victim.pid}/cmdline").read_bytes()' in _v[:_v.index("daemon.enforce_apps()")],
   "test-parental : attente de /proc/PID/cmdline avant enforce_apps (#1987)")
