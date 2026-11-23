#!/usr/bin/python3
"""Vue des tâches : « Déplacer vers › Nouveau bureau » nomme le bureau créé (#3350).

Un seul bureau, une fenêtre. Dans la vue, clic droit sur sa carte, puis
« Déplacer vers Nouveau bureau » : un deuxième bureau est créé, la fenêtre y
part, et ce bureau s'appelle « Bureau 2 », comme avec « Nouveau bureau »
(Win+Ctrl+D). Sans nom, openbox le baptise « desktop 2 », en anglais, et
c'est ce nom qu'il affiche en passant d'un bureau à l'autre (Win+Ctrl+→).

À lancer dans un Xvfb dédié, avec openbox, wmctrl et xdotool :
  xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-nouveau-bureau.py
"""

import os
import re
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




def names():
    text = out("xprop", "-root", "_NET_DESKTOP_NAMES")
    return re.findall(r'"((?:[^"\\]|\\.)*)"', text.split("=", 1)[-1]) if "=" in text else []


WINDOW = """
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
w = Gtk.Window(title="Fenêtre à déplacer")
w.set_default_size(360, 240)
w.show_all()
Gtk.main()
"""

home = Path(tempfile.mkdtemp(prefix="winlux-taskview-nouveau-bureau-"))
runtime = home / "run"
runtime.mkdir(mode=0o700)
env = dict(os.environ, HOME=str(home), XDG_RUNTIME_DIR=str(runtime), NO_AT_BRIDGE="1", WINLUX_LANG="fr")
wm = subprocess.Popen(["openbox", "--config-file", str(RC)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
app = subprocess.Popen([sys.executable, "-c", WINDOW], env=env)
try:
    time.sleep(0.8)
    subprocess.run(["wmctrl", "-n", "1"])
    ok(wait(lambda: desktop_of("Fenêtre à déplacer") == "0", 15), "fenêtre ouverte, un seul bureau")

    subprocess.Popen([sys.executable, "-B", str(VIEW)], env=env)
    ok(wait(visible, 15), "vue des tâches ouverte")
    time.sleep(0.8)
    card = colour_box((0x34, 0x3b, 0x4e), True) or colour_box((0x2a, 0x30, 0x40), True)
    if card:
        subprocess.run(["xdotool", "mousemove", str((card[0] + card[2]) // 2), str((card[1] + card[3]) // 2), "click", "3"])
        time.sleep(0.6)
        # Un seul bureau : le menu n'a que « Déplacer vers Nouveau bureau ».
        subprocess.run(["xdotool", "key", "Down"])
        time.sleep(0.2)
        subprocess.run(["xdotool", "key", "Return"])
    wait(lambda: desktop_of("Fenêtre à déplacer") == "1", 5)
    time.sleep(0.4)
    print(f"  fenêtre sur le bureau {desktop_of('Fenêtre à déplacer')}, bureau courant {current_desktop()}, "
          f"noms {names()}", flush=True)
    ok(card is not None and desktop_of("Fenêtre à déplacer") == "1", "la fenêtre part sur le nouveau bureau")
    listed = names()
    ok(len(listed) >= 2 and listed[1] == "Bureau 2",
       "le bureau créé par « Déplacer vers › Nouveau bureau » s'appelle « Bureau 2 » (#3350)")
finally:
    for line in alive():
        os.kill(int(line.split()[0]), 15)
    app.terminate()
    wm.terminate()

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("vue des tâches, déplacer vers un nouveau bureau : OK")
