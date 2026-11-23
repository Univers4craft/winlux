# Bureau au repos (#3130) : l'horloge ne lance plus qu'un programme (date),
# la surveillance des entrées aucun, celle du réseau ni ip ni grep.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-repos-boucles.py")],
                    capture_output=True, text=True, timeout=300)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "boucles au repos : OK" in _r.stdout,
   "bureau au repos : horloge, entrées, réseau et session sans programme lancé à chaque tour (#3130)")
