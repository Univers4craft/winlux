#!/usr/bin/python3
"""Vue des tâches et changement de bureau (#3074).

Un clic sur un autre bureau y mène et ferme la vue : aucune vue ne reste
cachée sur l'ancien bureau. « Nouveau bureau » garde la vue visible sur le
nouveau bureau. Un second Windows + Tab ferme la vue ouverte.

À lancer dans un Xvfb dédié, avec openbox, wmctrl et xdotool :
  xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-bureaux.py
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


def views():
    """Processus de vue vivants."""
    return [line for line in out("pgrep", "-af", "winlux-taskview.py").splitlines() if "test-taskview" not in line]


def visible():
    return bool(out("xdotool", "search", "--onlyvisible", "--name", "Vue des tâches").split())


def current_desktop():
    return next((line.split()[0] for line in out("wmctrl", "-d").splitlines() if " * " in line), "?")


def wait(condition, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.05)
    return condition()


def violet_frame():
    """(x droite, y milieu) du cadre violet du bureau courant dans la bande."""
    root = Gdk.get_default_root_window()
    width, height = root.get_width(), root.get_height()
    pixbuf = Gdk.pixbuf_get_from_window(root, 0, 0, width, height)
    data, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()
    points = []
    for y in range(height // 2, height):
        for x in range(width):
            i = y * stride + x * channels
            if abs(data[i] - 0xb6) < 12 and abs(data[i + 1] - 0x9c) < 12 and abs(data[i + 2] - 0xff) < 12:
                points.append((x, y))
    if not points:
        return None
    return max(x for x, _ in points), (min(y for _, y in points) + max(y for _, y in points)) // 2


def launch():
    return subprocess.Popen([sys.executable, "-B", str(VIEW)], env=env)


home = Path(tempfile.mkdtemp(prefix="winlux-taskview-bureaux-"))
runtime = home / "run"
runtime.mkdir(mode=0o700)
env = dict(os.environ, HOME=str(home), XDG_RUNTIME_DIR=str(runtime), NO_AT_BRIDGE="1", WINLUX_LANG="fr")
wm = subprocess.Popen(["openbox", "--config-file", str(RC)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    time.sleep(0.8)
    subprocess.run(["wmctrl", "-n", "2"])
    subprocess.run(["wmctrl", "-s", "0"])
    time.sleep(0.3)

    # 1. Clic sur « Bureau 2 » : on y passe, la vue se ferme pour de bon.
    launch()
    ok(wait(visible, 15), "vue des tâches ouverte")
    time.sleep(0.5)
    frame = violet_frame()
    print(f"  cadre du bureau courant : {frame}", flush=True)
    if frame:
        subprocess.run(["xdotool", "mousemove", str(frame[0] + 60), str(frame[1]), "click", "1"])
    wait(lambda: current_desktop() == "1" and not views(), 5)
    print(f"  bureau courant {current_desktop()}, vues vivantes {views()}", flush=True)
    ok(current_desktop() == "1", "clic sur Bureau 2 : on passe au bureau 2 (#3074)")
    ok(not views(), "clic sur Bureau 2 : aucune vue ne reste cachée sur le bureau 1 (#3074)")

    # 2. Second Windows + Tab : la vue ouverte se ferme.
    launch()
    ok(wait(visible, 15), "vue des tâches rouverte")
    time.sleep(0.3)
    second = launch()
    second.wait(10)
    wait(lambda: not views(), 5)
    print(f"  après le second appel : {views()}", flush=True)
    ok(not views(), "second appel : la vue ouverte se ferme, une seule vue à la fois (#3074)")

    # 3. « Nouveau bureau » change de bureau : la vue suit.
    launch()
    ok(wait(visible, 15), "vue des tâches ouverte une troisième fois")
    time.sleep(0.5)
    before = current_desktop()
    subprocess.run([sys.executable, "-B", str(VIEW), "new"], env=env)
    wait(lambda: current_desktop() != before, 5)
    time.sleep(0.5)
    print(f"  bureau {before} -> {current_desktop()}, vue visible {visible()}, vues vivantes {views()}", flush=True)
    ok(visible(), "nouveau bureau : la vue reste visible sur le bureau où l'on arrive (#3074)")
    for line in views():
        os.kill(int(line.split()[0]), 15)
finally:
    wm.terminate()

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("vue des tâches, bureaux : OK")
