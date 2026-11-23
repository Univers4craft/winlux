# #3315 : un panneau fermé arrête son minuteur de rafraîchissement.
_r_3315 = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-popup-repos.py")], capture_output=True, text=True, timeout=200)
print(_r_3315.stdout + _r_3315.stderr)
ok(_r_3315.returncode == 0, "panneaux de la barre des tâches : aucun réveil chaque seconde panneau fermé (#3315)")
