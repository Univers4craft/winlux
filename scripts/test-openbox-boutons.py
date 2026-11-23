#!/usr/bin/env python3
"""Boutons Réduire et Agrandir selon le style Win32, sous l'openbox de Winlux (#3064).

Un programme PE (testc/boite/boutons_titre.c) ouvre trois fenêtres. D'après
Learn (« Window Styles ») et la KB Q130760 de Microsoft, Windows montre :
  BoutonsTous     WS_OVERLAPPEDWINDOW             Réduire, Agrandir, Fermer
  BoutonsFixe     WS_POPUP|WS_CAPTION|WS_SYSMENU   Fermer seul
  BoutonsReduire  WS_MINIMIZEBOX sans MAXIMIZEBOX  Réduire, Agrandir grisé, Fermer
Les boutons sont les sous-fenêtres carrées visibles du cadre d'openbox
(disposition « LIMC », sans icône), de gauche à droite. Un bouton grisé est
dessiné avec une couleur « disabled » du thème Winlux. La fonction Réduire
reste offerte (_NET_WM_ALLOWED_ACTIONS), et le programme réduit toujours
lui-même sa fenêtre sans bouton Réduire : relancé avec « reduire », il appelle
ShowWindow(BoutonsFixe, SW_MINIMIZE) et la fenêtre doit passer à l'état
iconique (WM_STATE), seul risque de régression de ce correctif.

    WINLUX_OPENBOX=build/openbox/openbox python3 scripts/test-openbox-boutons.py
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
WINLUX = Path(os.environ.get("WINLUX_BIN", ROOT / "target/release/winlux"))
OPENBOX = Path(os.environ.get("WINLUX_OPENBOX", ROOT / "build/openbox/openbox"))
CC = os.environ.get("WINLUX_MINGW_CC", "x86_64-w64-mingw32-gcc")
THEMERC = ROOT / "integration/native/themes/Winlux/openbox-3/themerc"


def run(*args, **kw):
    return subprocess.run(args, capture_output=True, text=True, **kw)


def disabled_colours():
    colours = set()
    for line in THEMERC.read_text(encoding="utf-8").splitlines():
        key, _, value = line.partition(":")
        if key.strip().endswith("button.disabled.image.color"):
            colours.add(value.strip().upper())
    return colours


def frame_of(title):
    clients = re.findall(r"0x[0-9a-f]+", run("xprop", "-root", "_NET_CLIENT_LIST").stdout)
    for client in clients:
        if f'"{title}"' not in run("xprop", "-id", client, "WM_NAME").stdout:
            continue
        window = client
        while True:
            tree = run("xwininfo", "-id", window, "-tree").stdout
            parent = re.search(r"Parent window id: (0x[0-9a-f]+)", tree)
            root = re.search(r"Root window id: (0x[0-9a-f]+)", tree)
            if not parent or not root:
                return None, None
            if parent.group(1) == root.group(1):
                return window, client
            window = parent.group(1)
    return None, None


def buttons(frame, work, colours):
    """Boutons visibles du cadre, de gauche à droite : (x, grisé ?)."""
    found = []
    tree = run("xwininfo", "-id", frame, "-tree").stdout
    for window in re.findall(r"^\s+(0x[0-9a-f]+)", tree, re.M):
        info = run("xwininfo", "-id", window).stdout
        if "IsViewable" not in info:
            continue
        width = int(re.search(r"Width: (\d+)", info).group(1))
        height = int(re.search(r"Height: (\d+)", info).group(1))
        if width != height or width < 8:
            continue
        x = int(re.search(r"Absolute upper-left X: +(-?\d+)", info).group(1))
        shot = work / f"{window}.xwd"
        shot.write_bytes(subprocess.run(["xwd", "-id", window, "-silent"], capture_output=True).stdout)
        pixels = run("convert", str(shot), "txt:-").stdout.upper()
        found.append((x, any(c in pixels for c in colours)))
    return sorted(found)


def main():
    for tool in (CC, "Xvfb", "xdotool", "xprop", "xwininfo", "xwd", "convert"):
        if not shutil.which(tool):
            print(f"{tool} absent : test sauté"); return 0
    if not WINLUX.is_file():
        print("runtime absent : test sauté"); return 0
    if not OPENBOX.is_file():
        print(f"ÉCHEC : openbox de Winlux absent ({OPENBOX}) : sh scripts/build-openbox.sh build/openbox")
        return 1
    colours = disabled_colours()
    if not colours:
        print("ÉCHEC : le thème Winlux ne définit aucune couleur de bouton désactivé"); return 1
    with tempfile.TemporaryDirectory() as temp:
        work = Path(temp)
        exe = work / "boutons.exe"
        subprocess.run([CC, "-O1", "-o", str(exe), str(ROOT / "testc/boite/boutons_titre.c"), "-luser32"], check=True)
        themes = work / ".local/share/themes"
        shutil.copytree(THEMERC.parent.parent, themes / "Winlux")
        (work / "rc.xml").write_text(
            '<?xml version="1.0" encoding="UTF-8"?>\n'
            '<openbox_config xmlns="http://openbox.org/3.4/rc">\n'
            "  <theme><name>Winlux</name><titleLayout>LIMC</titleLayout></theme>\n"
            "</openbox_config>\n")
        read, write = os.pipe()
        xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(write), "-screen", "0", "1024x400x24", "-nolisten", "tcp"],
                                pass_fds=(write,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.close(write)
        display = os.read(read, 16).decode().strip()
        os.environ["DISPLAY"] = f":{display}"
        env = dict(os.environ, HOME=str(work))
        procs = [xvfb]
        try:
            manager = subprocess.Popen([str(OPENBOX), "--sm-disable", "--config-file", str(work / "rc.xml")],
                                       env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            procs.append(manager)
            for _ in range(300):
                if "window id" in run("xprop", "-root", "_NET_SUPPORTING_WM_CHECK").stdout:
                    break
                time.sleep(0.1)
            out = open(work / "out.txt", "w")
            program = subprocess.Popen([str(WINLUX), str(exe)], cwd=work, stdout=out, stderr=out)
            procs.append(program)
            titles = ("BoutonsTous", "BoutonsFixe", "BoutonsReduire")
            frames = {}
            for _ in range(150):
                frames = {t: frame_of(t) for t in titles}
                if all(f[0] for f in frames.values()):
                    break
                time.sleep(0.2)
            time.sleep(1)
            if not all(f[0] for f in frames.values()):
                print("ÉCHEC : fenêtres jamais gérées par openbox")
                print((work / "out.txt").read_text(errors="replace")[-2000:]); return 1
            seen = {t: buttons(frames[t][0], work, colours) for t in titles}
            for t in titles:
                print(f"{t} : " + ", ".join("grisé" if d else "actif" for _, d in seen[t]))
            expected = {"BoutonsTous": [False, False, False], "BoutonsFixe": [False],
                        "BoutonsReduire": [False, True, False]}
            fail = 0
            for t in titles:
                got = [d for _, d in seen[t]]
                if got != expected[t]:
                    print(f"ÉCHEC : {t} attendu " + ", ".join("grisé" if d else "actif" for d in expected[t]))
                    fail = 1
            actions = run("xprop", "-id", frames["BoutonsFixe"][1], "_NET_WM_ALLOWED_ACTIONS").stdout
            print("BoutonsFixe :", actions.strip())
            if "_NET_WM_ACTION_MINIMIZE" not in actions:
                print("ÉCHEC : le programme doit pouvoir encore réduire sa fenêtre (ShowWindow)"); fail = 1
            # ShowWindow(SW_MINIMIZE) sur une fenêtre sans bouton Réduire.
            program.kill(); program.wait()
            for _ in range(50):
                if not frame_of("BoutonsFixe")[0]:
                    break
                time.sleep(0.1)
            program = subprocess.Popen([str(WINLUX), str(exe), "reduire"], cwd=work, stdout=out, stderr=out)
            procs.append(program)
            state = ""
            for _ in range(100):
                client = frame_of("BoutonsFixe")[1]
                state = run("xprop", "-id", client, "WM_STATE").stdout if client else ""
                if "Iconic" in state:
                    break
                time.sleep(0.1)
            print("BoutonsFixe après ShowWindow(SW_MINIMIZE) :", " ".join(state.split()))
            if "Iconic" not in state:
                print("ÉCHEC : ShowWindow(SW_MINIMIZE) ne réduit plus une fenêtre sans bouton Réduire"); fail = 1
            if fail:
                print((work / "out.txt").read_text(errors="replace")[-2000:]); return 1
            print("Boutons Réduire et Agrandir selon le style Win32 (#3064)")
            return 0
        finally:
            for p in reversed(procs):
                p.kill()
                p.wait()


if __name__ == "__main__":
    sys.exit(main())
