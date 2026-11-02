#!/usr/bin/python3
"""Volet de widgets Win+W (winlux-widgets) : réglages, météo, barre, volet.

Sans affichage : réglages (ordre, retrait, agenda), état du PC lu dans une
fausse racine, relevé wttr.in converti au format de weather.rs (shell PE),
bouton de la barre avec un faux xfconf-query, traductions des quatre langues,
raccourci Win+W livré. Avec un affichage X (xvfb-run) : le volet s'ouvre au
bord gauche, montre les widgets dans l'ordre choisi, masque ceux qui n'ont
rien à montrer, se referme à Échap et par un second Win+W, le minuteur
s'écoule volet fermé.
"""
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys
import tempfile
import time

directory = Path(__file__).resolve().parent
script = directory / "winlux-widgets.py"
temp = Path(tempfile.mkdtemp(prefix="winlux-widgets-test."))
root = temp / "root"
(root / "proc").mkdir(parents=True)
(root / "proc/stat").write_text("cpu  100 0 100 800 0 0 0 0 0 0\n")
(root / "proc/meminfo").write_text("MemTotal:       8000000 kB\nMemFree:  1000000 kB\nMemAvailable:   6000000 kB\n")
battery = root / "sys/class/power_supply/BAT0"
battery.mkdir(parents=True)
(battery / "capacity").write_text("76\n")
(battery / "status").write_text("Charging\n")
run_dir = temp / "run"
run_dir.mkdir()
home = temp / "home"
(home / "Images").mkdir(parents=True)
os.environ.update(WINLUX_WIDGETS_ROOT=str(root), WINLUX_RUN_DIR=str(run_dir), HOME=str(home),
                  XDG_CONFIG_HOME=str(temp / "config"), XDG_CACHE_HOME=str(temp / "cache"),
                  WINLUX_TODAY="2026-11-02", WINLUX_LANG="fr", WINLUX_WIDGETS_NO_ANIMATION="1",
                  NO_AT_BRIDGE="1", GTK_A11Y="none")
os.environ.pop("WINLUX_WEATHER_OFFLINE", None)

# Partie sans interface : le module s'arrête avant GTK s'il est lancé en ligne
# de commande ; ici il est chargé en entier seulement si GTK est présent.
source = script.read_text(encoding="utf-8")
cli_source = source.split("# ── Interface ─")[0]
cli = {"__file__": str(script), "__name__": "widgets_test"}
exec(compile(cli_source, str(script), "exec"), cli)

# ── Réglages ──
config = cli["load_config"]()
assert config["order"] == list(cli["WIDGETS"]) and config["hidden"] == [] and config["button"] is False, config
path = cli["config_path"]()
path.parent.mkdir(parents=True)
path.write_text(json.dumps({"order": ["timer", "inconnu", "timer", "pc"], "hidden": ["notes", "xyz"], "units": "k",
                            "agenda": [{"date": "2026-11-01", "time": "", "text": "Hier"},
                                       {"date": "2026-11-02", "time": "18:00", "text": "Sport"},
                                       {"date": "2026-11-02", "time": "08:15", "text": "Bus"}]}), encoding="utf-8")
config = cli["load_config"]()
assert config["order"][:2] == ["timer", "pc"] and sorted(config["order"]) == sorted(cli["WIDGETS"]), config["order"]
assert config["hidden"] == ["notes"] and config["units"] == "c", config
assert [entry["text"] for entry in cli["events_of"](config)] == ["Bus", "Sport"], "agenda : tri ou rendez-vous passés"
cli["move"](config, "pc", -1)
assert config["order"][:2] == ["pc", "timer"]
cli["move"](config, "pc", -1)
assert config["order"][0] == "pc", "déplacement au-delà du début"
parse = cli["parse_event"]
assert parse("14:30 Dentiste")["time"] == "14:30"
assert parse("9h Réunion") == {"date": "2026-11-02", "time": "09:00", "text": "Réunion"}
assert parse("Appeler Paul")["time"] == "" and parse("25:00 Rien")["time"] == "" and parse("   ") is None

# ── État du PC ──
assert cli["memory"]() == (2000000 * 1024, 8000000 * 1024)
assert cli["battery"]() == (76, True)
first = cli["cpu_times"]()
(root / "proc/stat").write_text("cpu  150 0 150 900 0 0 0 0 0 0\n")
assert cli["cpu_percent"](first, cli["cpu_times"]()) == 50
assert cli["size_text"](2000000 * 1024) == "1,9 Go"

# ── Météo : wttr.in (format j1) -> fichier lu par weather.rs ──
report = {"current_condition": [{"temp_C": "14", "temp_F": "57", "FeelsLikeC": "12", "FeelsLikeF": "54",
                                 "humidity": "72", "windspeedKmph": "18", "weatherCode": "116"}],
          "nearest_area": [{"areaName": [{"value": "Besançon"}]}],
          "weather": [{"date": "2026-11-02", "mintempC": "8", "maxtempC": "15", "mintempF": "46", "maxtempF": "59",
                       "hourly": [{"weatherCode": "113"}, {"weatherCode": "296"}, {"weatherCode": "116"}]},
                      {"date": "2026-11-03", "mintempC": "-2", "maxtempC": "3", "mintempF": "28", "maxtempF": "37",
                       "hourly": [{"weatherCode": "338"}]}]}
fixture = temp / "wttr.json"
fixture.write_text(json.dumps(report), encoding="utf-8")
os.environ["WINLUX_WEATHER_URL"] = fixture.as_uri()
assert cli["fetch_weather"](), "relevé météo non écrit"
raw = (run_dir / "weather.json").read_bytes()
assert "city=Besançon\n".encode("cp1252") in raw, raw  # shell PE : octets Windows-1252
text = raw.decode("cp1252")
for line in ("temp=14", "feels_like=12", "humidity=72", "wind=18", "condition=partly",
             "forecast0_day=lun", "forecast0_min=8", "forecast0_max=15", "forecast0_cond=rain",
             "forecast1_day=mar", "forecast1_min=-2", "forecast1_cond=snow"):
    assert line + "\n" in text, (line, text)
# weather.rs cherche ces mots dans « condition » ; « partly » ne doit pas
# contenir « cloud » (testé avant « partly » côté PE).
rust = (directory.parent / "testexe/src/shell/weather.rs").read_text(encoding="utf-8")
for name, word in {"sunny": "sun", "partly": "partly", "cloudy": "cloud", "rain": "rain", "snow": "snow",
                   "storm": "storm", "fog": "fog"}.items():
    assert f'b"{word}"' in rust and word in name, (name, word)
assert "cloud" not in "partly"
weather = cli["read_weather"]()
assert weather["temp"] == 14 and weather["city"] == "Besançon" and len(weather["forecast"]) == 2, weather
assert not cli["weather_stale"](weather)
assert cli["condition_of"]("389") == "storm" and cli["condition_of"]("248") == "fog" and cli["condition_of"]("x") == "cloudy"
fahrenheit = cli["weather_lines"](report, "f")
assert "temp=57\n" in fahrenheit and "units=f\n" in fahrenheit and "forecast0_max=59\n" in fahrenheit
# /run/winlux fermé : le cache du compte prend le relais.
os.environ["WINLUX_WEATHER_OFFLINE"] = "1"
assert not cli["fetch_weather"](), "WINLUX_WEATHER_OFFLINE doit couper le réseau"
os.environ.pop("WINLUX_WEATHER_OFFLINE")


def command(*args, env=None):
    environment = dict(os.environ, **(env or {}))
    return subprocess.run([sys.executable, "-B", str(script), *args], capture_output=True, text=True,
                          timeout=30, env=environment)


# ── Barre des tâches : sortie genmon et faux xfconf-query ──
output = command("--tray").stdout
assert "<img>" in output and ">14°</span></txt>" in output and "<click>" in output and "Besançon" in output, output
assert 'foreground="#f3f0fa"' in output, "texte de la barre : couleur claire fixée (barre sombre)"
bin_dir = temp / "bin"
bin_dir.mkdir()
state = temp / "xfconf.json"
state.write_text(json.dumps({"/panels/panel-1/plugin-ids": ["3", "1", "2", "8", "10", "4", "5", "6", "9", "7"],
                             "/plugins/plugin-3/expand": ["true"]}))
(bin_dir / "xfconf-query").write_text(f"""#!{sys.executable}
import json, sys
state = json.load(open({str(state)!r}))
args = sys.argv[1:]
prop = args[args.index("-p") + 1]
values = [args[i + 1] for i, a in enumerate(args) if a == "-s"]
if "-r" in args:
    for key in list(state):
        if key == prop or key.startswith(prop + "/"): del state[key]
elif values:
    if prop not in state and "-n" not in args and "-a" not in args: sys.exit(1)
    state[prop] = values
elif prop in state:
    print("\\n".join(state[prop]) if len(state[prop]) == 1 else "Value is an array with %d items:\\n\\n" % len(state[prop]) + "\\n".join(state[prop]))
else:
    sys.exit(1)
json.dump(state, open({str(state)!r}, "w"))
""")
(bin_dir / "xfconf-query").chmod(0o755)
fake = {"PATH": f"{bin_dir}:{os.environ['PATH']}"}
assert command("set", "button", "on", env=fake).returncode == 0
panel = json.loads(state.read_text())
assert panel["/panels/panel-1/plugin-ids"][0] == "11", panel  # barre centrée : à gauche
assert panel["/plugins/plugin-11"] == ["genmon"] and panel["/plugins/plugin-11/command"] == ["/usr/bin/winlux-widgets --tray"]
assert json.loads(command("status").stdout)["button"] is True
assert command("set", "button", "off", env=fake).returncode == 0
panel = json.loads(state.read_text())
assert "11" not in panel["/panels/panel-1/plugin-ids"] and "/plugins/plugin-11" not in panel, panel
ids = cli["ids_with_button"]([3, 1, 2, 8, 10, 4, 5, 6, 9, 7], False)
assert ids == [3, 1, 2, 8, 11, 10, 4, 5, 6, 9, 7], ids  # barre à gauche : près des icônes
assert cli["ids_with_button"]([11, 3, 1], True) == [11, 3, 1]
# winlux-control align-left/center garde le bouton (plugin-11 genmon).
control = (directory / "winlux-control.sh").read_text(encoding="utf-8")
assert "plugin-11" in control and '${widgets:+11 }' in control, "winlux-control retirerait le bouton à chaque session"

# ── Ligne de commande : lieu, unité, retour aux réglages par défaut ──
assert command("set", "location", "Lyon", env={"WINLUX_WEATHER_OFFLINE": "1"}).returncode == 0
assert command("set", "units", "f", env={"WINLUX_WEATHER_OFFLINE": "1"}).returncode == 0
status = json.loads(command("status").stdout)
assert status["location"] == "Lyon" and status["units"] == "f", status
assert not (run_dir / "weather.json").exists(), "relevé d'un autre lieu conservé"
assert command("set", "units", "kelvin").returncode == 2
assert command("reset").returncode == 0
assert json.loads(command("status").stdout)["hidden"] == []

# ── Traductions : chaque texte affiché existe en anglais, espagnol, allemand ──
translations = cli["_TR"]
used = set(re.findall(r"_t\(\s*'([^']+)'\s*\)", source)) | set(re.findall(r'_t\(\s*"([^"]+)"\s*\)', source))
used |= set(cli["TITLES"].values()) | set(cli["CONDITIONS"].values())
missing = sorted(text for text in used if text not in translations)
assert not missing, f"textes sans traduction : {missing}"
for key, row in translations.items():
    assert len(row) == 3 and all(value.strip() for value in row), key
pages = (directory / "winlux-settings-pages.py").read_text(encoding="utf-8")
block = pages[pages.index("def widget_settings"):pages.index("# ── Lecteurs réseau ──")]
pages_tr = runpy.run_path(str(directory / "winlux-settings-pages.py"), run_name="widgets_test")["_TR"] \
    if os.environ.get("DISPLAY") else None
for text in re.findall(r'_t\("([^"]+)"\)', block):
    assert f"'{text}'" in pages or f'"{text}"' in pages[:pages.index("def _t(")], f"Paramètres : {text} non traduit"
    if pages_tr is not None:
        assert text in pages_tr, text

# ── Raccourci Windows + W et intégration ──
rc = (directory.parent / "integration/native/config/openbox/rc.xml").read_text(encoding="utf-8")
assert '<keybind key="W-w"><action name="Execute"><command>winlux-widgets</command>' in rc
assert '"W-w"' in (directory / "winlux-native-config.py").read_text(encoding="utf-8"), "profils existants sans Win+W"
assert "winlux-widgets" in (directory / "stage-desktop-components.sh").read_text(encoding="utf-8")
assert (directory.parent / "integration/native/applications/winlux-widgets.desktop").is_file()
print("OK: widgets, réglages, agenda, état du PC, météo partagée avec le shell PE, bouton de la barre, traductions, Win+W")

if not os.environ.get("DISPLAY"):
    print("Affichage absent : volet non ouvert (lancer sous xvfb-run)")
    raise SystemExit(0)

# ── Volet réel ──
assert cli["fetch_weather"]()  # relevé effacé plus haut par le changement de lieu
os.environ["WINLUX_WEATHER_OFFLINE"] = "1"
module = runpy.run_path(str(script), run_name="widgets_test")
Gtk, Gdk, GLib = module["Gtk"], module["Gdk"], module["GLib"]
path.write_text(json.dumps({"order": ["timer", "weather", "agenda", "pc", "notes", "photos"], "hidden": ["notes"],
                            "agenda": [{"date": "2026-11-02", "time": "14:30", "text": "Dentiste"}]}), encoding="utf-8")
app = module["Widgets"]()
failures, done = [], []


def settle(seconds=0.3):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.02)


def scenario():
    try:
        settle()
        window = app.window
        assert window.get_visible(), "le volet ne s'est pas ouvert"
        x, y = window.get_position()
        width, height = window.get_size()
        screen = window.get_display().get_monitor(0).get_geometry()
        assert x <= 40 and width <= 460 and height >= screen.height - 80, (x, y, width, height, screen.height)
        shown = [card.winlux_name for card in app.list.get_children()]
        assert shown == ["timer", "weather", "agenda", "pc"], shown  # pense-bête retiré, pas de photo
        assert "battery" in app.pc_rows, "batterie présente mais non affichée"
        # Une image dans Images : le widget Photos apparaît.
        import shutil
        shutil.copy(directory.parent / "assets/brand/" / next(p.name for p in (directory.parent / "assets/brand").iterdir()
                                                              if p.suffix == ".png"), home / "Images/photo.png")
        app.restore("notes")
        settle()
        shown = [card.winlux_name for card in app.list.get_children()]
        assert shown[0] == "notes" and "photos" in shown, shown
        app.remove("weather")
        assert "weather" in json.loads(path.read_text())["hidden"]
        # Minuteur d'une seconde : s'écoule volet fermé, application gardée en vie.
        app.set_timer(1)
        app.toggle_timer()
        app.hide()
        assert not window.get_visible()
        settle(2.4)
        assert app.timer_end is None and app.timer_left == 0 and not app.held, (app.timer_end, app.timer_left)
        # Second Win+W : rouvre ; Échap : referme.
        app.activate()
        settle()
        assert window.get_visible() and app.timer_label.get_text() == "00:00"
        event = Gdk.Event.new(Gdk.EventType.KEY_PRESS)
        event.key.keyval = Gdk.KEY_Escape
        event.key.window = window.get_window()
        window.emit("key-press-event", event)
        assert not window.get_visible(), "Échap ne ferme pas le volet"
        done.append(True)
    except Exception as error:  # noqa: BLE001
        failures.append(error)
    app.quit()
    return False


GLib.timeout_add(500, scenario)
app.run([])
if failures:
    raise failures[0]
assert done, "scénario du volet non terminé"
print("OK: volet Win+W au bord gauche, ordre et retrait des widgets, photos, minuteur volet fermé, Échap")
