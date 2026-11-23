#!/usr/bin/python3
"""#3067 : le démon des alarmes tourne toute la session sans charger GTK,
et dort jusqu'à l'échéance au lieu de relever toutes les 2 s."""
import datetime as dt
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "winlux-alarms.py"
m = runpy.run_path(str(SCRIPT), run_name="winlux_alarms")

# Attente jusqu'à la prochaine échéance, 2 s au plus.
moment = dt.datetime(2026, 11, 22, 20, 0, 30)
stamp = moment.timestamp()
empty = {"alarms": [], "timers": [], "focus": {}}
print("sans échéance :", m["next_wait"](empty, moment, stamp))
assert m["next_wait"](empty, moment, stamp) == 2.0
timer = {"alarms": [], "timers": [{"end": stamp + 0.5}], "focus": {}}
print("minuteur dans 0,5 s :", m["next_wait"](timer, moment, stamp))
assert abs(m["next_wait"](timer, moment, stamp) - 0.5) < 1e-6
late = {"alarms": [], "timers": [{"end": stamp - 5}], "focus": {}}
assert m["next_wait"](late, moment, stamp) == 0.01
alarm = {"alarms": [m["new_alarm"]("20:01")], "timers": [], "focus": {}}
print("alarme à 20:01, il est 20:00:30 :", m["next_wait"](alarm, moment, stamp))
assert m["next_wait"](alarm, moment, stamp) == 2.0
moment2 = dt.datetime(2026, 11, 22, 20, 0, 59, 500000)
print("alarme à 20:01, il est 20:00:59,5 :", m["next_wait"](alarm, moment2, moment2.timestamp()))
assert abs(m["next_wait"](alarm, moment2, moment2.timestamp()) - 0.5) < 1e-3

# Le démon tourne et traite un minuteur alors que « import gi » échoue.
with tempfile.TemporaryDirectory() as folder:
    fake = Path(folder) / "faux-gi" / "gi"
    fake.mkdir(parents=True)
    (fake / "__init__.py").write_text("raise ImportError('GTK interdit dans le démon (#3067)')\n")
    env = dict(os.environ, HOME=folder, PYTHONPATH=str(fake.parent), WINLUX_LANG="fr", XDG_STATE_HOME=folder)
    env.pop("DISPLAY", None)
    tool = lambda *args: subprocess.run([sys.executable, "-B", str(SCRIPT), *args], env=env, check=True,
                                        capture_output=True, text=True).stdout
    daemon = subprocess.Popen([sys.executable, "-B", str(SCRIPT), "daemon"], env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    try:
        time.sleep(1.5)
        tool("add-timer", "3", "essai")
        state = Path(folder) / "winlux/clock.json"
        end = lambda: json.loads(state.read_text())["timers"][0]["end"]
        print("avant l'échéance, fin prévue :", end())
        assert end() is not None
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline and daemon.poll() is None and end() is not None:
            time.sleep(0.05)
        print("après l'échéance, fin :", end(), "| démon vivant :", daemon.poll() is None)
        assert daemon.poll() is None, daemon.communicate()[0]
        assert end() is None, "le démon n'a pas traité le minuteur"
    finally:
        daemon.kill()
        out = daemon.communicate()[0]
    # Seule la fenêtre de sonnerie, processus à part, a besoin de GTK : ici
    # elle échoue sur le faux module, la preuve que le démon l'a bien lancée.
    print("journal du démon :", out.strip()[-300:] or "(vide)")
    assert "in run_ring" in out and "in run_daemon" not in out, out
print("Alarmes, démon léger : OK")
