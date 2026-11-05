#!/usr/bin/python3
"""Ancrage à la souris (#460) : vrai openbox sous Xvfb, glisser au xdotool.

    xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-snap-mouse.py

Vérifie : bord gauche → moitié, coin → quart, haut → agrandie, barre des
dispositions → case choisie, fenêtre ancrée décrochée → taille d'avant,
survol du bouton Agrandir → grille des dispositions. Plus la logique pure
(zones, barre, rang du bouton) sans affichage.
"""
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
snap = runpy.run_path(str(HERE / "winlux-snap.py"), run_name="snap_mouse_test")

# ── Logique pure ──────────────────────────────────────────────────────────
monitor = (0, 0, 1366, 768)
zone = snap["drag_zone"]
assert zone((0, 400), monitor) == "half:left" and zone((1365, 400), monitor) == "half:right"
assert zone((0, 10), monitor) == "quarter:tl" and zone((1365, 760), monitor) == "quarter:br"
assert zone((600, 0), monitor) == "maximize" and zone((600, 400), monitor) is None
assert zone((1366 + 0, 300), (1366, 0, 1920, 1080)) == "half:left", "deuxième écran"
bar, cells = snap["snap_bar"](monitor)
assert bar[0] > 0 and bar[0] + bar[2] < 1366 and abs((bar[0] + bar[2] / 2) - 683) <= 1, bar
assert {name for name, _ in cells} == {z for layout in snap["LAYOUTS"] for z in layout}
for name, (x, y, w, h) in cells:
    assert bar[0] <= x and x + w <= bar[0] + bar[2] and bar[1] <= y and y + h <= bar[1] + bar[3], name
    assert snap["bar_zone"]((x + w // 2, y + h // 2), cells) == name
big, _ = snap["snap_bar"]((0, 0, 3840, 2160), 2)
assert big[2] == 2 * bar[2], "barre à l'échelle 2 en 4K"
assert snap["bar_wanted"]((683, 20), monitor, False) and not snap["bar_wanted"]((50, 20), monitor, False)
assert not snap["bar_wanted"]((683, 100), monitor, False) and snap["bar_wanted"]((683, 100), monitor, True)
assert snap["float_geometry"]((1024, 5), (683, 0, 683, 728), (300, 200, 800, 500)) == (625, 0, 800, 500)
rank = snap["maximize_button_rank"]
assert rank("NLIMC") == 1 and rank("NLMIC") == 2 and rank("LC") is None and rank("MLC") is None
assert all(len(r) == 3 and all(r) for r in snap["_TR"].values())
for script in ("winlux-user-session.sh", "winlux-native-session.sh"):
    assert "winlux-snap watch" in (HERE / script).read_text(), script
print("ancrage à la souris (logique) : OK")

if not os.environ.get("DISPLAY") or not all(shutil.which(tool) for tool in ("openbox", "xdotool", "wmctrl", "xwininfo", "xprop")):
    print("openbox, xdotool ou affichage absent : glisser réel non vérifié")
    raise SystemExit(0)
try:
    import gi
    gi.require_version("Gtk", "3.0")
except (ImportError, ValueError):
    print("GTK absent : glisser réel non vérifié")
    raise SystemExit(0)

work = Path(tempfile.mkdtemp(prefix="winlux-snap-mouse-"))
home = work / "home"
(home / ".themes").mkdir(parents=True)
shutil.copytree(ROOT / "integration/native/themes/Winlux", home / ".themes/Winlux")
(home / ".config/openbox").mkdir(parents=True)
shutil.copy(ROOT / "integration/native/config/openbox/rc.xml", home / ".config/openbox/rc.xml")
env = {**os.environ, "HOME": str(home), "WINLUX_SNAP_ASSIST": "0", "XDG_CONFIG_HOME": str(home / ".config")}
processes = []


def start(*args):
    process = subprocess.Popen(list(args), env=env, stdout=subprocess.DEVNULL, stderr=open(work / "log", "a"))
    processes.append(process)
    return process


def x(*args):
    return subprocess.run(list(args), env=env, capture_output=True, text=True, timeout=10).stdout


def wait(condition, seconds=6):
    end = time.time() + seconds
    while time.time() < end:
        value = condition()
        if value: return value
        time.sleep(0.1)
    return condition()


def frame(window):
    return snap["window_geometry"](window)


def drag(window, to, steps=12):
    left, top, width, _ = frame(window)
    sx, sy = left + width // 3, top + 12
    x("xdotool", "mousemove", str(sx), str(sy))
    time.sleep(0.2)
    x("xdotool", "mousedown", "1")
    time.sleep(0.15)
    for step in range(1, steps + 1):
        x("xdotool", "mousemove", str(sx + (to[0] - sx) * step // steps), str(sy + (to[1] - sy) * step // steps))
        time.sleep(0.04)
    time.sleep(0.5)
    x("xdotool", "mouseup", "1")
    time.sleep(0.8)


try:
    start("openbox", "--config-file", str(home / ".config/openbox/rc.xml"))
    time.sleep(1)
    script = ("import gi; gi.require_version('Gtk','3.0'); from gi.repository import Gtk;"
              "w=Gtk.Window(title='ancrage-souris'); w.set_default_size(500,300); w.move(300,200);"
              "w.connect('destroy', Gtk.main_quit); w.show_all(); Gtk.main()")
    start(sys.executable, "-c", script)
    window = wait(lambda: x("xdotool", "search", "--name", "^ancrage-souris$").split()[:1])
    assert window, "fenêtre de test absente"
    window = window[0]
    time.sleep(0.5)
    x("wmctrl", "-n", "2")  # deux bureaux : le bord ne doit pas faire changer de bureau
    start(sys.executable, "-B", str(HERE / "winlux-snap.py"), "watch")
    time.sleep(1.5)
    area = snap["work_areas"]()[0]
    before = frame(window)

    drag(window, (0, 400))
    got = wait(lambda: frame(window) if abs(frame(window)[2] - area[2] // 2) <= 4 else None)
    assert got and got[0] <= 2, f"bord gauche : moitié attendue, {frame(window)}"
    assert wait(lambda: snap["get_prop"](window, "_WINLUX_SNAP") == "half:left")
    assert re.search(r"^0\s+\*", x("wmctrl", "-d"), re.M), "bord gauche : openbox est passé au bureau voisin"

    drag(window, (700, 400))
    got = wait(lambda: frame(window) if abs(frame(window)[2] - before[2]) <= 4 else None)
    assert got and abs(got[3] - before[3]) <= 4, f"décrochée : taille d'avant {before[2:]} attendue, {frame(window)}"
    assert wait(lambda: snap["get_prop"](window, "_WINLUX_SNAP") == "")

    drag(window, (1365, 760))
    got = wait(lambda: frame(window) if abs(frame(window)[2] - area[2] // 2) <= 4 and abs(frame(window)[3] - area[3] // 2) <= 4 else None)
    assert got and got[0] >= area[2] // 2 - 4, f"coin bas droit : quart attendu, {frame(window)}"
    assert re.search(r"^0\s+\*", x("wmctrl", "-d"), re.M), "bord droit : openbox est passé au bureau voisin"

    drag(window, (300, 0))
    assert wait(lambda: snap["is_maximized"](window)), "bord du haut : fenêtre agrandie attendue"
    x("wmctrl", "-i", "-r", window, "-b", "remove,maximized_vert,maximized_horz")
    time.sleep(0.5)

    # Barre des dispositions : case « tiers du milieu » (third:2).
    bar, cells = snap["snap_bar"]((0, 0) + tuple(int(v) for v in re.findall(r"(\d+)x(\d+)", x("xdotool", "getdisplaygeometry").replace(" ", "x"))[0]))
    cell = dict(cells)["third:2"]
    drag(window, (cell[0] + cell[2] // 2, cell[1] + cell[3] // 2), steps=16)
    got = wait(lambda: frame(window) if abs(frame(window)[2] - area[2] // 3) <= 4 else None)
    assert got and abs(got[0] - area[2] // 3) <= 4, f"barre des dispositions : tiers du milieu attendu, {frame(window)}"

    # Survol du bouton Agrandir : la grille s'ouvre sans clic.
    x("wmctrl", "-i", "-r", window, "-e", "0,300,200,500,300")
    time.sleep(0.6)
    left, top, width, _ = frame(window)
    found = None
    for dx in range(30, 140, 6):
        x("xdotool", "mousemove", str(left + width - dx), str(top + 14))
        time.sleep(0.15)
        if snap["hovered_maximize"](snap["_X"](), 1):
            found = dx
            break
    assert found, "bouton Agrandir introuvable sous la souris"
    popup = wait(lambda: x("xdotool", "search", "--name", "^" + snap["_t"]("Dispositions d’ancrage") + "$").split(), 4)
    assert popup, "survol d'Agrandir : grille des dispositions attendue"
    x("xdotool", "mousemove", "40", "700")
    assert wait(lambda: not x("xdotool", "search", "--name", "^" + snap["_t"]("Dispositions d’ancrage") + "$").split(), 4), \
        "la grille se ferme quand la souris s'éloigne"
finally:
    for process in reversed(processes):
        process.terminate()
    for process in processes:
        try: process.wait(timeout=5)
        except subprocess.TimeoutExpired: process.kill()
    log = (work / "log").read_text(errors="replace") if (work / "log").exists() else ""
    shutil.rmtree(work, ignore_errors=True)
    if "Traceback" in log or os.environ.get("WINLUX_TEST_VERBOSE"): print(log[-3000:], file=sys.stderr)
print("ancrage à la souris (openbox réel) : OK")
