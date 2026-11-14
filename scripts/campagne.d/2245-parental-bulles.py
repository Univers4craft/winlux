# #2245 : les bulles du contrôle parental passaient par notify-send, absent
# de l'image (#630) : aucune ne s'affichait. gdbus, vrai bus privé, faux serveur.
_r = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-parental-bulles.py")],
                    capture_output=True, text=True, timeout=120)
ok(_r.returncode == 0 and "bulle reçue telle quelle" in _r.stdout,
   "contrôle parental : bulles par org.freedesktop.Notifications sans notify-send (#2245)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
