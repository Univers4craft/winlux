#!/usr/bin/python3
"""Application Horloge : alarmes, minuteurs, concentration (Ne pas déranger), commandes."""
import configparser
import datetime as dt
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "winlux-alarms.py"
m = runpy.run_path(str(SCRIPT), run_name="alarms_test")
D = dt.datetime

# Alarme répétée en semaine : sonne à l'heure, une seule fois par minute prévue, pas le week-end.
data = {"alarms": [m["new_alarm"]("07:30", [0, 1, 2, 3, 4], "Réveil")], "timers": [], "zones": [], "focus": {}}
monday = D(2026, 11, 2, 7, 30, 12)
assert [a["label"] for a in m["due_alarms"](data, monday)] == ["Réveil"]
assert m["due_alarms"](data, monday.replace(second=40)) == []
assert m["due_alarms"](data, D(2026, 11, 7, 7, 30)) == []           # samedi
assert data["alarms"][0]["on"]
assert m["next_ring"](data["alarms"][0], D(2026, 11, 6, 8, 0)) == D(2026, 11, 9, 7, 30)  # vendredi → lundi
# Alarme « une seule fois » : se coupe après avoir sonné.
once = {"alarms": [m["new_alarm"]("18:00")], "timers": [], "zones": [], "focus": {}}
assert m["next_ring"](once["alarms"][0], D(2026, 11, 2, 19, 0)) == D(2026, 11, 3, 18, 0)
assert len(m["due_alarms"](once, D(2026, 11, 2, 18, 0, 5))) == 1 and not once["alarms"][0]["on"]
assert m["next_ring"](once["alarms"][0], D(2026, 11, 2, 18, 1)) is None
# Rappel dans 5 minutes : resonne à l'échéance, même pour une alarme unique.
once["alarms"][0].update(on=True, snooze=D(2026, 11, 2, 18, 5).timestamp())
assert m["due_alarms"](once, D(2026, 11, 2, 18, 3)) == []
assert len(m["due_alarms"](once, D(2026, 11, 2, 18, 5, 1))) == 1 and once["alarms"][0]["snooze"] == 0
assert m["parse_clock"]("25:00") is None and m["parse_clock"]("7:05") == (7, 5)

# Minuteurs : temps restant, pause, fin unique.
timer = m["new_timer"](90, "Pâtes", start=1000)
assert m["timer_left"](timer, 1030) == 60 and m["timer_left"](timer, 2000) == 0
data = {"alarms": [], "timers": [timer, m["new_timer"](30)], "zones": [], "focus": {}}
assert [t["label"] for t in m["due_timers"](data, 1090)] == ["Pâtes"] and m["due_timers"](data, 1100) == []
assert timer["end"] is None and timer["remaining"] == 90
assert m["duration"](3725) == "01:02:05"

# Concentration : Ne pas déranger activé puis retiré, sauf s'il était déjà là.
with tempfile.TemporaryDirectory() as folder:
    os.environ["XDG_CONFIG_HOME"] = folder
    dnd = Path(folder) / "winlux/do-not-disturb"
    data = {"alarms": [], "timers": [], "zones": [], "focus": {}}
    m["focus_start"](data, 25, 1000)
    assert dnd.exists() and data["focus"]["end"] == 1000 + 25 * 60
    assert not m["focus_due"](data, 2000) and m["focus_due"](data, 2500)
    assert not dnd.exists() and data["focus"] == {}
    dnd.touch()
    m["focus_start"](data, 3, 0)            # au moins 5 minutes
    assert data["focus"]["minutes"] == 5
    m["focus_stop"](data)
    assert dnd.exists(), "le Ne pas déranger choisi par l'utilisateur reste"
    del os.environ["XDG_CONFIG_HOME"]

# Horloges du monde.
assert m["offset_text"](3600 * 9, 3600) == m["_t"]("{h} h d’avance").format(h="8")
assert m["offset_text"](-3600 * 5, 3600) == m["_t"]("{h} h de retard").format(h="6")
assert m["offset_text"](3600, 3600) == m["_t"]("Même heure")
tokyo = m["zone_time"]("Asia/Tokyo", D(2026, 11, 2, 12, 0, tzinfo=dt.timezone.utc))
assert tokyo is not None and tokyo.hour == 21

# Commandes.
with tempfile.TemporaryDirectory() as folder:
    env = dict(os.environ, XDG_STATE_HOME=f"{folder}/s", XDG_CONFIG_HOME=f"{folder}/c")

    def tool(*args):
        result = subprocess.run([sys.executable, "-I", str(SCRIPT), *args], env=env, capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, (args, result.stderr)
        return result.stdout

    tool("add-alarm", "06:45", "56", "Week-end")
    tool("add-alarm", "12:00", "-")
    tool("add-timer", "120", "Thé")
    tool("focus", "30")
    listing = tool("list").splitlines()
    assert listing[0] == "alarm 06:45 56 on Week-end" and listing[1].startswith("alarm 12:00 - on"), listing
    assert listing[2].startswith("timer 120 ") and listing[-1] == "focus 30", listing
    assert (Path(folder) / "c/winlux/do-not-disturb").exists()
    tool("stop-focus")
    assert not (Path(folder) / "c/winlux/do-not-disturb").exists()

# Textes en quatre langues, lanceur traduit, son, session et image.
for key, row in m["_TR"].items():
    assert len(row) == 3 and all(row), key
entry = configparser.RawConfigParser(strict=False)
entry.optionxform = str
entry.read(HERE.parent / "integration/native/applications/winlux-alarms.desktop", encoding="utf-8")
for lang in ("en", "es", "de"):
    assert entry["Desktop Entry"][f"Name[{lang}]"] and entry["Desktop Entry"][f"Comment[{lang}]"]
assert (HERE.parent / "assets/sounds/alarme.wav").stat().st_size > 10000
assert "winlux-alarms daemon" in (HERE / "winlux-user-session.sh").read_text()
assert "'winlux-alarms.py:winlux-alarms'" in (HERE / "stage-desktop-components.sh").read_text()
print("Horloge : OK")
