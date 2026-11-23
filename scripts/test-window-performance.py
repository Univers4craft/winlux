#!/usr/bin/python3
"""Run with xvfb-run; measures geometry latency, not visible frame pacing."""
import json
from pathlib import Path
import os
import runpy
import statistics
import subprocess
import tempfile
import threading
import time

project = Path(__file__).resolve().parent.parent
module = runpy.run_path(str(project / "scripts/winlux-settings.py"), run_name="settings_test")
Gtk, GLib = module["Gtk"], module["GLib"]
if subprocess.run(["wmctrl", "-m"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
    raise RuntimeError("Use a dedicated Xvfb display without an existing window manager")
# Windows + flèches passe par winlux-snap (installé dans /usr/bin sur l'image) :
# ici il est pris dans scripts/, sans l'Assistant d'ancrage.
shims = tempfile.TemporaryDirectory()
os.symlink(project / "scripts/winlux-snap.py", Path(shims.name) / "winlux-snap")
manager_env = dict(os.environ, PATH=shims.name + os.pathsep + os.environ.get("PATH", ""), WINLUX_SNAP_ASSIST="0")
manager = subprocess.Popen(["openbox", "--sm-disable", "--config-file", str(project / "integration/native/config/openbox/rc.xml")],
                           stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, env=manager_env)


def manager_ready():
    """Openbox d'Ubuntu (#3190) : « wmctrl -m » répond dès qu'il prend l'écran,
    avant la fin de son démarrage. Une fenêtre affichée à ce moment-là attend
    le prochain événement X reçu par openbox, indéfiniment sur un Xvfb au
    repos : les Paramètres manquaient une fois sur huit environ. L'openbox de
    Winlux est corrigé (integration/native/openbox/0002-fenetre-au-demarrage.patch,
    gardé par test-openbox-demarrage.py) ; celui d'Ubuntu, pris ici dans le
    PATH, ne l'est pas. Une sonde attend donc que sa boucle d'événements
    tourne, ce qui n'est pas le sujet de ce test, avant d'ouvrir les
    Paramètres."""
    probe = Gtk.Window(title="Sonde du gestionnaire de fenêtres")
    probe.set_default_size(40, 40)
    probe.show_all()
    deadline = time.monotonic() + 30
    try:
        for attempt in range(100000):
            while Gtk.events_pending():
                Gtk.main_iteration()
            if "Sonde du gestionnaire de fenêtres" in subprocess.run(["wmctrl", "-l"], capture_output=True, text=True).stdout:
                return
            if time.monotonic() > deadline:
                raise RuntimeError("openbox n'a pris en charge aucune fenêtre en 30 s")
            # Un événement X pour openbox : sa boucle traite alors sa file.
            subprocess.run(["xprop", "-root", "-f", "_WINLUX_SONDE_3190", "32c", "-set", "_WINLUX_SONDE_3190", str(attempt)],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(0.05)
    finally:
        probe.destroy()
        while Gtk.events_pending():
            Gtk.main_iteration()


manager_ready()
window = module["Settings"]()
failures = []
# The title follows the interface language since #145.
SETTINGS_TITLES = {"Paramètres", "Settings", "Configuración", "Einstellungen", "Paramètres Winlux", "Parametres Winlux", "Winlux Settings",
                   "Configuración de Winlux", "Winlux-Einstellungen"}

def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, timeout=10)

def rows():
    try: output = run("wmctrl", "-lG")
    except subprocess.CalledProcessError: return {}
    result = {}
    for line in output.splitlines():
        fields = line.split(None, 7)
        if len(fields) == 8: result[fields[0]] = (tuple(map(int, fields[2:6])), fields[7])
    return result

def check():
    try:
        # Mapping time is not what this test measures: the Settings window
        # builds all of its pages first, which can take several seconds on a
        # loaded runner. Only the geometry round trips below are timed.
        found = []
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            found = [key for key, (_, title) in rows().items() if title in SETTINGS_TITLES]
            if found: break
            time.sleep(0.05)
        assert found, "Settings window did not map"
        identifier = found[0]
        run("xdotool", "windowactivate", "--sync", identifier)
        # Comme Windows 11 : ← moitié gauche, → depuis la moitié gauche rend
        # la taille d'avant, → encore moitié droite.
        before = rows()[identifier][0]
        for key, x, half in [("super+Left", 0, True), ("super+Right", before[0], False), ("super+Right", 960, True)]:
            run("xdotool", "key", "--clearmodifiers", key)
            def reached(geometry):
                if half: return abs(geometry[0] - x) < 32 and abs(geometry[2] - 960) < 32
                return abs(geometry[0] - x) < 32 and abs(geometry[2] - before[2]) < 32
            for _ in range(100):
                geometry = rows()[identifier][0]
                if reached(geometry): break
                time.sleep(0.02)
            assert reached(geometry), (key, geometry, before)
        timings = {"move_ms": [], "resize_ms": []}
        for index in range(12):
            for name, args in [("move_ms", ["windowmove", "--sync", identifier, str(100 + index * 8), str(80 + index * 4)]),
                               ("resize_ms", ["windowsize", "--sync", identifier, str(800 + index * 8), str(500 + index * 4)])]:
                started = time.monotonic()
                run("xdotool", *args)
                timings[name].append((time.monotonic() - started) * 1000)
        # With 12 samples the nearest-rank 95th percentile is the maximum.
        report = {key: dict(median=round(statistics.median(values), 2), p95=round(max(values), 2),
                            maximum=round(max(values), 2), samples=len(values)) for key, values in timings.items()}
        print("OK: left/right snapping\n" + json.dumps(report, indent=2), flush=True)
    except Exception as error: failures.append(error)
    finally: GLib.idle_add(window.destroy)

threading.Thread(target=check, daemon=True).start()
try: Gtk.main()
finally:
    manager.terminate()
    _, diagnostics = manager.communicate(timeout=5)
    if failures and diagnostics: print(diagnostics)
if failures: raise failures[0]
