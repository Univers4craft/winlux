#!/usr/bin/python3
"""Vue des tâches au clavier (#3075).

Windows + Tab ouvre la vue avec la première fenêtre choisie : Entrée
l'ouvre ; flèche droite puis Entrée ouvre la suivante ; Suppr ferme la
fenêtre choisie ; Tab passe à la bande des bureaux, flèche droite puis
Entrée mène au bureau suivant.

À lancer dans un Xvfb dédié, avec openbox, wmctrl et xdotool :
  xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-clavier.py
"""
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

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


def ours():
    return sorted(line.split(None, 3)[3] for line in out("wmctrl", "-l").splitlines()
                  if len(line.split(None, 3)) == 4 and line.split(None, 3)[3].startswith("Fenêtre "))


def active():
    return out("xdotool", "getactivewindow", "getwindowname").strip()


def current_desktop():
    return next((line.split()[0] for line in out("wmctrl", "-d").splitlines() if " * " in line), "?")


def wait(condition, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.05)
    return condition()


def keys(*names):
    for name in names:
        subprocess.run(["xdotool", "key", name])
        time.sleep(0.25)


def open_view():
    subprocess.Popen([sys.executable, "-B", str(VIEW)], env=env)
    shown = wait(visible, 15)
    time.sleep(0.6)
    return shown


WINDOWS = """
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
for n in range(3):
    w = Gtk.Window(title=f"Fenêtre {n + 1}")
    w.set_default_size(320, 220)
    w.connect("delete-event", lambda w, *_: w.destroy() or True)
    w.show_all()
Gtk.main()
"""

home = Path(tempfile.mkdtemp(prefix="winlux-taskview-clavier-"))
runtime = home / "run"
runtime.mkdir(mode=0o700)
env = dict(os.environ, HOME=str(home), XDG_RUNTIME_DIR=str(runtime), NO_AT_BRIDGE="1", WINLUX_LANG="fr")
wm = subprocess.Popen(["openbox", "--config-file", str(RC)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
apps = subprocess.Popen([sys.executable, "-c", WINDOWS], env=env)
try:
    time.sleep(0.8)
    subprocess.run(["wmctrl", "-n", "2"])
    subprocess.run(["wmctrl", "-s", "0"])
    ok(wait(lambda: len(ours()) == 3, 15), "trois fenêtres ouvertes")

    # 1. Entrée ouvre la fenêtre choisie d'office.
    ok(open_view(), "vue des tâches ouverte")
    keys("Return")
    wait(lambda: not alive(), 5)
    first = active()
    print(f"  Entrée : fenêtre active « {first} », vues vivantes {alive()}", flush=True)
    ok(not alive() and first.startswith("Fenêtre "), "Entrée ouvre la fenêtre choisie et ferme la vue (#3075)")

    # 2. Flèche droite puis Entrée : une autre fenêtre.
    open_view()
    keys("Right", "Return")
    wait(lambda: not alive(), 5)
    second = active()
    print(f"  Droite + Entrée : fenêtre active « {second} »", flush=True)
    ok(not alive() and second.startswith("Fenêtre ") and second != first,
       "flèche droite puis Entrée ouvre la fenêtre voisine (#3075)")

    # 3. Suppr ferme la fenêtre choisie, la vue reste ouverte.
    open_view()
    before = ours()
    keys("Delete")
    wait(lambda: len(ours()) == len(before) - 1, 5)
    print(f"  Suppr : {before} -> {ours()}, vue visible {visible()}", flush=True)
    ok(len(ours()) == len(before) - 1 and visible(), "Suppr ferme la fenêtre choisie, la vue reste (#3075)")

    # 4. Tab passe à la bande des bureaux ; droite puis Entrée mène au bureau 2.
    keys("Tab", "Right", "Return")
    wait(lambda: current_desktop() == "1" and not alive(), 5)
    print(f"  Tab + Droite + Entrée : bureau {current_desktop()}, vues vivantes {alive()}", flush=True)
    ok(current_desktop() == "1" and not alive(), "Tab puis flèche droite et Entrée mène au bureau suivant (#3075)")
finally:
    for line in alive():
        os.kill(int(line.split()[0]), 15)
    apps.terminate()
    wm.terminate()

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("vue des tâches au clavier : OK")
