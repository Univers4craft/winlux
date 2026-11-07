#!/usr/bin/python3
"""Touches de volume du clavier (#620) : plus, moins, sourdine, micro.

    xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-volume-keys.py

Mixeurs factices (wpctl, puis pactl) dans le PATH. Sous un affichage avec
openbox et xdotool, les vraies touches XF86Audio* passent par le rc.xml livré.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
problems = []


def check(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition: problems.append(label)


tmp = Path(tempfile.mkdtemp(prefix="winlux-volume-"))
bin_dir = tmp / "bin"
bin_dir.mkdir()
state_file = tmp / "state.json"
FAKE = r'''#!PYTHON
import json, sys
path = "STATE"
state = json.load(open(path))
args = sys.argv[1:]
name = sys.argv[0].rsplit("/", 1)[-1]
if name == "wpctl":
    if args[:1] == ["get-volume"]:
        target = "mic" if "SOURCE" in args[1] else "out"
        print(f"Volume: {state[target + '_volume'] / 100:.2f}" + (" [MUTED]" if state[target + "_muted"] else ""))
    elif args[:1] == ["set-volume"]:
        value = args[-1]
        state["out_volume"] = min(100, int(value.rstrip("%")))
    elif args[:1] == ["set-mute"]:
        target = "mic" if "SOURCE" in args[1] else "out"
        state[target + "_muted"] = (not state[target + "_muted"]) if args[2] == "toggle" else args[2] == "1"
    else: sys.exit(1)
elif name == "pactl":
    if args == ["get-default-sink"]: print("alsa_output.pci")
    elif args == ["-f", "json", "list", "sinks"]:
        print(json.dumps([{"name": "alsa_output.pci", "mute": state["out_muted"],
                           "volume": {"front-left": {"value_percent": f"{state['out_volume']}%"}}}]))
    elif args[0] == "set-sink-volume": state["out_volume"] = int(args[2].rstrip("%"))
    elif args[0] == "set-sink-mute": state["out_muted"] = args[2] == "1"
    elif args[0] == "set-source-mute": state["mic_muted"] = not state["mic_muted"]
    elif args[0] == "get-source-mute": print("Mute: " + ("yes" if state["mic_muted"] else "no"))
    else: sys.exit(1)
json.dump(state, open(path, "w"))
'''.replace("PYTHON", sys.executable).replace("STATE", str(state_file))


def use(*names):
    for child in bin_dir.iterdir(): child.unlink()
    for name in names:
        (bin_dir / name).write_text(FAKE)
        (bin_dir / name).chmod(0o755)
    (bin_dir / "winlux-volume").write_text(f"#!/bin/sh\nexec {sys.executable} -B {HERE / 'winlux-volume.py'} \"$@\"\n")
    (bin_dir / "winlux-volume").chmod(0o755)


def reset(volume=40, muted=False):
    state_file.write_text(json.dumps({"out_volume": volume, "out_muted": muted, "mic_volume": 80, "mic_muted": False}))


def current():
    return json.loads(state_file.read_text())


env = dict(os.environ, PATH=str(bin_dir), WINLUX_VOLUME_NO_OSD="1", WINLUX_VOLUME_RUN=str(tmp / "run"), WINLUX_LANG="fr")


def press(action):
    return subprocess.run([sys.executable, "-B", str(HERE / "winlux-volume.py"), "key", action], env=env,
                          capture_output=True, text=True, timeout=20)


for backend in ("wpctl", "pactl"):
    use(backend)
    reset(41)
    press("up")
    check(current()["out_volume"] == 42, f"{backend} : Volume + par pas de 2 % ({current()['out_volume']})")
    press("down"); press("down")
    check(current()["out_volume"] == 38, f"{backend} : Volume − ({current()['out_volume']})")
    press("mute")
    check(current()["out_muted"], f"{backend} : Sourdine coupe le son")
    press("down")
    check(current()["out_muted"] and current()["out_volume"] == 36, f"{backend} : Volume − ne rétablit pas le son")
    press("up")
    check(not current()["out_muted"], f"{backend} : Volume + rétablit le son, comme Windows")
    press("mute"); press("mute")
    check(not current()["out_muted"], f"{backend} : Sourdine deux fois")
    reset(99)
    press("up"); press("up")
    check(current()["out_volume"] == 100, f"{backend} : plafond 100 %")
    reset(1)
    press("down")
    check(current()["out_volume"] == 0, f"{backend} : plancher 0 %")
    result = press("mic-mute")
    check(current()["mic_muted"] and json.loads(result.stdout)["mic_muted"], f"{backend} : sourdine du micro")
use()
result = press("up")
check(result.returncode == 0 and json.loads(result.stdout)["ok"] is False, "sans mixeur : rien, sans erreur")

# Indicateur : rendu GTK réel, puis remplacé par un appui plus récent.
module = {}
exec(compile((HERE / "winlux-volume.py").read_text(encoding="utf-8"), "winlux-volume", "exec"), module)
check(module["icon_name"](0, False) == "audio-volume-muted-symbolic" and module["icon_name"](80, False) == "audio-volume-high-symbolic"
      and module["icon_name"](80, True) == "audio-volume-muted-symbolic", "icône selon le niveau")

# ── Liaisons livrées et migration des comptes existants ─────────────────────
namespace = "{http://openbox.org/3.4/rc}"
rc = ET.parse(ROOT / "integration/native/config/openbox/rc.xml").getroot()
bindings = {binding.get("key"): binding.findtext(f"{namespace}action/{namespace}command") for binding in rc.find(f"{namespace}keyboard")}
expected = {"XF86AudioRaiseVolume": "winlux-volume key up", "XF86AudioLowerVolume": "winlux-volume key down",
            "XF86AudioMute": "winlux-volume key mute", "XF86AudioMicMute": "winlux-volume key mic-mute"}
check(all(bindings.get(key) == command for key, command in expected.items()), "rc.xml : touches de volume liées")
config = (HERE / "winlux-native-config.py").read_text(encoding="utf-8")
added = config[config.index("ADDED_KEYS = ("):config.index(")", config.index("ADDED_KEYS = ("))]
check(all(f'"{key}"' in added for key in expected), "ADDED_KEYS : touches ajoutées aux rc.xml existants")
check(" winlux-volume " in (HERE / "stage-desktop-components.sh").read_text(encoding="utf-8"), "winlux-volume installé dans l'image")

# ── Vraies touches sous openbox ─────────────────────────────────────────────
if os.environ.get("DISPLAY") and shutil.which("openbox") and shutil.which("xdotool"):
    use("wpctl")
    reset(50)
    (tmp / "home").mkdir()
    shown = dict(env, PATH=f"{bin_dir}:{os.environ.get('PATH', '/usr/bin:/bin')}", HOME=str(tmp / "home"))
    shown.pop("WINLUX_VOLUME_NO_OSD")
    wm = subprocess.Popen(["openbox", "--config-file", str(ROOT / "integration/native/config/openbox/rc.xml")], env=shown,
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(1.5)
        subprocess.run(["xdotool", "key", "XF86AudioRaiseVolume"], check=True)
        deadline = time.time() + 10
        while time.time() < deadline and current()["out_volume"] == 50: time.sleep(0.2)
        check(current()["out_volume"] == 52, f"openbox : touche Volume + ({current()['out_volume']})")
        osd = ""
        while time.time() < deadline and not osd:
            osd = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Volume$"], capture_output=True, text=True).stdout.strip()
            time.sleep(0.1)
        check(bool(osd), "indicateur de volume affiché")
        subprocess.run(["xdotool", "key", "XF86AudioMute"], check=True)
        deadline = time.time() + 10
        while time.time() < deadline and not current()["out_muted"]: time.sleep(0.2)
        check(current()["out_muted"], "openbox : touche Sourdine")
    finally:
        wm.terminate()
        wm.wait(timeout=5)
else:
    print("openbox, xdotool ou affichage absent : vraies touches non vérifiées")

shutil.rmtree(tmp, ignore_errors=True)
if problems:
    print(f"{len(problems)} échec(s)")
    sys.exit(1)
print("OK: touches de volume du clavier (#620)")
