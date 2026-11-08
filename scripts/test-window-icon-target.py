#!/usr/bin/python3
"""#915 : une fenêtre .exe reçoit l'icône de son lanceur du menu Démarrer.

window_target() et launcher_icon() sont tirés de winlux-applications.py :
chemin relatif au dossier du processus, chemin Windows C:\\… ou Z:\\…, et
repli sur l'Icon= du lanceur winlux-pe-*.desktop de la même cible quand
l'extraction de la fenêtre ne donne que l'icône générique."""
import ast
import os
import re
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
source = (HERE / "winlux-applications.py").read_text(encoding="utf-8")
wanted = {"windows_path", "window_target", "launcher_icon"}
nodes = [item for item in ast.parse(source).body if isinstance(item, ast.FunctionDef) and item.name in wanted]
problems = []
if {node.name for node in nodes} != wanted:
    print("ÉCHEC : fonctions absentes", wanted - {node.name for node in nodes})
    sys.exit(1)

with tempfile.TemporaryDirectory() as directory:
    home = Path(directory) / "home"
    apps = home / ".local/share/applications"
    program = home / "Program Files/Notepad++"
    program.mkdir(parents=True)
    apps.mkdir(parents=True)
    exe = program / "notepad++.exe"
    exe.write_bytes(b"MZ")
    icon = home / ".cache/winlux/icons/abc.ico"
    icon.parent.mkdir(parents=True)
    icon.write_bytes(b"\0")
    (apps / "winlux-pe-abc.desktop").write_text(
        f"[Desktop Entry]\nName=Notepad++\nIcon={icon}\nX-Winlux-Executable={exe}\n")
    (apps / "winlux-pe-autre.desktop").write_text(
        f"[Desktop Entry]\nName=Autre\nIcon={icon}\nX-Winlux-Executable={home}/autre.exe\n")
    scope = {"Path": Path, "os": os, "re": re, "HOME": home, "APPS": apps}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), "winlux-applications.py", "exec"), scope)
    window_target, launcher_icon = scope["window_target"], scope["launcher_icon"]
    pid = str(os.getpid())
    old = os.getcwd()
    os.chdir(program)
    try:
        cases = {
            "chemin absolu": [b"/usr/bin/winlux", os.fsencode(exe)],
            "chemin relatif au dossier du processus": [b"/usr/bin/winlux", b"./notepad++.exe"],
            "chemin Windows C:\\": [b"/usr/bin/winlux", b"C:\\Program Files\\Notepad++\\notepad++.exe"],
        }
        for label, args in cases.items():
            target = window_target(pid, args)
            if target is None or target.resolve() != exe.resolve():
                problems.append(f"{label} : {target} au lieu de {exe}")
    finally:
        os.chdir(old)
    if window_target(pid, [b"/usr/bin/winlux", b"--regedit"]) is not None:
        problems.append("sans .exe, une cible a été inventée")
    if window_target(pid, [b"/usr/bin/winlux", b"Z:\\opt\\x.exe"]) != Path("/opt/x.exe"):
        problems.append("Z:\\ ne mène pas à la racine Linux")
    if launcher_icon(exe) != str(icon):
        problems.append(f"icône du lanceur non reprise : {launcher_icon(exe)}")
    if launcher_icon(home / "inconnu.exe") is not None:
        problems.append("icône reprise d'un lanceur d'un autre .exe")

if "icon = launcher_icon(target) or icon" not in source:
    problems.append("window_icons ne se replie pas sur l'icône du lanceur")

for problem in problems:
    print("ÉCHEC :", problem)
if problems:
    sys.exit(1)
print("Fenêtres .exe : cible relative ou Windows retrouvée, icône du lanceur reprise (#915).")
