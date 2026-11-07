#!/usr/bin/python3
"""Assistant d'ancrage comme Windows 11 (#585) : vrai openbox sous Xvfb.

    xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-snap-assist.py

Vérifie la logique (cases proposées après une moitié, un coin ou une case
de disposition ; cases déjà prises ignorées ; cartes lisibles du 4K au
1366x768 ; icônes _NET_WM_ICON), puis sous openbox : l'assistant remplit la
place libre, propose d'abord les fenêtres non ancrées, place la carte choisie
au clavier, enchaîne les quarts libres, ne s'ouvre pas sur un écran plein,
se ferme avec Échap.
"""
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
snap = runpy.run_path(str(HERE / "winlux-snap.py"), run_name="snap_assist_test")

# ── Logique pure ──────────────────────────────────────────────────────────
following = snap["following_zones"]
assert following("half:left") == ["half:right"] and following("wide:right") == ["wide:left"]
assert following("quarter:tr") == ["quarter:tl", "quarter:bl", "quarter:br"]
assert following("third:2") == ["third:1", "third:3"]
area = (0, 0, 1366, 728)
rect = snap["zone_rect"]
free = snap["free_zones"]
left = rect("half:left", area)
assert free(following("quarter:tr"), area, [left]) == ["quarter:br"], "moitié gauche prise : seul le quart bas droit"
assert free(["half:right"], area, [rect("half:right", area)]) == [], "écran plein : rien à proposer"
assert free(["half:right"], area, [rect("quarter:tr", area)]) == ["half:right"], "un quart ne remplit pas une moitié"
assert free(["third:1", "third:3"], area, [rect("wide:left", area)]) == ["third:3"]
for (w, h, scale) in ((683, 364, 1), (683, 728, 1), (1920, 2120, 2), (960, 1060, 1), (3840, 2120, 2)):
    columns, card_w, card_h = snap["card_grid"](w, h, scale, 6)
    logical = w / scale
    assert 1 <= columns <= 4 and columns * card_w + 16 * (columns - 1) <= logical - 2 * 18, (w, h, scale, columns, card_w)
    assert card_w >= 150 and card_h >= 90, ("cartes lisibles", w, h, scale, card_w, card_h)
assert snap["card_grid"](683, 364, 1, 6)[0] >= 2, "deux cartes de front dans un quart en 1366x768"
assert snap["card_grid"](1920, 2120, 2, 1)[0] == 1, "une seule fenêtre : une seule colonne"
icons = snap["parse_icons"]([2, 2, 1, 2, 3, 4, 1, 1, 9, 99, 99])
assert [(w, h) for w, h, _ in icons] == [(2, 2), (1, 1)], icons
assert snap["best_icon"](icons, 2)[0] == 2 and snap["best_icon"](icons, 64)[0] == 2 and snap["best_icon"]([], 8) is None
geometries = {1: (0, 0, 100, 100), 2: (50, 50, 100, 100), 3: (500, 0, 50, 50)}
assert not snap["unobscured"](1, [1, 2, 3], geometries) and snap["unobscured"](2, [1, 2, 3], geometries)
assert snap["unobscured"](3, [3, 1, 2], geometries) and not snap["unobscured"](9, [1], geometries)
assert snap["order_candidates"]([("a", ""), ("b", ""), ("c", "")], {"a"}) == [("b", ""), ("c", ""), ("a", "")]
assert all(len(row) == 3 and all(row) for row in snap["_TR"].values()), "quatre langues"
for row in (snap["_TR"]["Emplacement {n} sur {total} · Échap pour fermer"],):
    assert all("{n}" in text and "{total}" in text for text in row)
print("assistant d'ancrage (logique) : OK")

if not os.environ.get("DISPLAY") or not all(shutil.which(tool) for tool in ("openbox", "xdotool", "wmctrl", "xwininfo", "xprop")):
    print("openbox, xdotool ou affichage absent : assistant réel non vérifié")
    raise SystemExit(0)
try:
    import gi
    gi.require_version("Gtk", "3.0")
except (ImportError, ValueError):
    print("GTK absent : assistant réel non vérifié")
    raise SystemExit(0)

work = Path(tempfile.mkdtemp(prefix="winlux-snap-assist-"))
home = work / "home"
(home / ".themes").mkdir(parents=True)
shutil.copytree(ROOT / "integration/native/themes/Winlux", home / ".themes/Winlux")
(home / ".config/openbox").mkdir(parents=True)
shutil.copy(ROOT / "integration/native/config/openbox/rc.xml", home / ".config/openbox/rc.xml")
env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"), "WINLUX_LANG": "fr"}
env.pop("WINLUX_SNAP_ASSIST", None)
processes = []
TOOL = [sys.executable, "-B", str(HERE / "winlux-snap.py")]


def start(*args):
    process = subprocess.Popen(list(args), env=env, stdout=subprocess.DEVNULL, stderr=open(work / "log", "a"))
    processes.append(process)
    return process


def x(*args):
    return subprocess.run(list(args), env=env, capture_output=True, text=True, timeout=10).stdout


def wait(condition, seconds=8):
    end = time.time() + seconds
    while time.time() < end:
        value = condition()
        if value: return value
        time.sleep(0.1)
    return condition()


def assist_window():
    found = [w for w in x("xdotool", "search", "--onlyvisible", "--name", "^Choisissez une fenêtre").split()
             if "winlux-snap-assist" in x("xprop", "-id", w, "WM_WINDOW_ROLE")]
    return found[0] if found else None


def frame(window):
    return snap["window_geometry"](window)


def near(a, b, slack=14):
    return a and b and all(abs(p - q) <= slack for p, q in zip(a, b))


def open_window(title, at):
    script = ("import gi; gi.require_version('Gtk','3.0'); from gi.repository import Gtk;"
              f"w=Gtk.Window(title={title!r}); w.set_default_size(420,260); w.move({at[0]},{at[1]});"
              "w.connect('destroy', Gtk.main_quit); w.show_all(); Gtk.main()")
    start(sys.executable, "-c", script)
    found = wait(lambda: x("xdotool", "search", "--name", f"^{title}$").split()[:1])
    assert found, f"fenêtre {title} absente"
    return found[0]


try:
    start("openbox", "--config-file", str(home / ".config/openbox/rc.xml"))
    time.sleep(1)
    one = open_window("fenetre-un", (100, 100))
    two = open_window("fenetre-deux", (300, 200))
    three = open_window("fenetre-trois", (500, 300))
    time.sleep(0.6)
    area = snap["work_areas"]()[0]

    # 1. Moitié gauche (sans assistant), puis coin haut droit : l'assistant
    #    ne propose que le quart bas droit, et remplit cette case.
    assert subprocess.run(TOOL + ["place", "half:left", one], env=env, timeout=10).returncode == 0
    time.sleep(0.5)
    helper = subprocess.Popen(TOOL + ["drop", two, "quarter:tr"], env=env, stderr=open(work / "log", "a"))
    processes.append(helper)
    popup = wait(assist_window)
    assert popup, "coin haut droit : l'assistant d'ancrage attendu"
    target = rect("quarter:br", area)
    assert wait(lambda: near(frame(popup), target, 16)), f"l'assistant remplit le quart bas droit {target}, {frame(popup)}"
    title = x("xdotool", "getwindowname", popup).strip()
    assert title == "Choisissez une fenêtre pour cet emplacement", title
    # Fenêtre non ancrée d'abord : Entrée place « fenetre-trois ».
    x("xdotool", "windowactivate", "--sync", popup)
    time.sleep(0.3)
    x("xdotool", "key", "Return")
    assert helper.wait(timeout=10) == 0
    assert wait(lambda: near(frame(three), target)), f"fenêtre trois dans le quart bas droit, {frame(three)}"
    assert snap["get_prop"](three, "_WINLUX_SNAP") == "quarter:br"
    assert not assist_window(), "plus de case libre : l'assistant ne revient pas"

    # 2. Écran plein (moitié gauche + moitié droite) : pas d'assistant.
    started = time.time()
    assert subprocess.run(TOOL + ["drop", two, "half:right"], env=env, timeout=10).returncode == 0
    assert time.time() - started < 8 and not assist_window(), "écran plein : pas d'assistant"

    # 3. Coin seul sur un écran vide : les trois autres quarts, l'un après l'autre.
    for window in (one, two, three):
        snap["restore"](window)
    time.sleep(0.4)
    helper = subprocess.Popen(TOOL + ["drop", one, "quarter:tl"], env=env, stderr=open(work / "log", "a"))
    processes.append(helper)
    popup = wait(assist_window)
    assert popup and wait(lambda: near(frame(popup), rect("quarter:tr", area), 16)), f"premier quart libre : haut droit, {frame(popup)}"
    x("xdotool", "windowactivate", "--sync", popup)
    time.sleep(0.3)
    x("xdotool", "key", "Return")
    second = wait(lambda: (lambda found: found if found and found != popup else None)(assist_window()))
    assert second and wait(lambda: near(frame(second), rect("quarter:bl", area), 16)), f"puis le quart bas gauche, {second and frame(second)}"
    # 4. Échap ferme l'assistant et arrête la suite.
    x("xdotool", "windowactivate", "--sync", second)
    time.sleep(0.3)
    x("xdotool", "key", "Escape")
    assert helper.wait(timeout=10) == 0
    assert not wait(lambda: assist_window(), 1), "Échap ferme l'assistant"
    placed = [w for w in (two, three) if snap["get_prop"](w, "_WINLUX_SNAP") == "quarter:tr"]
    assert len(placed) == 1 and near(frame(placed[0]), rect("quarter:tr", area)), "la carte choisie va dans le quart haut droit"
finally:
    for process in reversed(processes):
        if process.poll() is None: process.terminate()
    for process in processes:
        try: process.wait(timeout=5)
        except subprocess.TimeoutExpired: process.kill()
    log = (work / "log").read_text(errors="replace") if (work / "log").exists() else ""
    shutil.rmtree(work, ignore_errors=True)
    if "Traceback" in log or os.environ.get("WINLUX_TEST_VERBOSE"): print(log[-3000:], file=sys.stderr)
print("assistant d'ancrage (openbox réel) : OK")
