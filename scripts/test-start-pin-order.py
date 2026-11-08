#!/usr/bin/python3
"""Menu Démarrer : ranger les applications épinglées dans l'ordre voulu (#925).

Partie 1, sans écran : nouvel ordre calculé par move_pin (début, gauche,
droite, fin, place n), épingles d'applications absentes laissées à leur rang,
textes du sous-menu « Déplacer » dans les quatre langues.

Partie 2, dans un Xvfb dédié (xvfb-run) : le clic droit sur un épinglé
propose « Déplacer » avec ses entrées grisées quand elles ne changent rien
(première et dernière icône) ; « Au début » et « À la fin » rangent l'icône,
l'ordre est écrit dans start.json et retrouvé à la réouverture ; une icône
glissée au pointeur (vrais événements X, xdotool) prend la place visée sans
lancer l'application ; un simple clic lance toujours l'application.
"""
from pathlib import Path
import json
import os
import runpy
import shutil
import subprocess
import tempfile

tmp = Path(tempfile.mkdtemp(prefix="winlux-start-pin-order-"))
home = tmp / "home"
os.environ["HOME"] = str(home)
os.environ.pop("XDG_CONFIG_HOME", None)
os.environ["XDG_DATA_HOME"] = str(home / ".local/share")
os.environ["WINLUX_LANG"] = "fr"
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
(tmp / "preferences.conf").write_text("TASKBAR_ALIGN=center\n")
os.environ["WINLUX_PREFERENCES"] = str(tmp / "preferences.conf")
devices = tmp / "proc/bus/input/devices"
devices.parent.mkdir(parents=True)
devices.write_text('I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\nN: Name="AT Translated Set 2 keyboard"\n'
                   'H: Handlers=sysrq kbd event0 leds\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
applications = home / ".local/share/applications"
applications.mkdir(parents=True)
PINS = [f"appli{index}.desktop" for index in range(8)]
for index, ident in enumerate(PINS):
    (applications / ident).write_text(
        f"[Desktop Entry]\nType=Application\nName=Appli {index}\nExec=true\nCategories=Utility;\n")
(home / ".config/winlux").mkdir(parents=True)
STATE = home / ".config/winlux/start.json"
# « absente.desktop » : application désinstallée, épingle gardée mais cachée.
STATE.write_text(json.dumps({"pins": PINS[:3] + ["absente.desktop"] + PINS[3:], "recent": [], "for_you": False}))

here = Path(__file__).resolve().parent
module = runpy.run_path(str(here / "winlux-start-menu.py"), run_name="start_pin_order_test")
move = module["move_pin"]

# ── Partie 1 : calcul de l'ordre ──
pins = ["a", "b", "c", "d"]
assert move(pins, "c", "first") == ["c", "a", "b", "d"]
assert move(pins, "b", "last") == ["a", "c", "d", "b"]
assert move(pins, "c", "previous") == ["a", "c", "b", "d"]
assert move(pins, "b", "next") == ["a", "c", "b", "d"]
assert move(pins, "a", "previous") == pins and move(pins, "d", "next") == pins
assert move(pins, "d", 1) == ["a", "d", "b", "c"] and move(pins, "a", 2) == ["b", "c", "a", "d"]
assert move(pins, "a", 99) == ["b", "c", "d", "a"] and move(pins, "z", "first") == pins
assert pins == ["a", "b", "c", "d"], "liste d'origine modifiée"
# Épingle cachée (application absente) : garde sa place dans la liste.
assert move(["a", "x", "b", "c"], "c", "first", {"a", "b", "c"}) == ["c", "x", "a", "b"]
assert move(["a", "x", "b", "c"], "a", "next", {"a", "b", "c"}) == ["b", "x", "a", "c"]
for lang, expected in (("en", "Move"), ("de", "Verschieben"), ("es", "Mover")):
    os.environ["WINLUX_LANG"] = lang
    other = runpy.run_path(str(here / "winlux-start-menu.py"), run_name="start_pin_order_lang")
    for text in ("Déplacer", "Au début", "Vers la gauche", "Vers la droite", "À la fin"):
        assert other["_t"](text) != text, f"{lang} : « {text} » non traduit"
    assert other["_t"]("Déplacer") == expected, other["_t"]("Déplacer")
os.environ["WINLUX_LANG"] = "fr"
print("OK: ordre des épinglés (début, gauche, droite, fin, place n), épingles cachées gardées, 4 langues")

if not os.environ.get("DISPLAY"):
    print("(pas d'écran : partie graphique non lancée, utiliser xvfb-run)")
    raise SystemExit(0)

Gtk, GLib, Gdk = module["Gtk"], module["GLib"], module["Gdk"]
app = module["Start"]()
launched = []
app.launch = lambda target: launched.append(target.get_id())
failures, steps = [], []
shots = os.environ.get("WINLUX_SHOTS")


def tiles():
    grid = next(child for child in app.content.get_children() if isinstance(child, Gtk.Grid))
    # GtkGrid rend ses enfants à l'envers : tri par ligne puis colonne.
    children = sorted(grid.get_children(), key=lambda child: (grid.child_get_property(child, "top-attach"),
                                                               grid.child_get_property(child, "left-attach")))
    return children


def shown():
    return [tile.winlux_pin for tile in tiles()]


def saved():
    return json.loads(STATE.read_text())["pins"]


def context(ident):
    tile = next(tile for tile in tiles() if tile.winlux_pin == ident)
    event = Gdk.Event.new(Gdk.EventType.BUTTON_PRESS)
    event.button, event.window = 3, tile.get_window()
    event.device = Gdk.Display.get_default().get_default_seat().get_pointer()
    tile.emit("button-press-event", event)
    menu = app.context
    assert menu is not None, "pas de menu contextuel"
    return menu


def labels(menu):
    found = {}
    for item in menu.get_children():
        if isinstance(item, Gtk.SeparatorMenuItem): continue
        row = item.get_child()
        label = next(child for child in row.get_children() if isinstance(child, Gtk.Label))
        found[label.get_text()] = item
    return found


def move_menu(ident):
    menu = context(ident)
    items = labels(menu)
    assert "Déplacer" in items, f"pas de « Déplacer » : {list(items)}"
    submenu = items["Déplacer"].get_submenu()
    return menu, {text: item for text, item in labels(submenu).items()}


def close(menu):
    menu.popdown()
    app.context = None


def first_greyed():
    menu, entries = move_menu(PINS[0])
    sensitive = {text: item.get_sensitive() for text, item in entries.items()}
    close(menu)
    assert sensitive == {"Au début": False, "Vers la gauche": False, "Vers la droite": True, "À la fin": True}, sensitive


def last_greyed():
    menu, entries = move_menu(PINS[-1])
    sensitive = {text: item.get_sensitive() for text, item in entries.items()}
    close(menu)
    assert sensitive == {"Au début": True, "Vers la gauche": True, "Vers la droite": False, "À la fin": False}, sensitive


def activate(ident, text):
    def run():
        menu, entries = move_menu(ident)
        entries[text].activate()
        close(menu)
    return run


def moved_first():
    assert shown()[:2] == [PINS[4], PINS[0]], shown()
    assert saved()[0] == PINS[4] and "absente.desktop" in saved(), saved()


def moved_last():
    assert shown()[-1] == PINS[1], shown()
    assert saved()[-1] == PINS[1], saved()


def reopen():
    app.hide()
    app.activate()


def kept():
    assert shown() == [PINS[4], PINS[0], PINS[2], PINS[3], PINS[5], PINS[6], PINS[7], PINS[1]], shown()


def centre(tile):
    x, y = tile.get_window().get_root_coords(0, 0)
    area = tile.get_allocation()
    return x + area.x + area.width // 2, y + area.y + area.height // 2


def xdotool(*args):
    subprocess.run(["xdotool", *map(str, args)], check=True)


def drag():
    # Septième icône (2e ligne) glissée sur la deuxième : elle prend sa place.
    order = shown()
    source, target = tiles()[6], tiles()[1]
    (sx, sy), (tx, ty) = centre(source), centre(target)
    drag.expected = order[:1] + [order[6]] + order[1:6] + order[7:]
    launched.clear()
    xdotool("mousemove", sx, sy)
    xdotool("mousedown", 1)
    for step_index in range(1, 11):
        xdotool("mousemove", sx + (tx - sx) * step_index // 10, sy + (ty - sy) * step_index // 10)
    if shots:
        GLib.timeout_add(200, lambda: shoot("glisser") and False)
        GLib.timeout_add(600, lambda: xdotool("mouseup", 1) and False)
    else:
        xdotool("mouseup", 1)


def dropped():
    assert shown() == drag.expected, (shown(), drag.expected)
    assert saved()[:2] == drag.expected[:2], saved()
    assert not launched, f"application lancée pendant le glisser : {launched}"


def click():
    launched.clear()
    x, y = centre(tiles()[2])
    xdotool("mousemove", x, y, "click", 1)


def clicked():
    assert launched == [shown()[2]], launched


def shoot(name):
    if not shots: return True
    window = app.window.get_window()
    width, height = window.get_width(), window.get_height()
    pixbuf = Gdk.pixbuf_get_from_window(window, 0, 0, width, height)
    pixbuf.savev(str(Path(shots) / f"{name}.png"), "png", [], [])
    return True


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


if shutil.which("xdotool") is None:
    raise SystemExit("xdotool manquant")
steps += [
    step(app.activate, lambda: (shoot("avant"), None)[1], delay=900),
    step(lambda: None, first_greyed),
    step(lambda: None, last_greyed),
    step(activate(PINS[4], "Au début"), moved_first),
    step(activate(PINS[1], "À la fin"), moved_last),
    step(reopen, kept, delay=700),
    step(drag, dropped, delay=1200 if shots else 600),
    step(lambda: None, lambda: (shoot("apres"), None)[1]),
    step(click, clicked),
]
GLib.timeout_add(100, steps.pop(0))
app.run([])
if failures:
    raise failures[0]
print("OK: clic droit « Déplacer » (entrées grisées aux extrémités), ordre enregistré et retrouvé, glisser-déposer sans lancement")
