#!/usr/bin/python3
"""Vue des tâches : une fenêtre affichée sur tous les bureaux y figure (#3346).

Deux bureaux. La fenêtre « Partout » est mise sur tous les bureaux
(_NET_WM_DESKTOP = 0xFFFFFFFF, ce que fait « Sur tous les bureaux » du menu
de fenêtre d'openbox), « Ici » reste sur le bureau 1. Sur le bureau 1, la vue
montre deux cartes ; sur le bureau 2, une carte, et un clic dessus rend
« Partout » active.

À lancer dans un Xvfb dédié, avec openbox, wmctrl et xdotool :
  xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-tous-bureaux.py
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

home = Path(tempfile.mkdtemp(prefix="winlux-taskview-tous-bureaux-"))
runtime = home / "run"
runtime.mkdir(mode=0o700)
env = dict(os.environ, HOME=str(home), XDG_RUNTIME_DIR=str(runtime), NO_AT_BRIDGE="1", WINLUX_LANG="fr")
wm = subprocess.Popen(["openbox", "--config-file", str(RC)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
apps = []
try:
    time.sleep(0.8)
    subprocess.run(["wmctrl", "-n", "2"])
    subprocess.run(["wmctrl", "-s", "0"])
    for title in ("Partout", "Ici"):
        apps.append(subprocess.Popen([sys.executable, "-c", WINDOW, title], env=env))
    ok(wait(lambda: desktop_of("Partout") == "0" and desktop_of("Ici") == "0", 15), "deux fenêtres ouvertes sur le bureau 1")
    everywhere = out("xdotool", "search", "--name", "^Partout$").split()
    if everywhere:
        subprocess.run(["xdotool", "set_desktop_for_window", everywhere[0], str(0xFFFFFFFF)])
    ok(wait(lambda: desktop_of("Partout") == "-1", 5), "« Partout » est sur tous les bureaux")

    subprocess.Popen([sys.executable, "-B", str(VIEW)], env=env)
    ok(wait(visible, 15), "vue des tâches ouverte sur le bureau 1")
    time.sleep(0.8)
    shown = cards()
    print(f"  bureau 1 : {shown} carte(s)", flush=True)
    ok(shown == 2, "bureau 1 : la fenêtre sur tous les bureaux a sa carte à côté de l'autre (#3346)")
    for line in alive():
        os.kill(int(line.split()[0]), 15)
    wait(lambda: not alive(), 5)

    subprocess.run(["wmctrl", "-s", "1"])
    time.sleep(0.3)
    subprocess.Popen([sys.executable, "-B", str(VIEW)], env=env)
    ok(wait(visible, 15), "vue des tâches ouverte sur le bureau 2")
    time.sleep(0.8)
    shown = cards()
    print(f"  bureau 2 : {shown} carte(s)", flush=True)
    ok(shown == 1, "bureau 2 : la fenêtre sur tous les bureaux y a aussi sa carte (#3346)")
    card = colour_box((0x34, 0x3b, 0x4e), True) or colour_box((0x2a, 0x30, 0x40), True)
    if card:
        subprocess.run(["xdotool", "mousemove", str((card[0] + card[2]) // 2), str((card[1] + card[3]) // 2), "click", "1"])
    wait(lambda: not alive(), 5)
    active = out("xdotool", "getactivewindow", "getwindowname").strip()
    print(f"  clic : fenêtre active « {active} », bureau courant {current_desktop()}", flush=True)
    ok(card is not None and active == "Partout" and current_desktop() == "1", "un clic sur sa carte l'active sans changer de bureau (#3346)")
finally:
    for line in alive():
        os.kill(int(line.split()[0]), 15)
    for app in apps:
        app.terminate()
    wm.terminate()

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("vue des tâches, fenêtre sur tous les bureaux : OK")
