# Init au repos (#3312) : la garde du bureau ne lance plus qu'un programme par
# seconde (le sleep) au lieu de quatre ; déconnexion et redémarrage inchangés.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-init-repos.py")],
                    capture_output=True, text=True, timeout=120)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "init au repos : OK" in _r.stdout,
   "garde du bureau par l'init : un seul programme par tour, actions inchangées (#3312)")
