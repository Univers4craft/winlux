#!/usr/bin/python3
"""Assistant d'installation : étapes sautées hors de la ligne d'état (#2895).

Sous Xvfb, l'assistant entier devant un faux runtime qui écrit une ligne
d'avancement, puis la ligne de journal d'un programme non lancé (« setup :
étape sautée : programme ignoré : rundll32… », comme Notepad++ 8.9.9.1),
puis une autre ligne d'avancement. Attendu :
- la ligne d'état ne montre jamais l'étape sautée, seulement l'avancement ;
- la page de fin la résume en clair (« Winlux a sauté 1 étape… ») ;
- la ligne entière reste dans le journal (sortie de l'assistant) ;
- la fenêtre garde sa largeur jusqu'à la fin (#2893).
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
SAUTEE = ('setup : étape sautée : programme ignoré : "C:\\\\windows\\\\System32\\\\rundll32.exe" '
          '"C:\\\\Program Files\\\\Exemple\\\\ext.dll",CleanupDll (outil système Windows : rundll32.exe)')
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


with tempfile.TemporaryDirectory() as work:
    work = Path(work)
    fake = work / "winlux"
    fake.write_text(f"""#!/bin/sh
case "$1 $2" in
    "--setup --info") printf 'nsis\\tExemple\\t1.0\\tÉditeur\\t\\n'; exit 0 ;;
    "--setup --license") exit 1 ;;
esac
echo "setup : copie des fichiers"
sleep 1
echo '{SAUTEE}'
sleep 1
echo "setup : raccourcis du menu Démarrer"
sleep 1
exit 0
""")
    fake.chmod(0o755)
    exe = work / "exemple-setup.exe"
    exe.write_bytes(b"MZ")
    home = work / "home"
    home.mkdir()
    trace = work / "trace.txt"
    env = dict(os.environ, HOME=str(home), WINLUX_DRIVE_C=str(home), WINLUX_LANG="fr",
               WINLUX_SETUP_WIZARD_AUTO="1", WINLUX_BIN=str(fake), NO_AT_BRIDGE="1",
               WINLUX_SETUP_WIZARD_TRACE=str(trace))
    proc = subprocess.Popen([sys.executable, str(WIZARD), str(exe)], env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
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
    output = proc.communicate()[0]
    check(proc.returncode == 0, f"code {proc.returncode}")
    lines = trace.read_text(encoding="utf-8").splitlines() if trace.exists() else []
    etats = [l for l in lines if l.startswith("état : ")]
    check("état : setup : copie des fichiers" in etats, f"avancement non affiché : {etats}")
    check("état : setup : raccourcis du menu Démarrer" in etats, f"avancement non affiché : {etats}")
    check(not any("étape sautée" in l or "rundll32" in l for l in etats),
          f"l'étape sautée est montrée dans la ligne d'état : {etats}")
    fins = [l for l in lines if l.startswith("fin : ")]
    check(any("Winlux a sauté 1 étape" in l for l in fins), f"pas de résumé en fin d'installation : {fins}")
    check(not any("rundll32" in l or ".dll" in l for l in fins), f"résumé technique : {fins}")
    check("détail : 1 ligne(s), volet visible" in lines, f"détail des étapes sautées non atteignable : {lines}")
    check(SAUTEE.replace("\\\\", "\\") in output, "la ligne de l'étape sautée manque au journal")
    check(len(widths) >= 5, f"fenêtre peu ou pas vue ({len(widths)} relevés)")
    if widths:
        check(max(widths) <= widths[0] + 8, f"la fenêtre s'est élargie de {widths[0]} à {max(widths)} px")

if failures:
    sys.exit(1)
print("Assistant : étape sautée hors de la ligne d'état, résumée en fin, gardée au journal (#2895)")
