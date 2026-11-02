#!/usr/bin/python3
"""« Afficher dans le dossier » (navigateurs, .exe, GTK/Qt) ouvre l'Explorateur.

1. org.freedesktop.FileManager1 : ShowItems, ShowFolders et
   ShowItemProperties, sur un vrai bus de session (dbus-run-session), ouvrent
   l'Explorateur sur le bon dossier (programme remplacé par WINLUX_EXPLORER).
2. Service D-Bus installé à la place de celui de Thunar.
3. inode/directory → winlux-explorer.desktop, partout où GIO et xdg-open
   regardent, et réparation d'un mimeapps.list existant.
"""
import ast
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
SERVICE = ROOT / "integration/native/filemanager/winlux-filemanager1.py"
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)


fm = runpy.run_path(str(SERVICE), run_name="filemanager_test")
work = Path(tempfile.mkdtemp())
try:
    downloads = work / "Téléchargements"
    (downloads / "sous dossier").mkdir(parents=True)
    report = downloads / "rapport final.pdf"
    report.write_text("x")
    uri = report.as_uri()
    folders = fm["folders"]
    check(folders("ShowItems", [uri]) == [downloads], f"ShowItems : {folders('ShowItems', [uri])}")
    check(folders("ShowItems", [str(report)]) == [downloads], "ShowItems avec un chemin")
    check(folders("ShowFolders", [(downloads / "sous dossier").as_uri()]) == [downloads / "sous dossier"], "ShowFolders")
    check(folders("ShowItemProperties", [uri]) == [downloads], "ShowItemProperties")
    check(folders("ShowItems", [(downloads / "effacé.zip").as_uri()]) == [downloads], "élément supprimé : son dossier")
    check(folders("ShowItems", [(work / "parti/loin/x.txt").as_uri()]) == [work], "dossier déplacé : premier parent existant")
    check(folders("ShowItems", ["https://exemple.org/x", "file://autre-machine/x"]) == [], "URI distantes ignorées")
    check(folders("ShowItems", [uri, uri, report.as_uri()]) == [downloads], "sans doublon")
    check(len(folders("ShowFolders", [f"file:///tmp/{i}" for i in range(40)])) <= fm["LIMIT"], "fenêtres limitées")

    # Vrai bus de session : le navigateur appelle ShowItems.
    if shutil.which("dbus-run-session") and shutil.which("gdbus"):
        log = work / "explorer.log"
        fake = work / "fake-explorer"
        fake.write_text(f'#!/bin/sh\nprintf "%s|%s\\n" "$1" "$2" >> "{log}"\n')
        fake.chmod(0o755)
        script = f"""
set -e
WINLUX_EXPLORER={fake} {sys.executable} {SERVICE} &
for i in $(seq 50); do gdbus introspect --session --dest org.freedesktop.FileManager1 --object-path /org/freedesktop/FileManager1 >/dev/null 2>&1 && break; sleep 0.1; done
gdbus call --session --dest org.freedesktop.FileManager1 --object-path /org/freedesktop/FileManager1 --method org.freedesktop.FileManager1.ShowItems "['{uri}']" "" >/dev/null
gdbus call --session --dest org.freedesktop.FileManager1 --object-path /org/freedesktop/FileManager1 --method org.freedesktop.FileManager1.ShowFolders "['{(downloads / 'sous dossier').as_uri()}']" "" >/dev/null
"""
        result = subprocess.run(["dbus-run-session", "--", "sh", "-c", script], capture_output=True, text=True, timeout=60)
        check(result.returncode == 0, f"appel D-Bus : {result.stderr[-400:]}")
        for _ in range(50):
            if log.exists() and len(log.read_text().splitlines()) >= 2:
                break
            time.sleep(0.1)
        lines = log.read_text().splitlines() if log.exists() else []
        check(lines == [f"explorer|{downloads}", f"explorer|{downloads / 'sous dossier'}"], f"Explorateur ouvert sur : {lines}")
    else:
        print("dbus-run-session ou gdbus absent : appel D-Bus sauté")

    # Service D-Bus : le nôtre, Thunar retiré de l'image.
    service = (ROOT / "integration/native/dbus/org.freedesktop.FileManager1.service").read_text()
    check("Name=org.freedesktop.FileManager1" in service, "nom du service")
    check("/usr/share/winlux/native/filemanager/winlux-filemanager1.py" in service, "Exec du service")
    stage = (HERE / "stage-desktop-components.sh").read_text()
    check("org.freedesktop.FileManager1.service\" \"$stage/usr/share/dbus-1/services/" in stage, "service copié dans l'image")
    check("grep -l '^Name=org.freedesktop.FileManager1$'" in stage and 'rm -f "$other"' in stage, "service de Thunar retiré")
    check('cp -a "$project/integration/native/." "$stage/usr/share/winlux/native/"' in stage, "script copié dans l'image")
    check("inode/directory=winlux-explorer.desktop;" in stage and "xfce-mimeapps.list" in stage, "inode/directory dans les listes XDG et XFCE")
    helpers = (ROOT / "integration/native/config/xfce4/helpers.rc").read_text()
    check("FileManager=winlux-explorer" in helpers, "exo-open : gestionnaire de fichiers = Explorateur")
    desktop = (ROOT / "integration/native/applications/winlux-explorer.desktop").read_text()
    check("MimeType=inode/directory;" in desktop and "Exec=/usr/bin/winlux-shell-app explorer %f" in desktop, "lanceur de l'Explorateur")

    # Liste des associations du compte : réparée sans écraser un vrai choix.
    source = (HERE / "winlux-native-config.py").read_text()
    tree = ast.parse(source)
    function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "folder_handler")
    scope = {}
    exec(compile(ast.Module([function], []), "folder_handler", "exec"), scope)
    fix = scope["folder_handler"]
    check("inode/directory=winlux-explorer.desktop;" in fix("[Default Applications]\nx-scheme-handler/http=firefox.desktop\n"),
          "ligne ajoutée à un fichier écrit par le navigateur")
    check(fix("[Default Applications]\ninode/directory=thunar.desktop;\n") == "[Default Applications]\ninode/directory=winlux-explorer.desktop;\n",
          "Thunar remplacé")
    check(fix("[Default Applications]\ninode/directory=org.kde.dolphin.desktop;\n").endswith("dolphin.desktop;\n"), "autre choix gardé")
    check("[Default Applications]" in fix("[Added Associations]\n"), "section créée")
finally:
    shutil.rmtree(work, ignore_errors=True)

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print("OK: « Afficher dans le dossier » ouvre l'Explorateur sur le bon dossier (D-Bus FileManager1, xdg, mimeapps)")
