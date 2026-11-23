# #3025 : l'Explorateur ouvert par un chemin Linux du dossier personnel
# (xdg-open, GIO, lanceur) part du lecteur C: et affiche « Dossier personnel
# (C:) › Documents », comme depuis le volet de gauche ; hors du dossier
# personnel, le chemin reste sur Z:. Cas détaillés : scripts/test-shell-app-uri.py.
def _chemin_3025(valeur):
    env = {k: v for k, v in os.environ.items() if k not in ("WINLUX_DRIVE_C", "WINLUX_HOME")}
    env["HOME"] = "/home/damien"
    return subprocess.run(["sh", str(SCRIPTS / "winlux-shell-app.sh"), "--chemin", valeur],
                          capture_output=True, env=env).stdout.decode(errors="replace")
_vus_3025 = [_chemin_3025(v) for v in ("/home/damien/Documents/banc2-explo", "/home/damien", "/tmp/x")]
print(f"      chemins transmis : {_vus_3025}")
ok(_vus_3025 == ["C:\\Documents\\banc2-explo", "C:\\", "Z:\\tmp\\x"],
   "Explorateur : un chemin Linux du dossier personnel s'ouvre sur Dossier personnel (C:), le reste sur Z: (#3025)")
