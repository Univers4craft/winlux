# Session au repos (#3297) : la garde de xfdesktop attend par wait et SIGCHLD,
# un seul sleep toutes les 10 s au lieu d'un par seconde.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-session-attente.py")],
                    capture_output=True, text=True, timeout=300)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "attente de session : OK" in _r.stdout,
   "session au repos : garde du bureau sans sleep chaque seconde (#3297)")
