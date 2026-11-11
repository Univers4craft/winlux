#!/usr/bin/python3
"""Tests hors ligne de winlux-permission : mémoire des choix (même format que
runtime/src/permissions.rs), révocation, ligne de commande et textes des
demandes dans les quatre langues. Aucune fenêtre n'est ouverte."""
import contextlib
import io
import os
from pathlib import Path
import runpy
import sys
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
# La fenêtre mémorise elle-même la réponse (le programme a pu quitter).
paint = "/home/a/Applications/paint.exe"
assert perm["stored"]("files", paint) is None
perm["remember_answer"]("files", paint, 0)
assert perm["stored"]("files", paint) == "allow"
perm["remember_answer"]("camera", paint, 2)  # fermée sans réponse : rien
assert perm["stored"]("camera", paint) is None
# Réponse déjà connue : pas de fenêtre, la réponse revient tout de suite
# (aucune bibliothèque graphique n'est chargée ici).
assert main(["winlux-permission", "ask", "files", "paint.exe", paint]) == 0
perm["set_decision"]("files", "deny", paint)
assert main(["winlux-permission", "ask", "files", "paint.exe", paint]) == 1
# Programme demandeur toujours là : la fenêtre reste ; parti : elle se ferme.
import subprocess
import time
me = (os.getpid(), perm["process_start"](os.getpid()))
assert me[1] is not None
assert perm["watch"]("camera", paint, me) is None
child = subprocess.Popen(["sleep", "30"])
alive = (child.pid, perm["process_start"](child.pid))
assert alive[1] is not None and perm["watch"]("camera", paint, alive) is None
child.kill()
child.wait()
assert perm["process_start"](child.pid) is None
assert perm["watch"]("camera", paint, alive) == 2
# Un numéro de processus réutilisé par un autre programme compte comme parti.
assert perm["watch"]("camera", paint, (os.getpid(), "1")) == 2
# Une autre fenêtre du même programme a répondu : celle-ci prend sa réponse.
perm["set_decision"]("camera", "allow", paint)
assert perm["watch"]("camera", paint, me) == 0
os.environ["WINLUX_PERMISSION_PARENT"] = str(child.pid)
assert perm["requester"]() == (child.pid, None)
del os.environ["WINLUX_PERMISSION_PARENT"]
assert perm["requester"]()[0] == os.getppid()
perm["forget"](paint)
# Chaque capacité a sa question, et chaque texte ses trois traductions.
assert set(perm["QUESTIONS"]) == set(perm["CAPABILITIES"])
for question, detail, _icon in perm["QUESTIONS"].values():
    assert question in perm["_TR"] and detail in perm["_TR"], question
    assert "{app}" in question and all("{app}" in row for row in perm["_TR"][question])
assert all(len(row) == 3 and all(row) for row in perm["_TR"].values())
# #1592 : la fenêtre ouverte, une ligne sur stderr pour le terminal qui attend.
err = io.StringIO()
perm["notice"]("files", "python.exe", err)
line = err.getvalue()
assert line.startswith("winlux : python.exe attend votre réponse dans une fenêtre.") and line.endswith("s.\n"), line
assert "Autoriser « python.exe » à accéder à vos fichiers personnels ?" in line and "120 s" in line, line
key = "{app} attend votre réponse dans une fenêtre. {question} Sans réponse : refus pour cette fois dans {delai} s."
assert all("{question}" in row and "{delai}" in row for row in perm["_TR"][key])
print("OK: winlux-permission (mémoire des choix, fenêtre fermée si le programme part, réponse enregistrée, langues)")

# #1038 : affichage injoignable -> code 2 (refus pour cette fois), rien de mémorisé.
# Avant, le plantage sur Gdk.Screen.get_default() rendait 1, lu comme « Refuser ».
try:
    import gi  # noqa: F401
    has_gtk = True
except ImportError:
    has_gtk = False
if has_gtk:
    lonely = "/home/a/Applications/seul.exe"
    env = dict(os.environ, DISPLAY=":93", WAYLAND_DISPLAY="")
    code = subprocess.run([sys.executable, str(Path(__file__).with_name("winlux-permission.py")), "ask", "files",
                           "seul.exe", lonely], env=env, capture_output=True, timeout=60).returncode
    assert code == 2, code
    assert perm["stored"]("files", lonely) is None
    print("OK: affichage injoignable, refus pour cette fois sans mémoriser (#1038)")
