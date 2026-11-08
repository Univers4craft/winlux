#!/usr/bin/env python3
"""winlux-shell-app explorer : une URI file:// (xdg-open, GIO) est décodée en
chemin avant d'ouvrir l'Explorateur (accents, espaces, caractères réservés)."""
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "winlux-shell-app.sh"
failures = 0


def resolve(value):
    run = subprocess.run(["sh", str(SCRIPT), "--chemin", value], capture_output=True)
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
print(f"test-shell-app-uri : {len(CASES) + 7} vérifications OK")
