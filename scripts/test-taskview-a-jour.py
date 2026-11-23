#!/usr/bin/python3
"""Vue des tâches : elle suit les fenêtres qui s'ouvrent et se ferment (#3347).

Vue ouverte sur deux fenêtres, A et B. B se ferme d'elle-même : sa carte
disparaît. Puis C et D s'ouvrent : leurs cartes apparaissent, sans rouvrir
la vue. Ni l'un ni l'autre ne ferme la vue.

À lancer dans un Xvfb dédié, avec openbox, wmctrl et xdotool :
  xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-a-jour.py
"""

import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import gi
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk

HERE = Path(__file__).resolve().parent
VIEW = HERE / "winlux-taskview.py"
RC = HERE.parent / "integration/native/config/openbox/rc.xml"
failures = []


def ok(condition, message):
    print(("OK    " if condition else "ÉCHEC ") + message, flush=True)
    if not condition:
        failures.append(message)


def out(*args):
    return subprocess.run(list(args), capture_output=True, text=True).stdout


def visible():
    return bool(out("xdotool", "search", "--onlyvisible", "--name", "Vue des tâches").split())


def alive():
    return [line for line in out("pgrep", "-af", "winlux-taskview.py").splitlines() if "test-taskview" not in line]


def desktop_of(title):
    for line in out("wmctrl", "-l").splitlines():
        parts = line.split(None, 3)
        if len(parts) == 4 and parts[3] == title:
            return parts[1]
    return None


def current_desktop():
    return next((line.split()[0] for line in out("wmctrl", "-d").splitlines() if " * " in line), "?")


def wait(condition, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.05)
    return condition()


def colour_box(colour, top_half):
    """Rectangle (x0, y0, x1, y1) des pixels de cette couleur, moitié haute ou basse de l'écran."""
    root = Gdk.get_default_root_window()
    width, height = root.get_width(), root.get_height()
    pixbuf = Gdk.pixbuf_get_from_window(root, 0, 0, width, height)
    data, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()
    rows = range(0, height // 2) if top_half else range(height // 2, height)
    points = [(x, y) for y in rows for x in range(0, width, 2)
              if all(abs(data[y * stride + x * channels + i] - colour[i]) < 10 for i in range(3))]
    if not points:
        return None
    return (min(x for x, _ in points), min(y for _, y in points), max(x for x, _ in points), max(y for _, y in points))




def cards():
    """Nombre de cartes de fenêtre affichées : zones du fond d'une carte
    (#2a3040, ou #343b4e quand elle a le focus) au-dessus de la bande des bureaux."""
    root = Gdk.get_default_root_window()
    width, height = root.get_width(), root.get_height()
    pixbuf = Gdk.pixbuf_get_from_window(root, 0, 0, width, height)
    data, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()
    step = 4
    seen, count = set(), 0

    def card(x, y):
        pixel = data[y * stride + x * channels: y * stride + x * channels + 3]
        return any(all(abs(pixel[i] - colour[i]) < 6 for i in range(3)) for colour in ((0x2a, 0x30, 0x40), (0x34, 0x3b, 0x4e)))

    cells = {(x, y) for y in range(0, height - 140, step) for x in range(0, width, step) if card(x, y)}
    for start in cells:
        if start in seen: continue
        stack, size = [start], 0
        seen.add(start)
        while stack:
            x, y = stack.pop()
            size += 1
            for nx, ny in ((x + step, y), (x - step, y), (x, y + step), (x, y - step)):
                if (nx, ny) in cells and (nx, ny) not in seen:
                    seen.add((nx, ny))
                    stack.append((nx, ny))
        if size > 200: count += 1
    return count


WINDOW = """
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
w = Gtk.Window(title=sys.argv[1])
w.set_default_size(360, 240)
w.show_all()
Gtk.main()
"""

home = Path(tempfile.mkdtemp(prefix="winlux-taskview-a-jour-"))
runtime = home / "run"
runtime.mkdir(mode=0o700)
env = dict(os.environ, HOME=str(home), XDG_RUNTIME_DIR=str(runtime), NO_AT_BRIDGE="1", WINLUX_LANG="fr")
wm = subprocess.Popen(["openbox", "--config-file", str(RC)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
apps = {}
try:
    time.sleep(0.8)
    for title in ("A", "B"):
        apps[title] = subprocess.Popen([sys.executable, "-c", WINDOW, title], env=env)
    ok(wait(lambda: desktop_of("A") == "0" and desktop_of("B") == "0", 15), "A et B ouvertes")

    subprocess.Popen([sys.executable, "-B", str(VIEW)], env=env)
    ok(wait(visible, 15), "vue des tâches ouverte")
    time.sleep(0.8)
    ok(cards() == 2, "deux cartes au départ")

    apps.pop("B").kill()
    ok(wait(lambda: desktop_of("B") is None, 5), "B fermée")
    wait(lambda: cards() == 1, 2)
    shown = cards()
    print(f"  après la fermeture de B : {shown} carte(s), vue visible {visible()}", flush=True)
    ok(shown == 1 and visible(), "la carte d'une fenêtre fermée disparaît de la vue ouverte (#3347)")

    for title in ("C", "D"):
        apps[title] = subprocess.Popen([sys.executable, "-c", WINDOW, title], env=env)
    ok(wait(lambda: desktop_of("C") == "0" and desktop_of("D") == "0", 15), "C et D ouvertes")
    wait(lambda: cards() == 3, 2)
    shown = cards()
    print(f"  après l'ouverture de C et D : {shown} carte(s), vue visible {visible()}", flush=True)
    ok(shown == 3 and visible(), "les fenêtres ouvertes pendant la vue y ont leur carte (#3347)")
finally:
    for line in alive():
        os.kill(int(line.split()[0]), 15)
    for app in apps.values():
        app.terminate()
    wm.terminate()

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("vue des tâches, mise à jour des fenêtres : OK")
