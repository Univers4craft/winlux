#!/usr/bin/python3
"""Menu Démarrer : chaque ouverture repart de l'état initial (#673).

Le menu reste lancé et sa fenêtre est seulement cachée : le défilement de la
liste restait là où on l'avait laissé, et il fallait remonter à la main à la
réouverture. Vérifie, après défilement jusqu'en bas (Épinglé puis Toutes les
applications), recherche tapée et fermeture : réouverture sur la vue Épinglé,
recherche vide, liste en haut ; et passage d'une vue à l'autre en haut aussi.

À lancer dans un Xvfb dédié (xvfb-run).
"""
from pathlib import Path
import json
import os
import runpy
import tempfile

tmp = Path(tempfile.mkdtemp(prefix="winlux-start-reset-"))
home = tmp / "home"
os.environ["HOME"] = str(home)
os.environ.pop("XDG_CONFIG_HOME", None)
os.environ["XDG_DATA_HOME"] = str(home / ".local/share")
os.environ["WINLUX_LANG"] = "fr"
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
devices = tmp / "proc/bus/input/devices"
devices.parent.mkdir(parents=True)
# Un clavier physique : pas de clavier tactile dans le menu.
devices.write_text('I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\nN: Name="AT Translated Set 2 keyboard"\n'
                   'H: Handlers=sysrq kbd event0 leds\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
applications = home / ".local/share/applications"
applications.mkdir(parents=True)
pins = []
for index in range(60):
    (applications / f"appli{index:02}.desktop").write_text(
        f"[Desktop Entry]\nType=Application\nName=Application {index:02}\nExec=true\nCategories=Utility;\n")
    pins.append(f"appli{index:02}.desktop")
(home / ".config/winlux").mkdir(parents=True)
# Toutes les applications en liste A–Z (#685) : les 60 applications d'une
# seule catégorie tiendraient sinon dans une seule carte, sans défilement.
(home / ".config/winlux/start.json").write_text(json.dumps({"pins": pins, "recent": [], "apps_view": "list"}))

here = Path(__file__).resolve().parent
module = runpy.run_path(str(here / "winlux-start-menu.py"), run_name="start_reset_test")
Gtk, GLib = module["Gtk"], module["GLib"]
app = module["Start"]()
failures, steps = [], []


def adjustment():
    # Liste du menu : le ScrolledWindow parent de app.content.
    widget = app.content
    while not isinstance(widget, Gtk.ScrolledWindow):
        widget = widget.get_parent()
    return widget.get_vadjustment()


def to_bottom():
    value = adjustment()
    value.set_value(value.get_upper() - value.get_page_size())


def scrolled():
    value = adjustment()
    assert value.get_upper() > value.get_page_size() + 50, "liste trop courte pour défiler"
    assert value.get_value() > 50, f"défilement non appliqué ({value.get_value()})"


def at_top(view):
    def check():
        assert adjustment().get_value() == 0, f"{view} : liste ouverte à {adjustment().get_value():.0f} px du haut"
    return check


def initial():
    assert app.window.get_visible(), "menu non rouvert"
    assert not app.all_apps and app.heading.get_text() == "Épinglé", f"vue {app.heading.get_text()!r} au lieu d'Épinglé"
    assert app.search.get_text() == "", f"recherche {app.search.get_text()!r} gardée"
    at_top("réouverture")()


def step(action, check, delay=500):
    def run():
        try:
            action()
        except Exception as error:  # noqa: BLE001 - rapporté à la fin
            failures.append(error)
            app.quit()
            return False
        GLib.timeout_add(delay, verify)
        return False

    def verify():
        try:
            check()
        except Exception as error:  # noqa: BLE001
            failures.append(error)
            app.quit()
            return False
        if steps:
            GLib.timeout_add(50, steps.pop(0))
        else:
            app.quit()
        return False
    return run


def reopen():
    app.hide()
    app.activate()


steps += [
    step(app.activate, at_top("première ouverture")),
    step(to_bottom, scrolled),
    step(reopen, initial),
    step(app.toggle_list, at_top("Toutes les applications")),
    step(to_bottom, scrolled),
    step(app.toggle_list, at_top("retour à Épinglé")),
    step(lambda: (app.toggle_list(), to_bottom()), scrolled),
    step(lambda: app.search.set_text("Application"), lambda: None, delay=800),
    step(reopen, initial),
]


def start():
    GLib.timeout_add(100, steps.pop(0))
    return False


app.hold()
GLib.timeout_add(300, start)
GLib.timeout_add(30000, lambda: (failures.append(AssertionError("délai dépassé")), app.quit()))
app.run([])
if failures:
    raise failures[0]
print("OK: menu Démarrer rouvert en haut de la vue Épinglé, recherche vide")
