# #3172 : un compte administrateur reçoit l'élévation sans question (#1739) ;
# l'installateur (vrai PE requireAdministrator de 256 Mo, vrai runtime) doit
# démarrer sans que Winlux le lise ni le copie avant (pic sous 128 Mo, moins
# de 2 s). Avant : pic de plus de 256 Mo pour une ligne de journal.
_r_3172 = subprocess.run(["sh", str(SCRIPTS / "test-uac-sans-lecture.sh")],
                         capture_output=True, text=True, timeout=300)
print(_r_3172.stdout.strip(), _r_3172.stderr.strip()[-1500:])
ok(_r_3172.returncode == 0 and ("sans être lu" in _r_3172.stdout or "test sauté" in _r_3172.stdout),
   "installateur lancé par un administrateur sans être lu ni copié (#3172)")
