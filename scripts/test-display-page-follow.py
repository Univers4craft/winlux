#!/usr/bin/python3
"""Paramètres > Écran suit le débranchement d'un écran sans rechargement
(#1442) : avec deux écrans, la section « Plusieurs écrans » et sa
disposition s'affichent ; quand l'état publié (control.txt) ne compte plus
qu'un écran, la page se reconstruit seule et la section disparaît.

À lancer sous xvfb-run."""
import os
import runpy
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
failures = []


def ok(cond, label):
    print(("OK    " if cond else "ÉCHEC ") + label)
    if not cond:
        failures.append(label)


if not os.environ.get("DISPLAY"):
    print("SKIP : pas d'affichage (lancer sous xvfb-run)")
    sys.exit(0)

TWO = ("display_count=2\ndisplay_layout=extend\ndisplay_1_name=Virtual-1\ndisplay_1_primary=1\n"
       "display_1_modes=1366x768\ndisplay_1_mode=1366x768\ndisplay_2_name=Virtual-2\n"
       "display_2_primary=0\ndisplay_2_modes=1366x768\ndisplay_2_mode=1366x768\n")
ONE = ("display_count=1\ndisplay_layout=extend\ndisplay_1_name=Virtual-1\ndisplay_1_primary=1\n"
       "display_1_modes=1366x768\ndisplay_1_mode=1366x768\n")
published = {"text": TWO}

os.environ["WINLUX_SETTINGS_PAGE"] = "8"
os.environ["WINLUX_AMBIANCE_FAKE_SUPPORTED"] = "0"
settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="display_page_follow_test")
# run_path rend une copie des globales : on remplace `read` dans celles
# que voient vraiment les méthodes de la fenêtre.
module_globals = settings["Settings"].poll.__globals__
real_read = module_globals["read"]
module_globals["read"] = lambda path, *a, **k: published["text"] if str(path) == "/run/winlux/control.txt" else real_read(path, *a, **k)
Gtk = settings["Gtk"]


def texts(widget, found):
    if isinstance(widget, Gtk.Label):
        found.append(widget.get_text())
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children():
            texts(child, found)
    return found


def pump(seconds, until=lambda: False):
    deadline = time.time() + seconds
    while time.time() < deadline and not until():
        Gtk.main_iteration_do(False)
        time.sleep(0.02)


window = settings["Settings"]()
pump(0.5)
ok("Plusieurs écrans" in texts(window.body, []), "deux écrans : section « Plusieurs écrans » affichée")

# Second écran débranché : winlux-control publie un nouvel état.
published["text"] = ONE
pump(5, lambda: "Plusieurs écrans" not in texts(window.body, []))
shown = texts(window.body, [])
ok("Plusieurs écrans" not in shown, "écran débranché : la page se met à jour seule, sans rechargement")
ok("Écran" in shown, "écran débranché : la carte de l'écran restant reste affichée")

# Pas de reconstruction en boucle quand rien ne change.
rebuilt = []
original = window.select
window.select = lambda *a: (rebuilt.append(1), original(*a))[1]
pump(3)
ok(not rebuilt, f"état inchangé : pas de reconstruction ({len(rebuilt)})")
window.destroy()

if failures:
    print(f"\n{len(failures)} échec(s)")
    sys.exit(1)
print("\nPage Écran : suit les écrans branchés et débranchés")
