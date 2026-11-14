# #2246 : un service de contrôle parental relancé ne dégelait jamais la
# session gelée par le précédent (état gardé en mémoire seulement).
_r = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-parental-reprise.py")],
                    capture_output=True, text=True, timeout=120)
ok(_r.returncode == 0 and "le compte gelé par le précédent est dégelé" in _r.stdout,
   "contrôle parental : service relancé, compte gelé repris et écran orphelin fermé (#2246)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
