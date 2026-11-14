#!/usr/bin/python3
"""Contrôle parental : application bloquée avant #2248 (#2351).

Un blocage fait avant #2248 a enregistré Mousepad sous le nom « Text Editor
Settings ». Après la mise à jour, le panneau (vrai GTK, sous Xvfb) et la bulle
montrée à l'enfant doivent dire « Mousepad » ; un .exe garde son nom enregistré.
Sans root : racine factice (WINLUX_PARENTAL_ROOT), processus et bulle simulés.
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import os
from pathlib import Path
import shutil
import sys
import tempfile
from unittest import mock

HERE = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="winlux-parental-ancien-nom."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"
os.environ["NO_AT_BRIDGE"] = "1"
failures = 0


def check(cond, label):
    global failures
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures += 1


try:
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk
except (ImportError, ValueError) as error:
    print(f"ÉCHEC prérequis manquant : PyGObject et GTK 3 (python3-gi, gir1.2-gtk-3.0) : {error}")
    sys.exit(1)
if not Gtk.init_check(sys.argv)[0]:
    print("ÉCHEC prérequis manquant : affichage X (lancer sous xvfb-run)")
    sys.exit(1)

apps = work / "usr/share/applications"
apps.mkdir(parents=True)
for file, body in {
    "org.xfce.mousepad-settings.desktop": "Name=Text Editor Settings\nExec=mousepad --preferences\nNoDisplay=true\n",
    "org.xfce.mousepad.desktop": "Name=Mousepad\nExec=mousepad %U\n",
}.items():
    (apps / file).write_text("[Desktop Entry]\nType=Application\n" + body, encoding="utf-8")

loader = SourceFileLoader("winlux_parental", str(HERE / "winlux-parental.py"))
spec = importlib.util.spec_from_loader("winlux_parental", loader)
parental = importlib.util.module_from_spec(spec)
loader.exec_module(parental)

OLD = [{"id": "bin:mousepad", "name": "Text Editor Settings"}, {"id": "exe:devoirs.exe", "name": "Devoirs"}]


# 1. Panneau : liste des applications bloquées.
class Panel(parental.ParentalPanel):
    def __init__(self):
        self.Gtk = Gtk
        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.cfg = {"apps": [dict(app) for app in OLD]}
        self.message = Gtk.Label()


def labels(widget):
    found = [widget.get_text()] if isinstance(widget, Gtk.Label) else []
    for child in widget.get_children() if isinstance(widget, Gtk.Container) else ():
        found += labels(child)
    return found


panel = Panel()
panel.apps_section({"apps": []})
window = Gtk.Window()
window.add(panel.box)
window.show_all()
shown = labels(panel.box)
check("Mousepad" in shown and "Text Editor Settings" not in shown,
      "panneau : Mousepad bloqué avant la mise à jour affiché « Mousepad » (#2351)")
check("Devoirs" in shown, "panneau : un .exe garde le nom enregistré")
window.destroy()

# 2. Service : bulle montrée à l'enfant quand Mousepad est fermé.
bubbles = []
daemon = parental.Daemon()
daemon.configs = {"enfant": {"apps": [dict(app) for app in OLD]}}
# Cache des noms chargé comme après 10 minutes de marche : son état juste après
# le démarrage est un autre défaut (#2387), hors de ce test.
daemon.names_at = -10.0 ** 9
with mock.patch.object(parental, "uid_of", return_value=1003), \
        mock.patch.object(parental, "pids_of", return_value={4242: 1003}), \
        mock.patch.object(parental, "proc_info", return_value=("/usr/bin/mousepad", ["mousepad"])), \
        mock.patch.object(parental.os, "kill"), \
        mock.patch.object(parental, "update_usage"), \
        mock.patch.object(parental, "session_displays", return_value=[(":1", 7)]), \
        mock.patch.object(parental, "notify", side_effect=lambda user, display, title, body: bubbles.append(body) or True):
    daemon.enforce_apps()
check(bubbles == ["« Mousepad » est bloquée par le contrôle parental."],
      f"bulle de l'enfant : « Mousepad », pas l'ancien nom ({bubbles})")

shutil.rmtree(work, ignore_errors=True)
print("test-parental-ancien-nom :", "réussi" if failures == 0 else f"{failures} échec(s)")
sys.exit(1 if failures else 0)
