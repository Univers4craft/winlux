#!/usr/bin/python3
"""Icône du clavier tactile au repos (#3419).

Avant : le greffon genmon n° 12 lançait `winlux-osk tray`, un Python neuf,
toutes les 5 s (10 en 60 s sur le vrai PC de damien, ≈ 52 ms de CPU chacun).
Après : `winlux-tray serve`, déjà lancé pour la barre, calcule la même ligne
et le lecteur en sh `winlux-tray osk` la recopie sans démarrer Python.

Vérifié, sans affichage :
1. La ligne du service est celle de `winlux-osk tray`, avec et sans clavier
   physique, réglage auto, toujours et jamais, et suit un clavier branché.
2. Le lecteur sert la sortie `osk` du service sans lancer le programme.
3. `winlux-osk panel` règle le greffon sur ce lecteur quand il est installé,
   et garde l'ancienne commande sans lui.
"""
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
tmp = Path(tempfile.mkdtemp(prefix="winlux-osk-repos."))
os.environ.update(WINLUX_OSK_ROOT=str(tmp), HOME=str(tmp / "home"), WINLUX_LANG="fr",
                  XDG_CACHE_HOME=str(tmp / "cache"), XDG_CONFIG_HOME=str(tmp / "home/.config"))
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label, flush=True)
    if not condition:
        failures.append(label)


KEYBOARD = """I: Bus=0011 Vendor=0001 Product=0001 Version=ab41
N: Name="AT Translated Set 2 keyboard"
H: Handlers=sysrq kbd event0 leds
B: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe

"""
POWER = """I: Bus=0019 Vendor=0000 Product=0001 Version=0000
N: Name="Power Button"
H: Handlers=kbd event1
B: KEY=10000000000000 0

"""
(tmp / "proc/bus/input").mkdir(parents=True)
(tmp / "home/.config/winlux").mkdir(parents=True)
devices = tmp / "proc/bus/input/devices"
conf = tmp / "home/.config/winlux/osk.conf"
tray = runpy.run_path(str(HERE / "winlux-tray.py"), run_name="tray_osk_test")


def direct():
    return subprocess.run([sys.executable, "-B", str(HERE / "winlux-osk.py"), "tray"],
                          capture_output=True, text=True, timeout=30).stdout.strip()


# 1. Même ligne que l'ancien Python, et elle suit l'état.
ok("osk" in tray["SERVED"] and tray["SERVED"]["osk"] <= 5, "l'icône du clavier tactile est servie par winlux-tray serve, au moins toutes les 5 s (#3419)")
cases = [("auto", POWER, True), ("auto", POWER + KEYBOARD, False), ("always", POWER + KEYBOARD, True), ("never", POWER, False)]
for mode, text, shown in cases:
    conf.write_text(f"OSK_ICON={mode}\n")
    devices.write_text(text)
    line = tray["render"]("osk")
    ok(line == direct() and ("<img>" in line) == shown,
       f"réglage {mode}, {'clavier branché' if KEYBOARD in text else 'sans clavier'} : même ligne que winlux-osk tray (#3419)")

# 2. Le lecteur sert la ligne du service, sans lancer le programme.
out = tmp / "cache/winlux/tray-out"
out.mkdir(parents=True, exist_ok=True)
fake = tmp / "bin/winlux-tray"
fake.parent.mkdir()
shutil.copy("/bin/sleep", fake)
service = subprocess.Popen([str(fake), "60"])
try:
    (out / "serve.pid").write_text(f"{service.pid}\n")
    (out / "out-osk").write_text("<txt></txt><tool></tool>\n")
    marker = tmp / "programme-lance"
    program = tmp / "programme"
    program.write_text(f"#!/bin/sh\ntouch {marker}\n")
    program.chmod(0o755)
    read = subprocess.run(["sh", str(HERE / "winlux-tray-read.sh"), "osk"], capture_output=True, text=True, timeout=10,
                          env=dict(os.environ, WINLUX_TRAY_PROGRAM=str(program), WINLUX_TRAY_NO_SERVE="1"))
    ok(read.stdout == "<txt></txt><tool></tool>\n" and not marker.exists(),
       "winlux-tray osk recopie la sortie du service sans démarrer de programme (#3419)")
finally:
    service.kill()
    service.wait()

# 3. Commande du greffon réglée par winlux-osk panel.
osk = runpy.run_path(str(HERE / "winlux-osk.py"), run_name="osk_panel_test")
conf.write_text("OSK_ICON=auto\n")
for installed in (True, False):
    calls = []
    globals_ = osk["set_panel"].__globals__
    reader = tmp / ("reader" if installed else "absent")
    if installed:
        reader.write_text("")
    globals_["TRAY_READER"] = reader
    globals_["_xfconf"] = lambda *args, calls=calls: (calls.append(args), "4\n" if args[-1].endswith("plugin-ids") else "")[1]
    globals_["shutil"].which = lambda name: "/usr/bin/" + name
    os.environ["WINLUX_OSK_NO_RESTART"] = "1"
    osk["set_panel"](True)
    commands = [args[-1] for args in calls if args[:2] == ("-p", "/plugins/plugin-12/command") and "-s" in args]
    if installed:
        ok(commands and commands[0] == f"{reader} osk", "winlux-osk panel règle le greffon sur le lecteur winlux-tray osk (#3419)")
    else:
        ok(commands and commands[0].endswith(" tray") and "osk" in commands[0],
           "sans le lecteur installé, le greffon garde winlux-osk tray (#3419)")

shutil.rmtree(tmp, ignore_errors=True)
if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("clavier tactile au repos : OK")
