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

# Fonds d'écran (#590) : seuls ceux que dessine scripts/render-brand.py sont
# livrés. Une image extérieure (générée ou trouvée) n'est pas vérifiable
# automatiquement et a déjà apporté une fenêtre à quatre carreaux proche du
# logo Windows ; en ajouter une passe par une revue et cette liste.
brand_source = (HERE / "render-brand.py").read_text()
rendered = set(re.findall(r'"(winlux-prisme-[a-z-]+)"', brand_source))
# Définitions supplémentaires (« Winlux Aurore », #596) : suffixes -LxH.
rendered |= {name + size for name in rendered for size in re.findall(r'"(-\d+x\d+)"', brand_source)}
for wall in (ROOT / "assets/wallpapers").iterdir():
    check(wall.stem in rendered, f"fond d'écran non dessiné par render-brand.py : {wall.name}")
iso = (HERE / "build-live-iso.sh").read_text()
source = re.search(r'^wallpaper_src="\$root/assets/wallpapers/([^"]+)"', iso, re.M)
check(source and Path(source.group(1)).stem in rendered, "wallpaper.bmp ne vient pas d'un fond Prisme")
for path in (ROOT / "scripts").glob("*.py"):
    check("Fond ecran" not in path.read_text(errors="replace") or path.name == Path(__file__).name,
          f"{path.name} : référence à l'ancien fond retiré")

# Programmes d'essai PE (#1427) : aucun texte affiché ne cite une marque
# (« FillRect surfaces Windows 11 » dans gui_demo). Seuls les littéraux de
# chaîne hors commentaires comptent ; les noms d'API Windows restent permis.
LITERAL = re.compile(r'b?"(?:[^"\\]|\\.)*"')
VISIBLE_BRAND = re.compile(r"windows\s*(?:1[01]|xp|vista)\b|microsoft|\bedge\b|\bbing\b", re.I)
API_NAMES = {'b"Microsoft Unified Security Protocol Provider"'}  # nom du paquet SSPI (Schannel)
for path in sorted((ROOT / "testexe/src/bin").glob("*.rs")):
    for number, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
        if line.lstrip().startswith("//"):
            continue
        for match in LITERAL.finditer(line):
            if "//" in line[:match.start()]:
                break
            if VISIBLE_BRAND.search(match.group()) and match.group() not in API_NAMES:
                check(False, f"{path.relative_to(ROOT)}:{number} : marque dans un texte affiché {match.group()}")

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print(f"OK: aucune marque Microsoft par défaut ({len(start_pins)} épingles, 4 langues, migration)")
