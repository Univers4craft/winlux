#!/usr/bin/python3
"""Contrepoids de la barre centrée : bloc de droite mesuré depuis le repère 46 (#2467).

Le bloc de droite commence souvent par un greffon interne à la barre (bouton de
l'assistant, flèche des icônes cachées), sans fenêtre X : invisible à la mesure,
il manquait au contrepoids, et le groupe Démarrer tombait à gauche de la moitié
de sa largeur. Le repère 46, genmon externe d'un pixel posé juste après le
séparateur 8, donne le vrai bord du bloc.

Relevés de la barre réelle sous Xvfb (1920 px, ordre livré, bouton de
l'assistant 31 après le repère) : 46 à x=1688, 10 réduit à 1 px sans icône,
état 4 à 1736, horloge 7, cloche 9. Le lanceur 31 (1689-1735) n'a pas de
fenêtre.
"""
import ast
from pathlib import Path
import sys

SCRIPTS = Path(__file__).resolve().parent
failures = []


def ok(condition, label):
    print(("OK    " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


source = (SCRIPTS / "winlux-tray.py").read_text(encoding="utf-8")
scope = {}
nodes = [node for node in ast.parse(source).body
         if isinstance(node, ast.FunctionDef) and node.name in ("balance_width", "balance_sides")]
exec(compile(ast.Module(body=nodes, type_ignores=[]), "winlux-tray.py", "exec"), scope)
balance_width, balance_sides = scope["balance_width"], scope["balance_sides"]

shipped = [3, 13, 1, 15, 14, 2, 8, 46, 31, 10, 4, 7, 9]
children = [(9, 1882, 38), (7, 1804, 78), (4, 1736, 68), (10, 1735, 1), (46, 1688, 1), (15, 831, 210), (13, 599, 186)]
right, left = balance_sides(shipped)
ok(right == {46, 31, 10, 4, 7, 9} and left == set(), f"côtés de la barre centrée ({sorted(right)}, {sorted(left)})")

width = balance_width(1920, children, right, left)
ok(width == 232, f"repère 46 : bloc de droite de 232 px, bouton de l'assistant compris ({width})")
blind = balance_width(1920, children, right, left, edge=None)
ok(blind == 184, f"sans repère : bouton interne de 46 px manqué, comme avant le correctif ({blind})")

# Repère hors de l'écran (barre en train de se réarranger) : ancien calcul.
moved = [(46, 1920, 1) if plugin == 46 else (plugin, x, w) for plugin, x, w in children]
ok(balance_width(1920, moved, right, left) == 184, "repère hors de l'écran : mesure par les greffons visibles")

# Contrepoids listé aussi avant le séparateur 3 : jamais compté à gauche.
doubled = [13] + shipped
sides = balance_sides(doubled)
ok(sides == (right, left), f"ids [13, 3, 13, 1…] : mêmes côtés que [3, 13, 1…] ({sides})")
ok(balance_width(1920, children, *sides) == width,
   f"ids [13, 3, 13, 1…] : même largeur que [3, 13, 1…] ({balance_width(1920, children, *sides)})")
ok(balance_width(1920, children, right, {13}) == width, "contrepoids exclu du bloc de gauche")

# Bouton Widgets (11) à gauche du séparateur 3 : retranché du bloc de droite.
widgets = balance_sides([11] + shipped)
ok(widgets[1] == {11} and balance_width(1920, children + [(11, 0, 60)], *widgets) == 172,
   "bouton Widgets à gauche : retranché du contrepoids")
ok(balance_sides([3, 44, 1, 2, 45, 8]) is None, "barre flottante : pas de contrepoids")

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("OK: contrepoids mesuré depuis le repère du bloc de droite")
