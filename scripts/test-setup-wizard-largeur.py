#!/usr/bin/python3
"""Assistant d'installation : largeur fixe pendant l'installation (#2893).

Sous Xvfb, l'assistant entier devant un faux runtime qui écrit une ligne
d'état courte, puis une ligne de 400 caractères (comme la note de Notepad++
8.9.9.1 sur rundll32). La fenêtre doit garder sa largeur : avant #2893 elle
suivait la ligne d'état et sortait de l'écran (853 → 1440 px sur le banc).
La ligne entière reste lisible en infobulle de la ligne d'état.
"""
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
WIZARD = HERE / "winlux-setup-wizard.py"
LONGUE = "setup : programme ignoré : " + " ".join(["C:\\\\Program Files\\\\Exemple\\\\outil.dll"] * 12)

with tempfile.TemporaryDirectory() as work:
    work = Path(work)
    fake = work / "winlux"
    fake.write_text(f"""#!/bin/sh
case "$1 $2" in
    "--setup --info") printf 'nsis\\tExemple\\t1.0\\tÉditeur\\t\\n'; exit 0 ;;
    "--setup --license") exit 1 ;;
esac
echo "setup : copie des fichiers"
sleep 2
echo '{LONGUE}'
sleep 3
echo "setup : Exemple est installé."
exit 0
""")
    fake.chmod(0o755)
    exe = work / "exemple-setup.exe"
    exe.write_bytes(b"MZ")
    home = work / "home"
    home.mkdir()
    env = dict(os.environ, HOME=str(home), WINLUX_DRIVE_C=str(home), WINLUX_LANG="fr",
               WINLUX_SETUP_WIZARD_AUTO="1", WINLUX_BIN=str(fake), NO_AT_BRIDGE="1")
    proc = subprocess.Popen([sys.executable, str(WIZARD), str(exe)], env=env,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    widths = []
    deadline = time.time() + 60
    while proc.poll() is None and time.time() < deadline:
        out = subprocess.run(["xwininfo", "-name", "Installation de Exemple"],
                             capture_output=True, text=True).stdout
        m = re.search(r"Width: (\d+)", out)
        if m:
            widths.append(int(m.group(1)))
        time.sleep(0.2)
    if proc.poll() is None:
        proc.kill()
        print("ÉCHEC : l'assistant ne s'est pas terminé", file=sys.stderr)
        sys.exit(1)

if len(widths) < 10:
    print(f"ÉCHEC : fenêtre peu ou pas vue ({len(widths)} relevés)", file=sys.stderr)
    sys.exit(1)
if max(widths) > widths[0] + 8:
    print(f"ÉCHEC : la fenêtre s'est élargie de {widths[0]} à {max(widths)} px", file=sys.stderr)
    sys.exit(1)
print(f"Assistant : largeur fixe ({widths[0]} px) malgré une ligne d'état de {len(LONGUE)} caractères (#2893)")
