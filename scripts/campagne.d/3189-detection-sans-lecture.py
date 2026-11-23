# #3189 : au lancement d'un .exe, la détection des installateurs (Inno,
# NSIS, 7z) ne parcourt plus tout le fichier. Vrai runtime : un programme
# ordinaire de 256 Mo démarre avec un pic sous 128 Mo (avant : 265 Mo), et
# de vrais installateurs (NSIS de makensis, 7z SFX, Inno en ressource 11111
# ou pointeur 0x30) suivis de 32 Mo restent reconnus.
_r_3189 = subprocess.run(["sh", str(SCRIPTS / "test-detection-sans-lecture.sh")],
                         capture_output=True, text=True, timeout=600)
print(_r_3189.stdout.strip(), _r_3189.stderr.strip()[-1500:])
ok(_r_3189.returncode == 0 and ("sans lire tout le fichier" in _r_3189.stdout or "test sauté" in _r_3189.stdout),
   "programme ordinaire lancé sans être parcouru, installateurs toujours reconnus (#3189)")
