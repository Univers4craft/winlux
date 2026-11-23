# Démarrage (#3415) : Xorg n'attend que les pilotes de stockage, réseau et
# affichage ; le reste se charge en arrière-plan, et services comme session
# attendent pilotes.done (limite 30 s).
_r = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/test-demarrage-pilotes.py")],
                    capture_output=True, text=True, timeout=300)
print(_r.stdout + _r.stderr)
ok(_r.returncode == 0 and "pilotes au démarrage : OK" in _r.stdout,
   "démarrage : pilotes de l'affichage d'abord, le reste en arrière-plan (#3415)")
