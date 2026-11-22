# #2862 : un programme Windows lancé par winlux-open enregistre ses fichiers
# selon l'umask du compte (0644 sous 022), comme un outil natif ; le dossier
# d'état du lanceur reste en 0700. Faux runtime, vrai lanceur.
_r = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-open-umask.py")],
                    capture_output=True, text=True, timeout=120)
ok(_r.returncode == 0 and "attendu 0644" in _r.stdout,
   "fichiers enregistrés par un programme Windows selon l'umask du compte, pas en 0600 (#2862)")
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
