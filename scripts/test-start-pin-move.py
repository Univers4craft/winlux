#!/usr/bin/python3
"""Menu Démarrer : déplacer une application épinglée (#925).

1. move_pin() : au début, à gauche, à droite, à la fin, place visée par un
   glisser-déposer ; rien ne bouge → None ; une épingle d'application absente
   garde sa place relative.
2. Vrai menu sous Xvfb : clic droit (xdotool) sur la 4e tuile → sous-menu
   « Déplacer » avec ses quatre choix ; « Au début » et « Vers la gauche »
   grisés sur la 1re tuile, « Vers la droite » et « À la fin » sur la
   dernière, et affichés grisés ; « Au début » sur la 4e la met en tête, à l'écran et dans
   start.json.
3. Glisser-déposer réel (xdotool) de la 5e tuile sur la 2e : tuile glissée
   estompée, place visée (pas celle de départ) entourée d'accent, rien
   n'est lancé au lâcher ; « Déplacer » absent de Toutes les applications.
4. Un nouveau menu (nouvelle session) relit le même ordre.
5. Les cinq textes existent en anglais, espagnol et allemand.

À lancer dans un Xvfb dédié (xvfb-run).
"""
from pathlib import Path
import json
import os
import runpy
import shutil
import subprocess
import sys
import tempfile

tmp = Path(tempfile.mkdtemp(prefix="winlux-start-pin-move-"))
home = tmp / "home"
os.environ["HOME"] = str(home)
os.environ.pop("XDG_CONFIG_HOME", None)
os.environ["XDG_DATA_HOME"] = str(home / ".local/share")
os.environ["WINLUX_LANG"] = "fr"
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
devices = tmp / "proc/bus/input/devices"
devices.parent.mkdir(parents=True)
devices.write_text('I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\nN: Name="AT Translated Set 2 keyboard"\n'
                   'H: Handlers=sysrq kbd event0 leds\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
applications = home / ".local/share/applications"
applications.mkdir(parents=True)
names = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"]
pins = []
for name in names:
    (applications / f"{name.lower()}.desktop").write_text(
        f"[Desktop Entry]\nType=Application\nName={name}\nExec=true\nIcon=application-x-executable\nCategories=Utility;\n")
    pins.append(f"{name.lower()}.desktop")
pins.insert(2, "absente.desktop")  # application désinstallée : garde sa place
state_file = home / ".config/winlux/start.json"
state_file.parent.mkdir(parents=True)
state_file.write_text(json.dumps({"pins": pins, "recent": [], "apps_view": "list"}))

here = Path(__file__).resolve().parent
module = runpy.run_path(str(here / "winlux-start-menu.py"), run_name="start_pin_move_test")
Gtk, GLib, Gdk = module["Gtk"], module["GLib"], module["Gdk"]
move_pin = module["move_pin"]
failures = []


def check(condition, message):
    if not condition:
        failures.append(message)


# ── 1. Fonction pure ──
base = ["a", "b", "x", "c", "d"]
shown = {"a", "b", "c", "d"}
check(move_pin(base, "c", "first", shown) == ["c", "a", "x", "b", "d"], f"au début : {move_pin(base, 'c', 'first', shown)}")
check(move_pin(base, "c", "left", shown) == ["a", "c", "x", "b", "d"], f"à gauche : {move_pin(base, 'c', 'left', shown)}")
check(move_pin(base, "b", "right", shown) == ["a", "c", "x", "b", "d"], f"à droite : {move_pin(base, 'b', 'right', shown)}")
check(move_pin(base, "a", "last", shown) == ["b", "c", "x", "d", "a"], f"à la fin : {move_pin(base, 'a', 'last', shown)}")
check(move_pin(base, "d", 1, shown) == ["a", "d", "x", "b", "c"], f"glisser à la 2e place : {move_pin(base, 'd', 1, shown)}")
for identifier, where in (("a", "first"), ("a", "left"), ("d", "right"), ("d", "last"), ("x", "first"), ("z", "last")):
    check(move_pin(base, identifier, where, shown) is None, f"{identifier} {where} devrait être sans effet")
check(move_pin(base, "c", 99, shown)[-1] == "c", "place hors grille ramenée à la fin")

# ── 2 à 4. Vrai menu ──
app = module["Start"]()
app.register(None)
xdotool = shutil.which("xdotool")


def tiles():
    grid = next(child for child in app.content.get_children() if isinstance(child, Gtk.Grid))
    buttons = sorted(grid.get_children(), key=lambda b: (grid.child_get_property(b, "top-attach"), grid.child_get_property(b, "left-attach")))
    return buttons


def order():
    return [button.get_tooltip_text() for button in tiles()]


def centre(button):
    allocation = button.get_allocation()
    _, x, y = button.get_window().get_origin()
    return x + allocation.x + allocation.width // 2, y + allocation.y + allocation.height // 2


def pump(seconds=0.4):
    loop = GLib.MainLoop()
    GLib.timeout_add(int(seconds * 1000), loop.quit)
    loop.run()


def xdo(*args):
    subprocess.run([xdotool, *map(str, args)], check=True)


def submenu_of_context():
    menu = app.context
    if menu is None: return None
    for item in menu.get_children():
        if item.get_submenu() is not None:
            return {child.get_label(): child for child in item.get_submenu().get_children()}
    return None


def right_click(index):
    if app.context is not None:
        app.context.popdown()
        pump(0.2)
    x, y = centre(tiles()[index])
    xdo("mousemove", x, y)
    pump(0.2)
    xdo("click", 3)
    pump(0.6)
    return submenu_of_context()


def run():
    app.activate()
    pump(1.0)
    check(order() == names, f"ordre initial {order()}")
    if xdotool is None:
        failures.append("xdotool absent")
        return
    choices = right_click(0)
    check(choices is not None and set(choices) == {"Au début", "Vers la gauche", "Vers la droite", "À la fin"},
          f"sous-menu Déplacer de la 1re tuile : {choices and list(choices)}")
    if choices:
        check(not choices["Au début"].get_sensitive() and not choices["Vers la gauche"].get_sensitive()
              and choices["Vers la droite"].get_sensitive() and choices["À la fin"].get_sensitive(),
              "1re tuile : « Au début » et « Vers la gauche » doivent être grisés, seulement eux")
        # Grisés pour de vrai : couleur du libellé différente des choix actifs.
        def colour(item):
            label = item.get_child()
            context = label.get_style_context()
            return context.get_color(context.get_state()).to_string()
        check(colour(choices["Au début"]) != colour(choices["À la fin"]),
              f"« Au début » grisé mais affiché comme les autres ({colour(choices['Au début'])})")
    choices = right_click(len(names) - 1)
    if choices:
        check(choices["Au début"].get_sensitive() and not choices["Vers la droite"].get_sensitive()
              and not choices["À la fin"].get_sensitive(), "dernière tuile : « Vers la droite » et « À la fin » grisés")
    choices = right_click(3)
    if choices:
        choices["Au début"].activate()
        pump(0.6)
    expected = ["Delta", "Alpha", "Bravo", "Charlie", "Echo", "Foxtrot"]
    check(order() == expected, f"après « Au début » sur Delta : {order()}")
    saved = json.loads(state_file.read_text())["pins"]
    check(saved == ["delta.desktop", "alpha.desktop", "absente.desktop", "bravo.desktop", "charlie.desktop", "echo.desktop", "foxtrot.desktop"],
          f"start.json : {saved}")
    if app.context is not None:
        app.context.popdown()
        pump(0.3)
    if not app.window.get_visible():
        app.activate()
        pump(0.6)
    # Glisser-déposer : Echo (5e) sur Alpha (2e).
    (sx, sy), (tx, ty) = centre(tiles()[4]), centre(tiles()[1])
    xdo("mousemove", sx, sy)
    pump(0.2)
    launched = []
    original = app.launch
    app.launch = lambda target: (launched.append(target.get_id()), original(target))
    source, target = tiles()[4], tiles()[1]
    xdo("mousedown", 1)
    for step in range(1, 13):
        xdo("mousemove", sx + (tx - sx) * step // 12, sy + (ty - sy) * step // 12)
        pump(0.05)
    pump(0.3)
    check(source.get_style_context().has_class("pin-dragged"), "épingle glissée non estompée pendant le glisser")
    check(target.get_style_context().has_class("pin-target"), "place visée non entourée pendant le glisser")
    check(not source.get_style_context().has_class("pin-target"), "la tuile de départ se surligne elle-même")
    xdo("mouseup", 1)
    pump(0.8)
    check(not launched, f"application lancée au lâcher : {launched}")
    check(not any(b.get_style_context().has_class("pin-dragged") or b.get_style_context().has_class("pin-target") for b in tiles()),
          "estompage ou cadre restés après le lâcher")
    expected = ["Delta", "Echo", "Alpha", "Bravo", "Charlie", "Foxtrot"]
    check(order() == expected, f"après glisser Echo sur Alpha : {order()}")
    check(app.window.get_visible(), "le menu s'est fermé pendant le glisser-déposer")
    # « Déplacer » n'existe que dans la grille Épinglé, pas dans Toutes les applications.
    app.toggle_list()
    pump(0.8)
    def buttons(widget):
        if isinstance(widget, Gtk.Button) and widget.get_tooltip_text() == "Alpha": yield widget
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): yield from buttons(child)
    listed = next(buttons(app.content), None)
    check(listed is not None, "Alpha introuvable dans Toutes les applications")
    if listed is not None:
        xdo("mousemove", *centre(listed))
        pump(0.2)
        xdo("click", 3)
        pump(0.6)
        check(app.context is not None and submenu_of_context() is None, "« Déplacer » proposé hors de la grille Épinglé")
        if app.context is not None: app.context.popdown()
        pump(0.3)


run()
# ── 4. Nouvelle session : un nouveau menu relit le même ordre ──
again = module["Start"]()
reread = [pin for pin in again.state["pins"] if pin != "absente.desktop"]
check(reread == ["delta.desktop", "echo.desktop", "alpha.desktop", "bravo.desktop", "charlie.desktop", "foxtrot.desktop"],
      f"ordre relu par un nouveau menu : {reread}")

# ── 5. Quatre langues ──
table = module["_TR"]
for text in ("Déplacer", "Au début", "Vers la gauche", "Vers la droite", "À la fin"):
    check(text in table and len(table[text]) == 3 and all(table[text]), f"« {text} » non traduit")

for failure in failures:
    print("ÉCHEC :", failure)
if failures:
    sys.exit(1)
print("OK: épingle déplacée par le clic droit et par glisser-déposer, ordre gardé, 4 langues (#925)")
