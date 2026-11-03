#!/usr/bin/python3
"""Menu Démarrer sur tablette sans clavier : clavier tactile intégré.

Le clavier de la barre des tâches, fenêtre à part, fermait le menu au premier
appui et se trouvait sous sa moitié gauche. Le menu porte donc son propre
clavier : ouvert d'office sans clavier physique, sous la recherche, dans la
fenêtre ; les appuis (vrais clics xdotool) écrivent dans la recherche sans
fermer le menu. Avec un clavier physique il reste caché, et l'icône clavier
de la recherche l'ouvre à la main.

À lancer dans un Xvfb dédié (xvfb-run).
"""
from pathlib import Path
import os
import runpy
import subprocess
import tempfile

KEYBOARD = """I: Bus=0011 Vendor=0001 Product=0001 Version=ab41
N: Name="AT Translated Set 2 keyboard"
H: Handlers=sysrq kbd event0 leds
B: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe

"""

tmp = Path(tempfile.mkdtemp(prefix="winlux-start-osk-"))
devices = tmp / "proc/bus/input/devices"
devices.parent.mkdir(parents=True)
devices.write_text("")
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
os.environ["HOME"] = str(tmp / "home")
os.environ.pop("XDG_CONFIG_HOME", None)
os.environ["WINLUX_LANG"] = "fr"
# « winlux-osk hide » lancé par le menu : un faux, pour ne rien toucher.
bin_dir = tmp / "bin"
bin_dir.mkdir()
(bin_dir / "winlux-osk").write_text("#!/bin/sh\nexit 0\n")
(bin_dir / "winlux-osk").chmod(0o755)
os.environ["PATH"] = f"{bin_dir}:{os.environ['PATH']}"

here = Path(__file__).resolve().parent
module = runpy.run_path(str(here / "winlux-start-menu.py"), run_name="start_osk_test")
Gtk, GLib = module["Gtk"], module["GLib"]
app = module["Start"]()
failures, steps = [], []


def find_key(label):
    found = []

    def walk(widget):
        if isinstance(widget, Gtk.Button) and widget.get_label() == label:
            found.append(widget)
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children():
                walk(child)
    walk(app.osk_widget)
    assert found, f"touche « {label} » absente du clavier du menu"
    return found[0]


def tap(widget):
    window = widget.get_window()
    origin = window.get_origin()
    allocation = widget.get_allocation()
    x, y = origin[1] + allocation.x + allocation.width // 2, origin[2] + allocation.y + allocation.height // 2
    subprocess.run(["xdotool", "mousemove", str(x), str(y), "click", "1"], check=True, timeout=5)


def inside_window(widget):
    wx, wy = app.window.get_position()
    ww, wh = app.window.get_size()
    origin = widget.get_window().get_origin()
    allocation = widget.get_allocation()
    x, y = origin[1] + allocation.x, origin[2] + allocation.y
    assert wx <= x and x + allocation.width <= wx + ww + 1, ("clavier hors du menu (largeur)", x, allocation.width, wx, ww)
    assert wy <= y and y + allocation.height <= wy + wh + 1, ("clavier hors du menu (hauteur)", y, allocation.height, wy, wh)
    search = app.search.get_allocation()
    assert allocation.y > search.y, "le clavier doit être sous la recherche"


def step(action, check, delay=400):
    def run():
        try:
            action()
            GLib.timeout_add(delay, lambda: verify())
        except Exception as error:  # noqa: BLE001
            failures.append(error)
            app.quit()
        return False

    def verify():
        try:
            check()
            if steps: GLib.timeout_add(50, steps.pop(0))
            else: app.quit()
        except Exception as error:  # noqa: BLE001
            failures.append(error)
            app.quit()
        return False
    return run


def opened_without_keyboard():
    assert app.window.get_visible(), "menu non ouvert"
    assert app.osk_widget is not None, "clavier tactile du menu absent"
    assert app.osk_widget.get_visible(), "sans clavier physique, le clavier du menu doit s'ouvrir"
    inside_window(app.osk_widget)
    x, y = app.window.get_position()
    width, height = app.window.get_size()
    subprocess.run(["xdotool", "mousemove", str(x + width // 2), str(y + 40)], check=True, timeout=5)


def typed(text):
    def check():
        assert app.window.get_visible(), f"le menu s'est fermé après l'appui (« {text} »)"
        assert app.search.get_text() == text, ("recherche", app.search.get_text(), text)
    return check


def hidden_by_icon():
    assert app.window.get_visible(), "le menu s'est fermé"
    assert not app.osk_widget.get_visible(), "l'icône clavier ne ferme pas le clavier du menu"


def shown_by_icon():
    assert app.osk_widget.get_visible(), "l'icône clavier n'ouvre pas le clavier du menu"
    inside_window(app.osk_widget)


def reopen_with_keyboard():
    app.hide()
    devices.write_text(KEYBOARD)
    app.activate()


def closed_with_keyboard():
    assert app.window.get_visible(), "menu non rouvert"
    assert not app.osk_widget.get_visible(), "avec un clavier physique, le clavier du menu doit rester caché"


def recommendations_hidden():
    labels = []

    def walk(widget):
        if isinstance(widget, Gtk.Label): labels.append(widget.get_text())
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): walk(child)
    walk(app.content)
    assert "Recommandations" not in labels, "« Recommandations » coupé par le clavier : à masquer quand il est ouvert"


def button_visible():
    assert app.osk_button.get_visible() and app.osk_button.get_mapped(), "bouton clavier absent à côté de la recherche"


def reopen_by_button():
    assert not app.osk_widget.get_visible(), "le clavier est déjà ouvert"
    button_visible()
    tap(app.osk_button)


def clear_search():
    app.search.set_text("")


steps += [
    step(lambda: None, lambda: (opened_without_keyboard(), recommendations_hidden(), button_visible())),
    step(lambda: tap(find_key("p")), typed("p")),
    step(lambda: tap(find_key("a")), typed("pa")),
    step(lambda: tap(find_key("i")), typed("pai")),
    # Avec du texte, GtkSearchEntry affiche sa croix : le bouton clavier reste.
    step(lambda: None, button_visible),
    step(lambda: tap(app.osk_button), hidden_by_icon),
    step(lambda: tap(app.osk_button), shown_by_icon),
    step(lambda: (tap(app.osk_button), clear_search()), hidden_by_icon),
    step(reopen_by_button, shown_by_icon),
    step(reopen_with_keyboard, lambda: (closed_with_keyboard(), button_visible()), delay=700),
    step(reopen_by_button, shown_by_icon),
]


def start():
    GLib.timeout_add(400, steps.pop(0))
    return False


GLib.timeout_add(300, start)
app.run([])
if failures:
    raise failures[0]
# « par » : l'application Paramètres passe avant « Bluetooth et appareils ».
rank, settings_for = module["rank"], module["settings_for"]
assert rank("Paramètres", "par") < rank("Bluetooth et appareils", "par"), "classement de la recherche"
assert settings_for("son")[0][0] == "Son", settings_for("son")
assert [label for label, _ in settings_for("par")][:1] == ["Partage de dossiers et transfert des réglages"], settings_for("par")
print("OK: menu Démarrer, clavier tactile intégré (ouvert sans clavier, appuis dans la recherche, caché avec clavier)")
