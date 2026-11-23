#!/usr/bin/python3
"""Menu Démarrer : une ouverture ne construit la liste qu'une fois (gain #3217).

do_activate appelait render(), puis place(), qui se termine lui-même par
render() : la liste des épinglés (ou des applications) était construite deux
fois à chaque ouverture, la première pour rien. Sur le PC Winlux, render coûtait
6,8 ms en médiane et jusqu'à 33 ms (diag-demarrer.txt, 22 nov. 2026).

Vérifie, dans le processus du menu préchargé (comme la touche Windows) :
- un seul render() par ouverture, sur cinq ouvertures ;
- le menu ouvert montre bien sa liste (au moins un bouton) et le champ de
  recherche est vide, à chaque ouverture, même après une recherche.

À lancer dans un Xvfb et un bus de session dédiés :
  xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session -- /usr/bin/python3 -B scripts/test-start-menu-rendu-unique.py
"""
import os
import runpy
import sys
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "winlux-start-menu.py"
home = Path(tempfile.mkdtemp(prefix="test-rendu-unique-"))
os.environ["HOME"] = str(home)
for var in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"):
    os.environ.pop(var, None)
failures = []


def ok(condition, message):
    print(("OK    " if condition else "ÉCHEC ") + message)
    if not condition:
        failures.append(message)


menu = runpy.run_path(str(SCRIPT), run_name="test_rendu_unique")
Gtk, GLib = menu["Gtk"], menu["GLib"]
app = menu["Start"]()
app.register(None)
app.preloading = True
app.activate()                      # préchargement : fenêtre construite, cachée


def pump():
    context = GLib.MainContext.default()
    for _ in range(200):
        if not context.iteration(False):
            break


pump()
compte = [0]
render = app.render


def render_compte(*args, **kwargs):
    compte[0] += 1
    return render(*args, **kwargs)


app.render = render_compte
par_ouverture, listes, recherches = [], [], []
for essai in range(5):
    if essai == 2:
        app.search.set_text("zz")   # une recherche laissée tapée avant de fermer
    if app.window.get_visible():
        app.hide()
    pump()
    compte[0] = 0
    app.activate()
    par_ouverture.append(compte[0])
    pump()
    listes.append(len(app.content.get_children()))
    recherches.append(app.search.get_text())
app.hide()
print(f"render par ouverture : {par_ouverture} ; éléments de la liste : {listes} ; recherche : {recherches}")
ok(all(n == 1 for n in par_ouverture),
   "Démarrer : la liste n'est construite qu'une fois par ouverture (#3217)")
ok(all(n > 0 for n in listes) and all(t == "" for t in recherches),
   "Démarrer : chaque ouverture montre la liste, recherche vide (#3217)")
sys.exit(1 if failures else 0)
