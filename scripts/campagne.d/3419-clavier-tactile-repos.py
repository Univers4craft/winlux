# Icône du clavier tactile au repos (#3419) : même ligne que winlux-osk tray,
# servie par winlux-tray serve et recopiée par le lecteur sans Python.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-clavier-tactile-repos.py")],
                    capture_output=True, text=True, timeout=180)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "clavier tactile au repos : OK" in _r.stdout,
   "icône du clavier tactile servie par winlux-tray serve, sans Python toutes les 5 s (#3419)")
