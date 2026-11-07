#!/usr/bin/python3
"""Touche Windows seule (#605) : ouvre et ferme le menu Démarrer, sans gêner
Windows+Gauche/Droite, Windows+Z, Windows+Espace ni Windows+clic.

    xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-super-key.py

Logique pure toujours vérifiée ; sous un affichage, le vrai démon est lancé
(sous un vrai openbox avec le rc.xml livré s'il est installé, pour vérifier
que ses saisies Windows+… n'empêchent ni ne déclenchent rien) et les touches
sont tapées par XTest.
"""
import ctypes
import os
from pathlib import Path
import runpy
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


# ── Logique : appui-relâché seul ─────────────────────────────────────────────
module = runpy.run_path(str(HERE / "winlux-super-key.py"), run_name="test_super_key")
PRESS, RELEASE, BUTTON = module["XI_RAW_KEY_PRESS"], module["XI_RAW_KEY_RELEASE"], module["XI_RAW_BUTTON_PRESS"]
SUPER_L, SUPER_R, LEFT, Z, SPACE, A = 133, 134, 113, 52, 65, 38


def fires(*events):
    tracker = module["Tracker"](lambda code: code in (SUPER_L, SUPER_R))
    return [tracker.event(kind, code, repeat) for kind, code, *rest in events for repeat in [bool(rest and rest[0])]].count(True)


check(fires((PRESS, SUPER_L), (RELEASE, SUPER_L)) == 1, "Windows seule ouvre Démarrer")
check(fires((PRESS, SUPER_R), (RELEASE, SUPER_R)) == 1, "Windows droite seule aussi")
check(fires((PRESS, SUPER_L), (PRESS, SUPER_L, True), (PRESS, SUPER_L, True), (RELEASE, SUPER_L)) == 1, "répétition automatique ignorée")
for name, other in (("Gauche", LEFT), ("Z", Z), ("Espace", SPACE)):
    check(fires((PRESS, SUPER_L), (PRESS, other), (RELEASE, other), (RELEASE, SUPER_L)) == 0, f"Windows+{name} n'ouvre pas Démarrer")
check(fires((PRESS, SUPER_L), (PRESS, LEFT), (RELEASE, SUPER_L), (RELEASE, LEFT)) == 0, "Windows relâchée avant Gauche")
check(fires((PRESS, SUPER_L), (BUTTON, 1), (RELEASE, SUPER_L)) == 0, "Windows+clic n'ouvre pas Démarrer")
check(fires((PRESS, A), (PRESS, SUPER_L), (RELEASE, SUPER_L), (RELEASE, A)) == 0, "Windows pendant une autre touche")
check(fires((PRESS, SUPER_L), (PRESS, LEFT), (RELEASE, LEFT), (RELEASE, SUPER_L), (PRESS, SUPER_L), (RELEASE, SUPER_L)) == 1,
      "Windows seule juste après Windows+Gauche")

# ── Installation et démarrage avec la session ───────────────────────────────
stage = (HERE / "stage-desktop-components.sh").read_text(encoding="utf-8")
check("'winlux-super-key.py:winlux-super-key'" in stage, "winlux-super-key installé dans l'image")
for script in ("winlux-native-session.sh", "winlux-user-session.sh"):
    check("/usr/bin/winlux-super-key" in (HERE / script).read_text(encoding="utf-8"), f"{script} lance winlux-super-key")
namespace = "{http://openbox.org/3.4/rc}"
rc = ET.parse(ROOT / "integration/native/config/openbox/rc.xml").getroot()
keys = {binding.get("key") for binding in rc.find(f"{namespace}keyboard")}
check("Super_L" not in keys and "W-space" in keys and {"W-Left", "W-Right", "W-z"} <= keys,
      "rc.xml : pas de liaison Super_L (elle volerait Windows+Gauche), Windows+Espace gardé")

# ── Démon réel sous X ────────────────────────────────────────────────────────
if not os.environ.get("DISPLAY"):
    print("pas d'affichage : démon réel non vérifié")
else:
    tmp = Path(tempfile.mkdtemp(prefix="winlux-super-key-"))
    hits = tmp / "hits"
    env = dict(os.environ, XDG_RUNTIME_DIR=str(tmp), HOME=str(tmp),
               WINLUX_SUPER_KEY_COMMAND=f"{sys.executable} -c \"open('{hits}', 'a').write('x')\"")
    processes = []
    if shutil.which("openbox"):
        (tmp / "openbox").mkdir()
        rc_copy = tmp / "openbox/rc.xml"
        shutil.copy(ROOT / "integration/native/config/openbox/rc.xml", rc_copy)
        processes.append(subprocess.Popen(["openbox", "--config-file", str(rc_copy)], env=env,
                                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        time.sleep(1.5)
    else:
        print("openbox absent : démon vérifié sans gestionnaire de fenêtres")
    daemon = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-super-key.py")], env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    processes.append(daemon)
    try:
        ready = daemon.stdout.readline()
        check("prêt" in ready, f"démon prêt ({ready.strip()})")
        second = subprocess.run([sys.executable, "-B", str(HERE / "winlux-super-key.py")], env=env, timeout=10)
        check(second.returncode == 0, "une seule instance par affichage")
        x11, xtst = ctypes.CDLL("libX11.so.6"), ctypes.CDLL("libXtst.so.6")
        x11.XOpenDisplay.restype = ctypes.c_void_p
        x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        x11.XKeysymToKeycode.restype = ctypes.c_ubyte
        x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        display = x11.XOpenDisplay(None)
        code = {name: x11.XKeysymToKeycode(display, sym) for name, sym in
                (("super", 0xFFEB), ("super_r", 0xFFEC), ("left", 0xFF51), ("z", 0x7A), ("space", 0x20))}

        def press(*names, button=None):
            for name in names: xtst.XTestFakeKeyEvent(display, code[name], 1, 0)
            if button:
                xtst.XTestFakeButtonEvent(display, button, 1, 0)
                xtst.XTestFakeButtonEvent(display, button, 0, 0)
            for name in reversed(names): xtst.XTestFakeKeyEvent(display, code[name], 0, 0)
            x11.XSync(display, 0)
            time.sleep(0.4)

        def count():
            try: return len(hits.read_text())
            except OSError: return 0

        press("super")
        check(count() == 1, "touche Windows : Démarrer lancé")
        press("super")
        check(count() == 2, "second appui : relancé (le menu se referme lui-même)")
        press("super", "left")
        press("super", "z")
        press("super", "space")
        press("super", button=1)
        check(count() == 2, "Windows+Gauche, +Z, +Espace et +clic : rien")
        press("super_r")
        check(count() == 3, "touche Windows droite")
        time.sleep(0.5)
        check(daemon.poll() is None, "démon toujours en marche")
    finally:
        for process in processes:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill()
        shutil.rmtree(tmp, ignore_errors=True)

if problems:
    print(f"{len(problems)} échec(s)")
    sys.exit(1)
print("OK: touche Windows seule pour Démarrer (#605)")
