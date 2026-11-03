#!/usr/bin/python3
"""Éclairage nocturne (winlux-ambiance) : soleil, planning, couleurs, commandes."""
import datetime as dt
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "winlux-ambiance.py"
module = runpy.run_path(str(SCRIPT), run_name="ambiance_test")

# Heures et valeurs refusées.
assert module["parse_clock"]("07:30") == 450 and module["parse_clock"]("24:00") is None
assert module["validate"]("FROM", "7:05") == "07:05" and module["validate"]("FROM", "soir") is None
assert module["validate"]("STRENGTH", "250") == "100" and module["validate"]("SCHEDULE", "jamais") is None

# Coordonnées du fuseau (zone1970.tab) et lever/coucher réels, à quelques minutes près.
with tempfile.TemporaryDirectory() as folder:
    (Path(folder) / "zone1970.tab").write_text(
        "# commentaire\nFR,MC\t+4852+00220\tEurope/Paris\nUS\t+404251-0740023\tAmerica/New_York\n")
    os.environ["WINLUX_ZONEINFO"] = folder
    paris = module["zone_coordinates"]("Europe/Paris")
    assert abs(paris[0] - 48.867) < 0.01 and abs(paris[1] - 2.333) < 0.01, paris
    york = module["zone_coordinates"]("America/New_York")
    assert abs(york[0] - 40.714) < 0.01 and abs(york[1] + 74.006) < 0.01, york
    assert module["zone_coordinates"]("Mars/Olympus") is None
del os.environ["WINLUX_ZONEINFO"]
sunrise, sunset = module["sun_times"](dt.date(2026, 6, 21), 48.8667, 2.3333, 2)
assert abs(sunrise - (5 * 60 + 47)) <= 4 and abs(sunset - (21 * 60 + 58)) <= 4, (sunrise, sunset)
sunrise, sunset = module["sun_times"](dt.date(2026, 12, 21), 48.8667, 2.3333, 1)
assert abs(sunrise - (8 * 60 + 41)) <= 4 and abs(sunset - (16 * 60 + 56)) <= 4, (sunrise, sunset)
assert module["sun_times"](dt.date(2026, 6, 21), 78.2, 15.6, 2) is None  # soleil de minuit (Svalbard)

# Planning à cheval sur minuit : la même nuit garde le même repère.
values = dict(module["DEFAULTS"], SCHEDULE="manual", FROM="21:00", TO="07:00")
period = module["scheduled_period"]
assert period(values, dt.datetime(2026, 11, 2, 22, 0)) == (True, "2026-11-02N")
assert period(values, dt.datetime(2026, 11, 3, 6, 59)) == (True, "2026-11-02N")
assert period(values, dt.datetime(2026, 11, 3, 7, 0)) == (False, "2026-11-03D")
assert period(dict(values, FROM="13:00", TO="15:00"), dt.datetime(2026, 11, 3, 12, 0)) == (False, "2026-11-02D")
assert period(dict(values, SCHEDULE="off"), dt.datetime(2026, 11, 3, 22, 0)) == (False, "")
# Au début de la nuit le planning allume ; un « désactiver » à la main tient jusqu'au matin.
follow = module["follow_schedule"]
assert follow(values, dt.datetime(2026, 11, 2, 21, 0)) and values["NIGHTLIGHT"] == "on"
values["NIGHTLIGHT"] = "off"
assert not follow(values, dt.datetime(2026, 11, 3, 2, 0)) and values["NIGHTLIGHT"] == "off"
assert follow(values, dt.datetime(2026, 11, 3, 7, 0)) and values["NIGHTLIGHT"] == "off"
assert follow(values, dt.datetime(2026, 11, 3, 21, 30)) and values["NIGHTLIGHT"] == "on"

# Couleurs : neutre le jour, rouge plein et bleu réduit la nuit, plus chaud avec l'intensité.
rgb = module["kelvin_rgb"]
assert rgb(6500) == (1.0, 1.0, 1.0)
warm, warmer = rgb(module["target_kelvin"](dict(values, NIGHTLIGHT="on", STRENGTH="30"))), rgb(2700)
assert warm[0] == 1.0 and 1.0 > warm[1] > warm[2] > warmer[2] > 0.2, (warm, warmer)
assert module["target_kelvin"](dict(values, NIGHTLIGHT="off", STRENGTH="100")) == 6500

# Commandes, avec un écran simulé.
with tempfile.TemporaryDirectory() as folder:
    screen = Path(folder) / "gamma"
    env = dict(os.environ, XDG_CONFIG_HOME=folder, WINLUX_AMBIANCE_FAKE=str(screen),
               WINLUX_AMBIANCE_NOW="2026-11-02T12:00", TZ="Europe/Paris")

    def tool(*args, check=True, **extra):
        result = subprocess.run([sys.executable, "-I", str(SCRIPT), *args], env=dict(env, **extra),
                                capture_output=True, text=True, timeout=20)
        if check: assert result.returncode == 0, (args, result.stderr)
        return result

    def status(**extra):
        return dict(line.split("=", 1) for line in tool("status", **extra).stdout.splitlines())

    state = status()
    assert state["supported"] == "1" and state["nightlight"] == "off" and state["schedule"] == "off", state
    assert re.fullmatch(r"\d\d:\d\d", state["sunset"]) and re.fullmatch(r"\d\d:\d\d", state["sunrise"]), state
    assert status(WINLUX_AMBIANCE_FAKE_SUPPORTED="0")["supported"] == "0"
    tool("toggle")
    assert status()["nightlight"] == "on"
    red, green, blue = map(float, screen.read_text().split())
    assert red == 1.0 and blue < green < 1.0, (red, green, blue)
    tool("set", "STRENGTH", "100")
    assert float(screen.read_text().split()[2]) < blue
    assert tool("set", "STRENGTH", "fort", check=False).returncode == 2
    assert tool("set", "STAMP", "x", check=False).returncode == 2
    tool("toggle")
    assert status()["nightlight"] == "off" and screen.read_text().split() == ["1.0000"] * 3
    # Planning choisi en pleine nuit : il s'applique aussitôt.
    tool("set", "SCHEDULE", "manual", WINLUX_AMBIANCE_NOW="2026-11-02T23:00")
    assert status()["nightlight"] == "on" and status()["schedule"] == "manual"
    tool("apply", WINLUX_AMBIANCE_NOW="2026-11-03T08:00")
    assert status()["nightlight"] == "off"
    tool("reset")
    assert screen.read_text().split() == ["1.0000"] * 3
    conf = (Path(folder) / "winlux" / "ambiance.conf").read_text()
    assert "SCHEDULE=manual" in conf and "STRENGTH=100" in conf, conf

# Textes de la page Écran et des réglages rapides : quatre langues.
for name, keys in (("winlux-settings-pages.py", ("Éclairage nocturne", "Activer maintenant", "Intensité",
                                                 "Planifier l’éclairage nocturne", "Jamais", "Définir les heures",
                                                 "Du coucher au lever du soleil ({sunset} – {sunrise})",
                                                 "Activer à", "Désactiver à",
                                                 "Heures calculées pour votre fuseau horaire, sans connexion à Internet.",
                                                 "Fuseau horaire sans coordonnées : 20:00 – 07:00 par défaut.",
                                                 "Des couleurs plus chaudes le soir reposent les yeux et aident à s’endormir.")),
                   ("winlux-system-tools.py", ("Éclairage nocturne : activer", "Éclairage nocturne : désactiver"))):
    text = (HERE / name).read_text(encoding="utf-8")
    table = text[text.index("_TR = {"):]
    for key in keys:
        line = next((row for row in table.splitlines() if row.strip().startswith(repr(key) + ":")), None)
        assert line and line.count("'") >= 8, (name, key)
# Branché dans la session et dans l'image.
assert "winlux-ambiance daemon" in (HERE / "winlux-user-session.sh").read_text()
assert "'winlux-ambiance.py:winlux-ambiance'" in (HERE / "stage-desktop-components.sh").read_text()
assert '8: "sections_display"' in (HERE / "winlux-settings-pages.py").read_text()
print("éclairage nocturne : OK")
