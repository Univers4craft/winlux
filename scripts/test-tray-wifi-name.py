#!/usr/bin/python3
"""#914 : l'infobulle réseau de la barre nomme le réseau Wi-Fi, pas la carte.

wifi_name() est tiré de winlux-tray.py et lancé avec de faux nmcli et iw
placés en tête du PATH : nom de la connexion NetworkManager d'abord, SSID
de iw sans NetworkManager, nom de l'interface sans l'un ni l'autre. Le nom
finit échappé dans <tool>, un SSID avec <&> ne casse pas la sortie genmon."""
import ast
import html
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
source = (HERE / "winlux-tray.py").read_text(encoding="utf-8")
node = next(item for item in ast.parse(source).body if isinstance(item, ast.FunctionDef) and item.name == "wifi_name")
scope = {"subprocess": subprocess}
exec(compile(ast.Module(body=[node], type_ignores=[]), "winlux-tray.py", "exec"), scope)
wifi_name = scope["wifi_name"]

problems = []


def fake(directory, name, output):
    path = Path(directory) / name
    path.write_text(f"#!/bin/sh\nprintf '%s\\n' '{output}'\n")
    path.chmod(0o755)


def check(label, tools, expected):
    with tempfile.TemporaryDirectory() as directory:
        for name, output in tools.items():
            fake(directory, name, output)
        old = os.environ["PATH"]
        # Seuls les faux outils et sh sont joignables : pas de vrai nmcli ou iw.
        os.environ["PATH"] = directory + ":" + str(Path(directory) / "vide")
        shell = Path(directory) / "sh"
        shell.symlink_to("/bin/sh")
        try:
            got = wifi_name("wlo1")
        finally:
            os.environ["PATH"] = old
    if got != expected:
        problems.append(f"{label} : {got!r} au lieu de {expected!r}")


check("NetworkManager", {"nmcli": "Ma box", "iw": "SSID: Autre"}, "Ma box")
check("nmcli sans connexion (--), repli sur iw", {"nmcli": "--", "iw": "Connected to 00:11:22:33:44:55 (on wlo1)\n\tSSID: Ma box"}, "Ma box")
check("sans NetworkManager", {"iw": "\tSSID: Ma box"}, "Ma box")
check("ni nmcli ni iw", {}, "wlo1")
check("caractères spéciaux", {"nmcli": "<Box & co>"}, "<Box & co>")

tool = html.escape("Wi-Fi : " + "<Box & co>")
if "<B" in tool or "& " in tool:
    problems.append(f"infobulle non échappée : {tool}")
if "names = [wifi_name(path.name) if wireless else path.name for path in active]" not in source:
    problems.append("network_part n'appelle pas wifi_name pour le Wi-Fi")
if "<tool>{html.escape(tooltip)}</tool>" not in source:
    problems.append("l'infobulle n'est plus échappée avant genmon")

for problem in problems:
    print("ÉCHEC :", problem)
if problems:
    sys.exit(1)
print("Barre : infobulle Wi-Fi au nom du réseau (nmcli, iw, repli sur l'interface) (#914).")
