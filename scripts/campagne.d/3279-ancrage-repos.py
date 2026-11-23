# Ancrage au repos (#3279) : winlux-snap watch ne se réveille plus souris
# immobile (événements XInput2 bruts), et garde le suivi toutes les 100 ms
# sans XInput 2.2. Sous openbox et Xvfb quand ils sont là.
_xvfb = __import__("shutil").which("xvfb-run")
_r = subprocess.run(([_xvfb, "-a", "-s", "-screen 0 1366x768x24"] if _xvfb else [])
                    + [sys.executable, "-B", str(ROOT / "scripts/test-ancrage-repos.py")],
                    capture_output=True, text=True, timeout=180)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "ancrage au repos : OK" in _r.stdout,
   "ancrage : aucun réveil souris immobile, suivi gardé sans XInput 2 (#3279)")
