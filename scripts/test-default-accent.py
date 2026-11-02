#!/usr/bin/env python3
"""L'accent par défaut de la session native est le violet Prisme, pas le bleu de Windows."""
import re
import ast
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PRISME = "#6a3df0"
# Famille Prisme : accent clair, survol, et variantes lisibles sur fond sombre.
PRISME_FAMILY = {PRISME, "#7c4dff", "#b19af9", "#b69cff"}
errors = []

pages = (ROOT / "scripts/winlux-settings-pages.py").read_text()
accents = ast.literal_eval(re.search(r"^ACCENTS = (\(.*?\))$", pages, re.M).group(1))
if accents[0].lower() != PRISME:
    errors.append(f"ACCENTS[0] vaut {accents[0]}, attendu {PRISME}")

old = re.compile(r"#8178b7", re.I)
default = re.compile(r"""["']accent["']\s*[:,]\s*["'](#[0-9a-fA-F]{6})""")
for path in sorted((ROOT / "scripts").glob("winlux-*")):
    if not path.is_file():
        continue
    text = path.read_text(errors="replace")
    if old.search(text):
        errors.append(f"{path.name} contient encore l'ancien accent #8178b7")
    for value in default.findall(text):
        if value.lower() not in PRISME_FAMILY:
            errors.append(f"{path.name} : accent par défaut {value}, hors de la famille Prisme")

for e in errors:
    print("ÉCHEC :", e)
print("accent par défaut :", "OK" if not errors else f"{len(errors)} erreur(s)")
sys.exit(1 if errors else 0)
