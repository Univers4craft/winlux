#!/usr/bin/python3
"""`winlux-settings about` ouvre À propos (et non Écran), comme
`ms-settings:about` sous Windows : noms de page de la ligne de commande."""
import os
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
os.environ.setdefault("WINLUX_LANG", "fr")
module = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="settings_args")
page, pages, names = module["page_argument"], module["PAGES"], module["PAGE_NAMES"]
failures = []
for value, expected in (("about", 9), ("ABOUT", 9), ("à propos", 9), ("apropos", 9), ("ms-settings:about", 9),
                        ("9", 9), ("display", 8), ("ms-settings:display", 8), ("network", 2), ("bluetooth", 22),
                        ("accounts", 33), ("comptes", 33), ("power", 23), ("hardware", 34), ("sound", 16),
                        ("99", None), ("inconnue", None), ("", None)):
    if page(value) != expected:
        failures.append(f"{value!r} → {page(value)}, attendu {expected}")
if any(not 0 <= number < len(pages) for number in names.values()):
    failures.append("numéro de page hors limites")
if pages[9] != module["_t"]("À propos") or pages[8] != module["_t"]("Écran"):
    failures.append("numérotation des pages changée")
shell = (HERE / "winlux-shell-app.sh").read_text()
if 'exec /usr/bin/python3 /usr/bin/winlux-settings "$2"' not in shell:
    failures.append("winlux-shell-app settings about ne passe pas le nom")
# La fenêtre s'ouvre bien sur la page demandée.
os.environ["WINLUX_SETTINGS_PAGE"] = str(page("about"))
window = module["Settings"]()
if window.current != 9:
    failures.append(f"fenêtre ouverte sur la page {window.current}, pas À propos")
if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print("OK: winlux-settings about / ms-settings:… ouvrent la bonne page")
