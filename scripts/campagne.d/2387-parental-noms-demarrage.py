# #2387 : pendant les 10 minutes après le démarrage, l'activité nommait les
# applications par leur programme (« mousepad ») pour toute la journée.
_r = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-parental-noms-demarrage.py")],
                    capture_output=True, text=True, timeout=60)
ok(_r.returncode == 0 and "activité juste après le démarrage : « Mousepad »" in _r.stdout,
   "contrôle parental : noms des applications chargés dès le démarrage du service (#2387)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
