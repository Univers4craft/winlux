#!/usr/bin/python3
"""Tests hors ligne de winlux-permission : mémoire des choix (même format que
runtime/src/permissions.rs), révocation, ligne de commande et textes des
demandes dans les quatre langues. Aucune fenêtre n'est ouverte."""
import contextlib
import io
import os
from pathlib import Path
import runpy
import tempfile

tmp = Path(tempfile.mkdtemp())
os.environ["WINLUX_PERMISSIONS_FILE"] = str(tmp / "permissions.tsv")
os.environ["WINLUX_LANG"] = "fr"
perm = runpy.run_path(str(Path(__file__).with_name("winlux-permission.py")), run_name="permission_test")

zoom = "/home/a/Téléchargements/Zoom/Zoom.exe"
jeu = "/home/a/Jeux/jeu.exe"
assert perm["load"]() == []
perm["set_decision"]("camera", "allow", zoom)
perm["set_decision"]("microphone", "deny", zoom)
perm["set_decision"]("network", "allow", jeu)
# Changer d'avis remplace la décision au lieu d'en ajouter une.
perm["set_decision"]("camera", "deny", zoom)
text = (tmp / "permissions.tsv").read_text()
assert text.count("camera\t") == 1 and f"camera\tdeny\t{zoom}\n" in text, text
apps = perm["by_app"]()
assert list(apps) == [jeu, zoom], apps
assert apps[zoom] == {"camera": "deny", "microphone": "deny"}
# Lignes écrites par le runtime (dernière ligne gagnante) et lignes abîmées.
with open(tmp / "permissions.tsv", "a") as f:
    f.write(f"screen\tallow\t{zoom}\nscreen\tdeny\t{zoom}\npeut-etre\nfiles\tsans-doute\t{jeu}\n")
assert perm["by_app"]()[zoom]["screen"] == "deny"
assert "files" not in perm["by_app"]()[jeu]
# « Tout redemander » oublie seulement cette application.
perm["forget"](zoom)
assert list(perm["by_app"]()) == [jeu]
# Ligne de commande utilisée par Paramètres.
main = perm["main"]
assert main(["winlux-permission", "set", "files", "allow", zoom]) == 0
with contextlib.redirect_stderr(io.StringIO()):
    assert main(["winlux-permission", "set", "disque", "allow", zoom]) == 3
    assert main(["winlux-permission", "set", "files", "peut-etre", zoom]) == 3
out = io.StringIO()
with contextlib.redirect_stdout(out):
    assert main(["winlux-permission", "list"]) == 0
assert f"files\tallow\t{zoom}" in out.getvalue().splitlines()
assert main(["winlux-permission", "forget", zoom]) == 0
with contextlib.redirect_stderr(io.StringIO()):
    assert main(["winlux-permission", "ask", "disque", "x", "y"]) == 3
# Chaque capacité a sa question, et chaque texte ses trois traductions.
assert set(perm["QUESTIONS"]) == set(perm["CAPABILITIES"])
for question, detail, _icon in perm["QUESTIONS"].values():
    assert question in perm["_TR"] and detail in perm["_TR"], question
    assert "{app}" in question and all("{app}" in row for row in perm["_TR"][question])
assert all(len(row) == 3 and all(row) for row in perm["_TR"].values())
print("OK: winlux-permission (mémoire des choix, révocation, ligne de commande, langues)")
