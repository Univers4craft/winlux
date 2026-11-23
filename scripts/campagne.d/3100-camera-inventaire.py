# #3100 : lister les caméras (MFEnumDeviceSources) ne pose aucune question et
# revient tout de suite, même si la fenêtre de demande mettrait 10 s à
# répondre ; un refus mémorisé reste un refus. Lister n'est pas utiliser
# (#1733). Vrai runtime et vrai PE, faux assistant de demande.
_r = subprocess.run(["sh", str(SCRIPTS / "test-camera-inventaire.sh")],
                    capture_output=True, text=True, timeout=300)
if _r.returncode != 0:
    print(_r.stdout[-3000:], _r.stderr[-3000:])
ok(_r.returncode == 0 and ("sans rien demander" in _r.stdout or "test sauté" in _r.stdout),
   "inventaire des caméras sans question ni attente au lancement (#3100)")
