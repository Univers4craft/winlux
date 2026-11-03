#!/usr/bin/python3
"""Navigation au clavier (#28) dans le menu Démarrer et les réglages rapides.

À lancer dans un Xvfb dédié (xvfb-run) : les touches sont envoyées par
xdotool à l'écran virtuel seulement.

    test-keyboard-nav.py start|quick
"""
from pathlib import Path
import runpy
import subprocess
import sys

kind = sys.argv[1] if len(sys.argv) > 1 else "start"
here = Path(__file__).resolve().parent
module = runpy.run_path(str(here / ("winlux-start-menu.py" if kind == "start" else "winlux-popup.py")),
                        run_name="keyboard_test")
Gtk, GLib = module["Gtk"], module["GLib"]
app = module["Start"]() if kind == "start" else module["Popup"](kind)
failures = []
steps = []


def key(*names):
    subprocess.run(["xdotool", "key", "--clearmodifiers", *names], check=True, timeout=5)


def focused():
    widget = app.window.get_focus()
    return widget


def step(action, check, delay=300):
    def run():
        try:
            action()
            GLib.timeout_add(delay, lambda: verify(check))
        except Exception as error:  # noqa: BLE001
            failures.append(error)
            app.quit()
        return False

    def verify(check):
        try:
            check()
            if steps:
                GLib.timeout_add(50, steps.pop(0))
            else:
                app.quit()
        except Exception as error:  # noqa: BLE001
            failures.append(error)
            app.quit()
        return False
    return run


def start():
    try:
        assert app.window and app.window.get_visible(), "fenêtre non ouverte"
        x, y = app.window.get_position()
        width, height = app.window.get_size()
        # Pointeur dans la fenêtre : sans gestionnaire de fenêtres, X donne
        # le clavier à la fenêtre sous le pointeur.
        subprocess.run(["xdotool", "mousemove", str(x + width // 2), str(y + height // 2)], check=True, timeout=5)
        GLib.timeout_add(200, steps.pop(0))
    except Exception as error:  # noqa: BLE001
        failures.append(error)
        app.quit()
    return False


seen = []
if kind == "start":
    def after_down():
        widget = focused()
        assert isinstance(widget, Gtk.Button), ("Flèche bas : pas d'application sélectionnée", widget)
        assert app.window.get_focus_visible(), "cadre de focus invisible au clavier"
        seen.append(widget)

    def after_tab():
        assert focused() is not None and focused() is not seen[0], "Tab ne déplace pas le focus"

    steps += [step(lambda: key("Down"), after_down), step(lambda: key("Tab"), after_tab)]
else:
    def after_tab():
        widget = focused()
        assert isinstance(widget, (Gtk.Button, Gtk.ToggleButton, Gtk.Scale)), ("Tab : aucun contrôle", widget)
        seen.append(widget)

    def after_second():
        assert focused() is not seen[0], "le second Tab ne change pas de contrôle"

    steps += [step(lambda: key("Tab"), after_tab), step(lambda: key("Tab"), after_second)]


def closed():
    assert not app.window.get_visible(), "Échap ne ferme pas"


steps.append(step(lambda: key("Escape"), closed))
GLib.timeout_add(900, start)
app.run([])
if failures:
    raise failures[0]
print(f"OK: {kind}, flèches, Tab et Échap au clavier")
