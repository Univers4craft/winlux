#!/usr/bin/python3
"""Run with xvfb-run; measures geometry latency, not visible frame pacing."""
import json
from pathlib import Path
import runpy
import statistics
import subprocess
import threading
import time

project = Path(__file__).resolve().parent.parent
module = runpy.run_path(str(project / "scripts/winlux-settings.py"), run_name="settings_test")
Gtk, GLib = module["Gtk"], module["GLib"]
if subprocess.run(["wmctrl", "-m"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
    raise RuntimeError("Use a dedicated Xvfb display without an existing window manager")
manager = subprocess.Popen(["openbox", "--sm-disable", "--config-file", str(project / "integration/native/config/openbox/rc.xml")],
                           stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
window = module["Settings"]()
failures = []
# The title follows the interface language since #145.
SETTINGS_TITLES = {"Paramètres Winlux", "Parametres Winlux", "Winlux Settings",
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
        for key, x in [("super+Left", 0), ("super+Right", 960)]:
            run("xdotool", "key", "--clearmodifiers", key)
            for _ in range(50):
                geometry = rows()[identifier][0]
                if abs(geometry[0] - x) < 32 and abs(geometry[2] - 960) < 32: break
                time.sleep(0.02)
            assert abs(geometry[0] - x) < 32 and abs(geometry[2] - 960) < 32, (key, geometry)
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
