#!/usr/bin/env python3
"""Chaque application livrée avec Winlux a son icône Prisme (#667, #681).

Vérifie, sans affichage :
  * l'Icon= de chaque .desktop livré existe dans le thème Winlux (ou est un
    fichier du dépôt installé à cet endroit) : un nom qui ne résout que dans
    Adwaita (help-about, edit-copy…) laisse l'application sans icône Prisme ;
  * les set_icon_name() des fenêtres GTK pointent vers une icône du thème ;
  * les fenêtres des .exe maison (lecteur multimédia, winlux-shell-app <mode>,
    regedit) reçoivent une icône du catalogue (_NET_WM_ICON), qui résout ;
  * le lecteur multimédia montre la même icône partout (.desktop, fenêtre).
"""
import importlib.util
import os
import re
import tempfile
from pathlib import Path

project = Path(__file__).resolve().parent.parent
theme = project / "integration/native/icons"
os.environ["WINLUX_ICON_ROOT"] = str(theme)
spec = importlib.util.spec_from_file_location("catalogue", project / "scripts/winlux-applications.py")
catalogue = importlib.util.module_from_spec(spec)
spec.loader.exec_module(catalogue)

# Fichiers installés à un chemin absolu par stage-desktop-components.sh.
INSTALLED = {
    "/usr/share/winlux/native/": project / "integration/native/",
}

def resolves(icon):
    for prefix, source in INSTALLED.items():
        if icon.startswith(prefix):
            return (source / icon[len(prefix):]).is_file()
    if icon.startswith("/"):
        return False
    return all(catalogue.theme_icon(icon, name) for name in ("Winlux", "Winlux-Sombre"))

missing = []
desktops = sorted(p for p in (project / "integration").rglob("*.desktop"))
assert len(desktops) > 30, desktops
for desktop in desktops:
    text = desktop.read_text(encoding="utf-8")
    icon = re.search(r"^Icon=(.+)$", text, re.M)
    if icon is None:
        missing.append(f"{desktop.relative_to(project)} : pas de ligne Icon=")
    elif not resolves(icon.group(1)):
        missing.append(f"{desktop.relative_to(project)} : Icon={icon.group(1)} absente du thème Winlux")

sources = [*(project / "scripts").glob("winlux*.py"), *(project / "integration/native").rglob("*.py")]
for source in sorted(sources):
    for name in re.findall(r'set_icon_name\("([^"]+)"\)', source.read_text(encoding="utf-8")):
        if not name.endswith("-symbolic") and not resolves(name):
            missing.append(f"{source.relative_to(project)} : set_icon_name(\"{name}\") absente du thème Winlux")
    # winlux-system-tools : icône choisie selon la page.
    for block in re.findall(r"set_icon_name\(\{(.*?)\}\.get", source.read_text(encoding="utf-8"), re.S):
        for name in re.findall(r':\s*"([^"]+)"', block):
            if not resolves(name):
                missing.append(f"{source.relative_to(project)} : icône de page {name} absente du thème Winlux")

shell_app = (project / "scripts/winlux-shell-app.sh").read_text(encoding="utf-8")
modes = set()
for line in re.findall(r"^    ([a-z0-9|]+)\)$", shell_app, re.M):
    modes.update(line.split("|"))
modes.discard("editor")  # Mousepad : sa propre icône
modes.add("regedit")
for mode in sorted(modes):
    name = catalogue.SHELL_APP_ICONS.get(mode)
    if name is None or not resolves(name):
        missing.append(f"winlux-shell-app {mode} : aucune icône de fenêtre (SHELL_APP_ICONS)")
for exe, name in catalogue.BUILTIN_ICONS.items():
    if not resolves(name):
        missing.append(f"{exe} : icône {name} absente du thème Winlux")

assert not missing, "Applications sans icône Prisme :\n  " + "\n  ".join(missing)

player = (project / "integration/winlux-mediaplayer.desktop").read_text(encoding="utf-8")
assert f"Icon={catalogue.BUILTIN_ICONS['mediaplayer.exe']}\n" in player, "le lecteur doit montrer la même icône partout"
assert "StartupWMClass=mediaplayer\n" in player
assert min(catalogue.ICON_SIZES) <= 16 and max(catalogue.ICON_SIZES) >= 48

# Le thème suivi est celui de la session (clair ou sombre), Winlux-Sombre par défaut.
assert catalogue.theme_icon("computer", "Winlux-Sombre").startswith(str(theme / "Winlux-Sombre")) \
    or catalogue.theme_icon("computer", "Winlux-Sombre").startswith(str(theme / "Winlux"))
assert catalogue.theme_icon("absente-introuvable") is None

print(f"OK: {len(desktops)} .desktop, fenêtres GTK et .exe maison ont une icône Prisme")
