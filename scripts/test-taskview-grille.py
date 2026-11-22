#!/usr/bin/python3
"""Vue des tâches : grille des fenêtres et bande des bureaux à l'écran (#3066).

Avec 3, 12 puis 30 fenêtres sur le bureau courant, la vue (winlux-taskview)
tient dans l'écran et la bande des bureaux reste visible en bas : le bureau
courant y est encadré de violet (#b69cff). Avant #3066, trois fenêtres
suffisaient à pousser la bande hors d'un écran de 768 px.

À lancer dans un Xvfb dédié, avec openbox, wmctrl et xdotool :
  xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-grille.py
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


def view_ids():
    return subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Vue des tâches"],
                          capture_output=True, text=True).stdout.split()


def wait(condition, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.05)
    return condition()


def geometry(window):
    out = subprocess.run(["xwininfo", "-id", window], capture_output=True, text=True).stdout
    values = {}
    for line in out.splitlines():
        key, _, value = line.strip().partition(":")
        if key in ("Absolute upper-left Y", "Height", "Width"):
            values[key] = int(value)
    return values


def strip_rows(screen_h):
    """Lignes de l'écran où apparaît le cadre violet du bureau courant."""
    root = Gdk.get_default_root_window()
    width = root.get_width()
    pixbuf = Gdk.pixbuf_get_from_window(root, 0, 0, width, screen_h)
    data, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()
    rows = []
    for y in range(screen_h):
        line = data[y * stride: y * stride + width * channels]
        for x in range(0, width * channels, channels):
            r, g, b = line[x], line[x + 1], line[x + 2]
            if abs(r - 0xb6) < 12 and abs(g - 0x9c) < 12 and abs(b - 0xff) < 12:
                rows.append(y)
                break
    return rows


WINDOW = """
import sys, gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
for n in range(int(sys.argv[1])):
    w = Gtk.Window(title=f"Fenêtre {n + 1}")
    w.set_default_size(320, 220)
    w.add(Gtk.Label(label=f"Fenêtre {n + 1}"))
    w.show_all()
Gtk.main()
"""

screen = Gdk.Screen.get_default()
screen_w, screen_h = screen.get_width(), screen.get_height()
home = Path(tempfile.mkdtemp(prefix="winlux-taskview-grille-"))
env = dict(os.environ, HOME=str(home), NO_AT_BRIDGE="1", WINLUX_LANG="fr")
wm = subprocess.Popen(["openbox", "--config-file", str(RC)], env=env,
                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
apps = []
try:
    time.sleep(0.8)
    shown = 0
    for count in (3, 12, 30):
        apps.append(subprocess.Popen([sys.executable, "-c", WINDOW, str(count - shown)], env=env))
        shown = count
        ok(wait(lambda: len(subprocess.run(["wmctrl", "-l"], capture_output=True, text=True).stdout.splitlines()) >= count, 15),
           f"{count} fenêtres ouvertes sur le bureau courant")
        view = subprocess.Popen([sys.executable, "-B", str(VIEW)], env=env)
        try:
            appeared = wait(view_ids, 15)
            time.sleep(0.6)
            ids = view_ids()
            size = geometry(ids[0]) if ids else {}
            print(f"  vue {count} fenêtres : {size}, écran {screen_w}x{screen_h}", flush=True)
            rows = strip_rows(screen_h)
            print(f"  cadre du bureau courant aux lignes {rows[:1]}…{rows[-1:]}", flush=True)
            ok(appeared and size.get("Height", 10 ** 6) <= screen_h and size.get("Width", 10 ** 6) <= screen_w,
               f"vue des tâches dans l'écran avec {count} fenêtres (#3066)")
            ok(bool(rows) and rows[-1] < screen_h - 4,
               f"bande des bureaux visible avec {count} fenêtres (#3066)")
        finally:
            view.terminate()
            view.wait(5)
            time.sleep(0.3)
finally:
    for app in apps:
        app.terminate()
    wm.terminate()

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("vue des tâches, grille : OK")
