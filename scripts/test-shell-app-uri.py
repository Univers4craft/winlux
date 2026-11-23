#!/usr/bin/env python3
"""winlux-shell-app explorer : une URI file:// (xdg-open, GIO) est décodée en
chemin avant d'ouvrir l'Explorateur (accents, espaces, caractères réservés)."""
import os
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "winlux-shell-app.sh"
failures = 0


def resolve(value, home="/home/essai-winlux", **extra):
    # Dossier personnel fixé : un chemin qui y est devient C:\… (#3025).
    env = {key: item for key, item in os.environ.items() if key not in ("WINLUX_DRIVE_C", "WINLUX_HOME")}
    env.update(HOME=home, **extra)
    run = subprocess.run(["sh", str(SCRIPT), "--chemin", value], capture_output=True, env=env)
    return run.returncode, run.stdout


def ok(cond, label):
    global failures
    print(("  OK     " if cond else "  ECHEC  ") + label)
    failures += not cond


CASES = [
    ("file:///home/damien/T%C3%A9l%C3%A9chargements", "Z:\\home\\damien\\Téléchargements",
     "Accents en pourcentage décodés (Téléchargements)"),
    ("file:///home/damien/Mes%20documents/", "Z:\\home\\damien\\Mes documents\\", "Espaces %20 décodés"),
    ("file:///tmp/a%23b%3Fc%25d%26e%2Bf", "Z:\\tmp\\a#b?c%d&e+f", "Caractères réservés # ? % & + décodés"),
    ("file://localhost/tmp/x", "Z:\\tmp\\x", "file://localhost/ accepté"),
    ("file:/tmp/x", "Z:\\tmp\\x", "file:/chemin accepté"),
    ("FILE:///tmp/x", "Z:\\tmp\\x", "Schéma insensible à la casse"),
    ("file:///home/d/été brut", "Z:\\home\\d\\été brut", "URI non encodée gardée telle quelle"),
    ("/home/damien/Téléchargements", "Z:\\home\\damien\\Téléchargements", "Chemin Linux inchangé"),
    ("C:\\Users\\damien", "C:\\Users\\damien", "Chemin Windows inchangé"),
]
for value, expected, label in CASES:
    code, out = resolve(value)
    ok(code == 0 and out == expected.encode(), f"{label} : {out.decode(errors='replace')!r}")

code, out = resolve("file:///tmp/%E9t%E9")
ok(code == 0 and out == b"Z:\\tmp\\\xe9t\xe9", "Octets non UTF-8 transmis sans perte")
ok(not out.startswith(b"e:"), "Le début de l'URI n'est plus pris pour une lettre de lecteur (« e:/// »)")
ok(resolve("file://autre-poste/partage")[0] != 0, "URI d'un autre poste refusée")
ok(resolve("file:///tmp/a%00b")[0] != 0, "Octet nul refusé")

# #3025 : un chemin du dossier personnel (lecteur C:) s'ouvre sur C:, comme
# depuis le volet de gauche, et non sur « Système Linux (Z:) › home › … ».
HOME_CASES = [
    ("/home/damien/Documents/banc2-explo", {}, "C:\\Documents\\banc2-explo", "Dossier du compte sur C: (#3025)"),
    ("file:///home/damien/T%C3%A9l%C3%A9chargements", {}, "C:\\Téléchargements", "URI du compte sur C: (#3025)"),
    ("/home/damien", {}, "C:\\", "Dossier personnel lui-même : C:\\ (#3025)"),
    ("/home/damien/", {}, "C:\\", "Dossier personnel avec barre finale : C:\\ (#3025)"),
    ("/home/damienX/a", {}, "Z:\\home\\damienX\\a", "Dossier voisin au nom plus long : reste sur Z: (#3025)"),
    ("/srv/c/a", {"WINLUX_DRIVE_C": "/srv/c"}, "C:\\a", "WINLUX_DRIVE_C suivi comme dans le runtime (#3025)"),
    ("/home/x", {"HOME": "/"}, "Z:\\home\\x", "Dossier personnel « / » : tout reste sur Z: (#3025)"),
    ("", {}, "", "Chemin vide : rien (#3025)"),
]
for value, extra, expected, label in HOME_CASES:
    code, out = resolve(value, **{"home": "/home/damien", **{("home" if k == "HOME" else k): v for k, v in extra.items()}})
    ok(code == 0 and out == expected.encode(), f"{label} : {out.decode(errors='replace')!r}")

desktop = (Path(__file__).resolve().parent.parent / "integration/native/applications/winlux-explorer.desktop").read_text()
ok("winlux-shell-app explorer %f" in desktop, "Le lanceur de l'Explorateur passe par winlux-shell-app")

# #1027 : le Bloc-notes du shell PE s'ouvre seul, avec un fichier facultatif.
script = SCRIPT.read_text(encoding="utf-8")
notepad = script[script.index("    notepad)"):script.index("    editor)")]
ok("export WINLUX_SHELL_APP=notepad" in notepad and 'shell_path "$2"' in notepad
   and "gui_demo.exe" in notepad, "winlux-shell-app notepad ouvre le Bloc-notes du shell PE (#1027)")
wm = (SCRIPT.parent.parent / "testexe/src/shell/wm.rs").read_text(encoding="utf-8")
ok('b"notepad"' in wm and "notepad::load_file_into_active" in wm, "WINLUX_SHELL_APP=notepad reconnu par le shell PE (#1027)")

if failures:
    sys.exit(f"test-shell-app-uri : {failures} échec(s)")
print(f"test-shell-app-uri : {len(CASES) + len(HOME_CASES) + 7} vérifications OK")
