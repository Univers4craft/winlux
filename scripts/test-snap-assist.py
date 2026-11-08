#!/usr/bin/python3
"""Assistant d'ancrage comme Windows 11 (#585) : vrai openbox sous Xvfb.

    xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-snap-assist.py

Vérifie la logique (cases proposées après une moitié, un coin ou une case
de disposition ; cases déjà prises ignorées ; cartes lisibles du 4K au
1366x768 ; icônes _NET_WM_ICON), puis sous openbox : l'assistant remplit la
place libre, propose d'abord les fenêtres non ancrées, place la carte choisie
au clavier, enchaîne les quarts libres, ne s'ouvre pas sur un écran plein,
se ferme avec Échap. Groupes d'ancrage (#597) : formés par l'assistant ou
quand l'écran est rempli, vignette au survol du bouton de la barre des tâches,
clic qui restaure le groupe réduit, groupe défait par un agrandissement.
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
# Groupes d'ancrage (#597).
props = {"a": ("g1", "half:left"), "b": ("g1", "half:right"), "c": ("g1", ""), "d": ("g2", "half:left")}
assert snap["group_members"]("a", props) == ["a", "b"] and snap["group_members"]("c", props) == []
assert snap["group_members"]("d", props) == [], "un groupe d'une seule fenêtre ne compte pas"
buttons = [(7, (100, 728, 150, 40)), (8, (250, 728, 150, 40))]
assert snap["button_under"]((260, 740), buttons) == 8 and snap["button_under"]((50, 740), buttons) is None
assert snap["popup_origin"]((100, 728, 150, 40), (300, 200), (0, 0, 1366, 768)) == (25, 520), "au-dessus d'un bouton du bas"
clear = snap["TOOLTIP_CLEARANCE"]
assert snap["popup_origin"]((100, 728, 150, 40), (300, 200), (0, 0, 1366, 768), clear)[1] + 200 <= 728 - 40, \
    "place laissée à l'infobulle du bouton (#675)"
assert snap["popup_origin"]((0, 0, 150, 27), (300, 200), (0, 0, 1366, 768)) == (0, 35), "en dessous d'un bouton du haut"
assert snap["popup_origin"]((1300, 728, 60, 40), (300, 200), (0, 0, 1366, 768))[0] == 1066, "sans sortir de l'écran"


class Entry:
    def __init__(self, name, wm_class): self.name, self.wm_class = name, wm_class
    def get_startup_wm_class(self): return self.wm_class


entries = [Entry("debian-xterm", "XTerm"), Entry("winlux-terminal", "winlux-terminal"), Entry("vide", None)]
assert snap["desktop_for_class"](entries, ["winlux-terminal", "XTerm"]).name == "winlux-terminal", \
    "Terminal Winlux : son lanceur, pas celui d'xterm (fiche 597)"
assert snap["desktop_for_class"](entries, ["xterm", "XTerm"]).name == "debian-xterm"
assert snap["desktop_for_class"](entries, ["mousepad", "Mousepad"]) is None
assert "-name winlux-terminal" in (ROOT / "integration/winlux-terminal").read_text() \
    and "StartupWMClass=winlux-terminal\n" in (ROOT / "integration/winlux-terminal.desktop").read_text()
print("assistant d'ancrage (logique) : OK")

if not os.environ.get("DISPLAY") or not all(shutil.which(tool) for tool in ("openbox", "xdotool", "wmctrl", "xwininfo", "xprop")):
    print("openbox, xdotool ou affichage absent : assistant réel non vérifié")
    raise SystemExit(0)
try:
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
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


def open_window(title, at, minimum=None):
    script = ("import gi; gi.require_version('Gtk','3.0'); from gi.repository import Gtk;"
              f"w=Gtk.Window(title={title!r}); w.set_default_size(420,260); w.move({at[0]},{at[1]});"
              + (f"w.set_size_request({minimum[0]},{minimum[1]});" if minimum else "") +
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
    group = snap["get_prop"](one, "_WINLUX_SNAP_GROUP")
    assert group and snap["get_prop"](placed[0], "_WINLUX_SNAP_GROUP") == group, "fenêtres placées par l'assistant : un groupe"

    # 5. Groupes d'ancrage (#597) : moitié gauche puis moitié droite → groupe,
    #    réduites puis restaurées ensemble depuis la vignette du bouton de la barre.
    for window in (one, two, three):
        snap["restore"](window)
    assert not snap["get_prop"](one, "_WINLUX_SNAP_GROUP"), "décrochée : plus dans le groupe"
    assert subprocess.run(TOOL + ["place", "half:left", one], env=env, timeout=10).returncode == 0
    assert subprocess.run(TOOL + ["drop", two, "half:right"], env=env, timeout=10).returncode == 0
    group = snap["get_prop"](one, "_WINLUX_SNAP_GROUP")
    assert group and snap["get_prop"](two, "_WINLUX_SNAP_GROUP") == group, "écran rempli à la main : un groupe aussi"
    assert snap["get_prop"](three, "_WINLUX_SNAP_GROUP") != group
    x("xdotool", "windowminimize", one)
    x("xdotool", "windowminimize", two)
    hidden = lambda w: "_NET_WM_STATE_HIDDEN" in x("xprop", "-id", w, "_NET_WM_STATE")
    assert wait(lambda: hidden(one) and hidden(two)), "fenêtres réduites"
    # Le bouton de la barre d'XFCE : _NET_WM_ICON_GEOMETRY (posée ici à la main).
    x("xprop", "-id", one, "-f", "_NET_WM_ICON_GEOMETRY", "32c", "-set", "_NET_WM_ICON_GEOMETRY", "100,720,150,40")
    start(sys.executable, "-B", str(HERE / "winlux-snap.py"), "watch")
    time.sleep(1.2)
    x("xdotool", "mousemove", "170", "740")

    def group_windows():
        return [w for w in x("xdotool", "search", "--onlyvisible", "--name", "^" + snap["_t"]("Groupe d’ancrage") + "$").split()
                if "winlux-snap-group" in x("xprop", "-id", w, "WM_WINDOW_ROLE")]

    def group_window():
        found = group_windows()
        return found[0] if found else None

    card = wait(group_window)
    assert card, "survol du bouton d'une fenêtre groupée : vignette du groupe attendue"
    left_, top_, width_, height_ = frame(card)
    assert top_ + height_ <= 720 - 40, f"vignette au-dessus du bouton et de son infobulle, {frame(card)}"
    # Aller sur la vignette puis revenir au bouton : toujours une seule vignette (#697).
    x("xdotool", "mousemove", str(left_ + width_ // 2), str(top_ + height_ // 2))
    time.sleep(0.6)
    x("xdotool", "mousemove", "170", "740")
    time.sleep(snap["HOVER_DELAY"] + 1.2)
    assert len(group_windows()) == 1, f"une seule vignette à la fois (#697), {group_windows()}"
    x("xdotool", "mousemove", str(left_ + width_ // 4), str(top_ + height_ // 2))
    time.sleep(0.3)
    x("xdotool", "click", "1")
    assert wait(lambda: not hidden(one) and not hidden(two)), "clic : les deux fenêtres du groupe reviennent"
    assert wait(lambda: near(frame(one), rect("half:left", area)) and near(frame(two), rect("half:right", area))), \
        f"chacune à sa place, {frame(one)} {frame(two)}"
    assert not group_window(), "la vignette se ferme après le clic"
    # Restauration au clavier/ligne de commande, et groupe défait par un agrandissement.
    x("xdotool", "windowminimize", two)
    assert wait(lambda: hidden(two))
    assert subprocess.run(TOOL + ["restore-group", one], env=env, timeout=10).returncode == 0
    assert wait(lambda: not hidden(two)), "restore-group : la fenêtre réduite revient"
    assert wait(lambda: near(frame(two), rect("half:right", area))), f"restore-group : à sa place, {frame(two)}"
    assert subprocess.run(TOOL + ["drop", two, "maximize"], env=env, timeout=10).returncode == 0
    assert not snap["get_prop"](two, "_WINLUX_SNAP_GROUP") and subprocess.run(TOOL + ["restore-group", one], env=env, timeout=10).returncode == 1, \
        "agrandie : le groupe est défait"
    assert not snap["get_prop"](one, "_WINLUX_SNAP_GROUP"), "la fenêtre restée seule quitte aussi le groupe (fiche 597)"
    # Agrandie par openbox lui-même (bouton Agrandir, double-clic sur la barre
    # de titre), sans winlux-snap : le groupe est défait aussi (#696).
    snap["restore"](two)
    assert subprocess.run(TOOL + ["place", "half:left", one], env=env, timeout=10).returncode == 0
    assert subprocess.run(TOOL + ["drop", two, "half:right"], env=env, timeout=10).returncode == 0
    assert snap["get_prop"](two, "_WINLUX_SNAP_GROUP"), "regroupées"
    x("wmctrl", "-i", "-r", two, "-b", "add,maximized_vert,maximized_horz")
    assert wait(lambda: snap["is_maximized"](two)), "agrandie par openbox"
    x("xdotool", "mousemove", "600", "300")
    time.sleep(0.8)
    x("xdotool", "mousemove", "170", "740")
    assert not wait(group_window, snap["HOVER_DELAY"] + 1.5), "agrandie par openbox : plus de vignette de groupe (#696)"
    assert subprocess.run(TOOL + ["restore-group", one], env=env, timeout=10).returncode == 1, "agrandie par openbox : groupe défait (#696)"
    assert not snap["get_prop"](two, "_WINLUX_SNAP_GROUP"), "agrandie par openbox : plus de groupe sur la fenêtre (#696)"
    assert not snap["get_prop"](one, "_WINLUX_SNAP_GROUP"), "agrandie par openbox : la fenêtre restée seule non plus (fiche 597)"

    # 6. Fenêtre plus large que sa moitié (Paramètres : 720 px minimum en
    #    1366x768, #647) : elle occupe quand même la gauche, aucun quart
    #    gauche n'est proposé après un coin à droite.
    for window in (one, two, three):
        snap["restore"](window)
    wide = open_window("fenetre-large", (200, 150), (760, 300))
    time.sleep(0.5)
    assert subprocess.run(TOOL + ["place", "half:left", wide], env=env, timeout=10).returncode == 0
    assert subprocess.run(TOOL + ["place", "half:right", two], env=env, timeout=10).returncode == 0
    time.sleep(0.5)
    assert frame(wide)[2] > area[2] // 2 + 40, f"la fenêtre large déborde de sa moitié, {frame(wide)}"
    helper = subprocess.Popen(TOOL + ["drop", three, "quarter:tr"], env=env, stderr=open(work / "log", "a"))
    processes.append(helper)
    assert helper.wait(timeout=10) == 0
    assert not wait(assist_window, 1.5), "gauche occupée par une fenêtre large : pas d'assistant à gauche (#647)"

    # 7. Vignette ouverte pendant qu'on remplit les quarts au clavier (fiche
    #    597, étape 6) : elle se refait avec la rangée du bas.
    for window in (one, two, three, wide):
        snap["restore"](window)
    for window, zone in ((one, "quarter:tl"), (two, "quarter:tr")):
        assert subprocess.run(TOOL + ["place", zone, window], env=env, timeout=10).returncode == 0
    snap["form_group"]([one, two])
    x("xdotool", "mousemove", "600", "300")
    time.sleep(0.8)
    x("xdotool", "mousemove", "170", "740")
    card = wait(group_window)
    assert card, "groupe de deux quarts : vignette attendue"
    from gi.repository import Gdk, Gtk  # noqa: F401 (Gtk ouvre l'affichage)

    def cell_rows():
        """Rangées de cases dans la colonne de gauche de la vignette (couleur de button.window)."""
        left_, top_, width_, height_ = frame(card)
        column = left_ + width_ // 2 - 124
        shot = Gdk.pixbuf_get_from_window(Gdk.get_default_root_window(), column, top_, 1, height_)
        pixels, stride, step = shot.get_pixels(), shot.get_rowstride(), shot.get_n_channels()
        rows, run = 0, 0
        for y in range(height_):
            r, g, b = pixels[y * stride:y * stride + 3]
            inside = abs(r - 0x2a) < 8 and abs(g - 0x30) < 8 and abs(b - 0x40) < 8
            run = run + 1 if inside else 0
            if run == 20: rows += 1
        return rows

    assert wait(lambda: cell_rows() == 1, 3), f"rangée du haut seule, {cell_rows()}"
    assert subprocess.run(TOOL + ["place", "quarter:bl", three], env=env, timeout=10).returncode == 0
    snap["form_group"]([one, two, three])
    assert wait(lambda: cell_rows() == 2, 4), f"vignette ouverte : la case du bas apparaît (fiche 597), {cell_rows()}"
    assert group_windows() == [card], "la même vignette, refaite"
finally:
    for process in reversed(processes):
        if process.poll() is None: process.terminate()
    for process in processes:
        try: process.wait(timeout=5)
        except subprocess.TimeoutExpired: process.kill()
    log = (work / "log").read_text(errors="replace") if (work / "log").exists() else ""
    shutil.rmtree(work, ignore_errors=True)
    if "Traceback" in log or os.environ.get("WINLUX_TEST_VERBOSE"): print(log[-3000:], file=sys.stderr)
print("assistant et groupes d'ancrage (openbox réel) : OK")
