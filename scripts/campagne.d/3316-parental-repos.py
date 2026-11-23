# Contrôle parental au repos (Gain #3316) : sans compte surveillé le service
# ne se réveille plus chaque seconde, un compte surveillé est pris en 10 s au
# plus, et la supervision (même pid) n'a plus tout le programme en mémoire.
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-parental-repos.py")],
                    capture_output=True, text=True, timeout=180)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "contrôle parental au repos : OK" in _r.stdout,
   "contrôle parental au repos : service endormi, supervision allégée et relance gardée (Gain #3316)")
