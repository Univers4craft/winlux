# #3099 : une question d'autorisation ne fige plus le fil de l'application.
# Vrai runtime et vrai PE (sendinput_msvc.c) : sans décision, avec une
# fenêtre de demande qui met 10 s à répondre, SendInput est refusé en moins
# de 2 s et la question est posée une fois, à côté ; autorisé, il tape ;
# refusé, il reste refusé. Tests unitaires de la caméra et de la saisie
# dans permissions.rs.
_r = subprocess.run(["sh", str(SCRIPTS / "test-sendinput.sh")],
                    capture_output=True, text=True, timeout=600)
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
ok(_r.returncode == 0 and ("sans attendre la fenêtre de demande" in _r.stdout or "test sauté" in _r.stdout),
   "question d'autorisation posée à côté, l'appel ne l'attend pas, refus inchangé (#3099)")
