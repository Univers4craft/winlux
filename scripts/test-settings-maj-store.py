#!/usr/bin/python3
"""Paramètres > Applications : section « Mises à jour du Store » (#3133).

Sous Xvfb, avec un faux winlux-boutique-maj qui note ses appels :
1. la section est visible, avec la version utilisée et le réglage
   « Rechercher au lancement du Store », activé par défaut ;
2. « Revenir à la version précédente / livrée » sont grisés sans copie du compte ;
3. « Rechercher maintenant » appelle « rechercher » et montre le verdict ;
4. couper le réglage appelle « auto non » : rien n'est caché.
"""
import os
from pathlib import Path
import runpy
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
failures = []


def check(condition, message):
    print(("OK   " if condition else "ÉCHEC ") + message)
    if not condition: failures.append(message)


tmp = Path(tempfile.mkdtemp())
log = tmp / "appels"
outil = tmp / "winlux-boutique-maj"
outil.write_text(f'#!/bin/sh\necho "$*" >> {log}\necho resultat=a_jour\necho numero=100\n')
outil.chmod(0o755)
(tmp / "livree.txt").write_text("numero=100\n")
os.environ.update(WINLUX_BOUTIQUE_MAJ=str(outil), WINLUX_STORE_LANCEUR=str(HERE / "winlux-store-lanceur.py"),
                  WINLUX_STORE_LIVREE_NUMERO=str(tmp / "livree.txt"), XDG_DATA_HOME=str(tmp / "data"),
                  XDG_CONFIG_HOME=str(tmp / "config"), WINLUX_LANG="fr", WINLUX_RUN_DIR=str(tmp / "run"))
(tmp / "run").mkdir()

settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="maj_store_test")
Gtk = settings["Gtk"]
pages = settings["EXTRA"]


def walk(widget):
    yield widget
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children(): yield from walk(child)


def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        while Gtk.events_pending(): Gtk.main_iteration_do(False)
        time.sleep(0.01)


def calls():
    return log.read_text().splitlines() if log.exists() else []


window = settings["Settings"]()
window.show_all()
window.open_page(pages.PAGE_APPS)
pump(0.3)
labels = [w.get_text() for w in walk(window.body) if isinstance(w, Gtk.Label)]
print(f"textes : {[l for l in labels if 'Store' in l or 'Version' in l]}")
check("Mises à jour du Store" in labels, "section « Mises à jour du Store » visible")
check("Version livrée avec le système (100)" in labels, "version utilisée : la livrée, numéro 100")
def find_switch():
    """Interrupteur de la ligne « Rechercher au lancement du Store » (la page est reconstruite après chaque action)."""
    row = next((w for w in walk(window.body) if isinstance(w, Gtk.Label) and w.get_text() == "Rechercher au lancement du Store"), None)
    line = row
    while line is not None and not any(isinstance(c, Gtk.Switch) for c in walk(line)):
        line = line.get_parent()
    return next((c for c in walk(line) if isinstance(c, Gtk.Switch)), None) if line else None


switch = find_switch()
check(switch is not None, "réglage « Rechercher au lancement du Store » visible")
check(switch is not None and switch.get_active(), "le réglage est activé par défaut")
buttons = {w.get_label(): w for w in walk(window.body) if isinstance(w, Gtk.Button) and w.get_label()}
check(buttons.get("Revenir à la version précédente") is not None and not buttons["Revenir à la version précédente"].get_sensitive()
      and not buttons["Revenir à la version livrée"].get_sensitive(), "retours en arrière grisés sans copie du compte")
check(calls() == [], f"ouvrir la page n'appelle pas l'outil : {calls()}")
buttons["Rechercher maintenant"].clicked()
pump(1.0)
check(calls() == ["rechercher"], f"« Rechercher maintenant » appelle rechercher : {calls()}")
dialogs = [w for w in Gtk.Window.list_toplevels() if isinstance(w, Gtk.MessageDialog) and w.get_visible()]
textes = [d.get_property("secondary-text") for d in dialogs]
print(f"bulles : {textes}")
check("Le Store est à jour." in textes, "le verdict est montré")
for d in dialogs: d.destroy()
switch = find_switch()
if switch is not None:
    switch.set_active(False)
    pump(1.0)
check(calls()[-1:] == ["auto non"], f"couper le réglage appelle « auto non » : {calls()}")
window.destroy()
if failures:
    sys.exit("ÉCHEC (#3133) : " + " ; ".join(failures))
print("Paramètres : mises à jour du Store visibles et sans réglage caché (#3133)")
