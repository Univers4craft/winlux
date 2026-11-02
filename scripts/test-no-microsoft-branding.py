#!/usr/bin/python3
"""Aucune marque ni logo Microsoft par défaut (projet indépendant).

Épingles du menu Démarrer (compte neuf, menu Whisker, bibliothèque de jeux),
lanceurs .desktop des épingles par défaut dans les quatre langues, et
migration de l'ancienne épingle Edge vers le « Navigateur » générique.
Un navigateur que l'utilisateur installe lui-même garde son vrai nom.
"""
import ast
import configparser
import os
from pathlib import Path
import re
import runpy
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
BRAND = re.compile(r"microsoft|\bedge\b|msedge|\bbing\b|windows\s*logo|office\s*365", re.I)
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)


def literal_list(path, marker):
    """Première liste littérale de chaînes qui suit `marker` dans un fichier Python."""
    text = path.read_text()
    start = text.index("[", text.index(marker))
    depth, end = 0, start
    for end, char in enumerate(text[start:], start):
        depth += char == "["
        depth -= char == "]"
        if depth == 0:
            break
    return ast.literal_eval(text[start:end + 1])


start_pins = literal_list(HERE / "winlux-start-menu.py", "DEFAULTS =")
games_pins = literal_list(ROOT / "integration/native/jeux/winlux-games.py", "pins = [")
whisker = (ROOT / "integration/native/config/xfce4/panel/whiskermenu-1.rc").read_text()
whisker_pins = re.search(r"^favorites=(.*)$", whisker, re.M).group(1).split(",")
for name, pins in (("menu Démarrer", start_pins), ("bibliothèque de jeux", games_pins), ("menu Whisker", whisker_pins)):
    check("winlux-browser.desktop" in pins, f"{name} : pas d'épingle « Navigateur »")
    for pin in pins:
        check(not BRAND.search(pin), f"{name} : épingle à marque Microsoft {pin}")
check(start_pins == games_pins, "épingles par défaut différentes entre menu Démarrer et jeux")

# Lanceurs des épingles par défaut : nom, nom générique, commentaire et icône.
sources = [ROOT / "integration", ROOT / "integration/native/applications", ROOT / "integration/native/helpers"]
for pin in set(start_pins) | set(whisker_pins):
    files = [folder / pin for folder in sources if (folder / pin).is_file()]
    if pin.startswith("org.xfce."):
        continue  # fourni par Ubuntu (Mousepad), sans marque Microsoft
    check(files, f"lanceur introuvable pour {pin}")
    for path in files:
        entry = configparser.RawConfigParser(strict=False, interpolation=None)
        entry.optionxform = str
        entry.read(path, encoding="utf-8")
        values = dict(entry["Desktop Entry"]) if entry.has_section("Desktop Entry") else {}
        for key, value in values.items():
            if key.split("[")[0] in ("Name", "GenericName", "Comment", "Icon", "Keywords"):
                check(not BRAND.search(value), f"{path.relative_to(ROOT)} : {key}={value}")
browser = configparser.RawConfigParser(strict=False, interpolation=None)
browser.optionxform = str
browser.read(ROOT / "integration/winlux-browser.desktop", encoding="utf-8")
names = {key: value for key, value in browser["Desktop Entry"].items() if key.startswith("Name")}
check(names.get("Name") == "Navigateur web" and names.get("Name[en]") == "Web browser"
      and names.get("Name[de]") == "Webbrowser" and names.get("Name[es]") == "Navegador web",
      f"« Navigateur web » dans les quatre langues : {names}")
check(browser["Desktop Entry"].get("Icon") == "web-browser", "icône neutre web-browser")

# Lien http/https : le navigateur par défaut, pas une marque.
launcher = (ROOT / "integration/winlux-browser").read_text()
check("winlux-browsers --open" in launcher, "winlux-browser n'ouvre pas le navigateur choisi")
check("n'est pas installé. Installez-le" not in launcher, "winlux-browser pousse encore Edge")

# Shell PE : plus de dessin imitant le logo Edge.
gfx = (ROOT / "testexe/src/shell/gfx.rs").read_text()
check("Glyph::Edge" not in gfx, "logo Edge dessiné dans le shell PE")

# Compte existant : l'ancienne épingle Edge devient « Navigateur », sans doublon.
with tempfile.TemporaryDirectory() as tmp:
    os.environ["HOME"] = tmp
    module = runpy.run_path(str(HERE / "winlux-start-menu.py"), run_name="branding_test")
    migrate = module["migrate_pins"]
    check(migrate(["winlux-explorer.desktop", "winlux-browser-edge.desktop"]) ==
          ["winlux-explorer.desktop", "winlux-browser.desktop"], "migration de l'épingle Edge")
    check(migrate(["winlux-browser-edge.desktop", "winlux-browser.desktop"]) == ["winlux-browser.desktop"],
          "migration sans doublon")
    check(migrate(None) == module["DEFAULTS"], "épingles illisibles : valeurs par défaut")

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print(f"OK: aucune marque Microsoft par défaut ({len(start_pins)} épingles, 4 langues, migration)")
