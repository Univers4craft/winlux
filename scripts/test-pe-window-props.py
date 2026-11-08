#!/usr/bin/env python3
"""Fenêtres PE : propriétés X11 attendues par les gestionnaires de fenêtres,
pagers et lanceurs (#683).

Sous openbox (le gestionnaire de fenêtres de la session native), lance
l'Explorateur Winlux et une fenêtre Win32 ordinaire, puis exige sur chaque
fenêtre de haut niveau du processus : _NET_WM_PID (le bon), WM_CLASS,
WM_CLIENT_MACHINE, et les marques d'une fenêtre réellement gérée par openbox,
WM_STATE et _NET_WM_DESKTOP (bureau 0, pas absent ni -1). Sans eux, wmctrl
affiche « N/A » ou le bureau -1, `xdotool windowactivate` échoue et la Vue
des tâches ignore la fenêtre.

Usage : xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-pe-window-props.py
"""
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WINLUX = ROOT / "target/release/winlux"
PE = ROOT / "target/x86_64-pc-windows-msvc/release"
REQUIRED = ["_NET_WM_PID", "WM_CLASS", "WM_CLIENT_MACHINE", "WM_STATE", "_NET_WM_DESKTOP"]


def windows_of(pid: int) -> list[str]:
    """Fenêtres affichées du processus (une fenêtre retirée n'a pas à être gérée)."""
    out = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(pid)],
                         capture_output=True, text=True).stdout
    return [w for w in out.split() if w]


def exists(win: str) -> bool:
    """La fenêtre existe encore (une fenêtre de passage du programme peut
    être détruite entre la liste et la lecture de ses propriétés)."""
    return subprocess.run(["xwininfo", "-id", win], capture_output=True).returncode == 0


def parent_of(win: str) -> str:
    """Fenêtre X parente (cadre d'openbox pour une fenêtre gérée)."""
    out = subprocess.run(["xwininfo", "-tree", "-id", win], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if "Parent window id:" in line:
            return line.split(":", 1)[1].split()[0]
    return ""


def props(win: str) -> dict[str, str]:
    out = subprocess.run(["xprop", "-id", win] + REQUIRED, capture_output=True, text=True).stdout
    found = {}
    for line in out.splitlines():
        name = line.split("(", 1)[0].split(":", 1)[0].strip()
        if name in REQUIRED and "not found" not in line and "no such atom" not in line:
            found[name] = line.split("=", 1)[-1].strip() if "=" in line else line.split(":", 1)[-1].strip()
    return found


def check(label: str, cmd: list[str], env: dict[str, str], cycle: bool = False) -> list[str]:
    proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    errors = []
    try:
        deadline = time.time() + 40
        wins: list[str] = []
        while time.time() < deadline:
            wins = [w for w in windows_of(proc.pid) if "WM_CLASS" in props(w)]
            if wins:
                break
            time.sleep(0.5)
        if not wins:
            return [f"{label} : aucune fenêtre de haut niveau"]
        time.sleep(1)
        wins = [w for w in windows_of(proc.pid) if w in wins] or wins
        for w in wins:
            got = props(w)
            missing = [name for name in REQUIRED if name not in got]
            if missing and not got and not exists(w):
                print(f"  --     {label} : fenêtre {w} détruite entre-temps, ignorée")
                continue
            if missing:
                # Une fenêtre placée depuis dans une autre fenêtre du programme
                # (SetParent) n'est plus de haut niveau : rien à exiger d'elle.
                parent = parent_of(w)
                if parent and props(parent).get("_NET_WM_PID") == str(proc.pid):
                    print(f"  --     {label} : fenêtre {w} devenue enfant de {parent}, ignorée")
                    continue
                name = subprocess.run(["xprop", "-id", w, "WM_NAME"], capture_output=True,
                                      text=True).stdout.strip()
                errors.append(f"{label} : fenêtre {w} sans {', '.join(missing)} "
                              f"(parent {parent or '?'}, {name or 'sans WM_NAME'})")
            elif got["_NET_WM_PID"] != str(proc.pid):
                errors.append(f"{label} : _NET_WM_PID {got['_NET_WM_PID']} au lieu de {proc.pid}")
            elif not got["WM_CLIENT_MACHINE"].strip('"'):
                errors.append(f"{label} : WM_CLIENT_MACHINE vide")
            elif got["_NET_WM_DESKTOP"] != "0":
                errors.append(f"{label} : _NET_WM_DESKTOP = {got['_NET_WM_DESKTOP']} au lieu de 0")
            else:
                print(f"  OK     {label} : {', '.join(f'{k}={v}' for k, v in got.items())}")
        if cycle and not errors:
            # Réduire puis réafficher : la fenêtre doit rester gérée, avec son
            # bureau (cas relevé sur la VM après windowminimize puis snap).
            w = wins[0]
            # (Sans --sync : xdotool lit alors _NET_ACTIVE_WINDOW pendant la
            # transition et échoue même avec une fenêtre GTK.)
            subprocess.run(["xdotool", "windowminimize", w], capture_output=True, timeout=10)
            time.sleep(1)
            act = subprocess.run(["xdotool", "windowactivate", w], capture_output=True, text=True, timeout=10)
            got: dict[str, str] = {}
            for _ in range(20):
                time.sleep(0.25)
                got = props(w)
                if got.get("_NET_WM_DESKTOP") == "0" and "Normal" in subprocess.run(
                        ["xprop", "-id", w, "WM_STATE"], capture_output=True, text=True).stdout:
                    break
            if act.returncode != 0 or "failed" in act.stderr:
                errors.append(f"{label} : windowactivate après réduction refusé ({act.stderr.strip()})")
            elif got.get("_NET_WM_DESKTOP") != "0" or "WM_STATE" not in got:
                errors.append(f"{label} : après réduction puis réaffichage, {got}")
            else:
                print(f"  OK     {label} : réduite puis réaffichée, toujours gérée (bureau 0)")
    finally:
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
    return errors


def main() -> int:
    if not os.environ.get("DISPLAY"):
        print("test-pe-window-props : DISPLAY absent, lancez-le sous xvfb-run")
        return 2
    rc = ROOT / "integration/native/config/openbox/rc.xml"
    wm = subprocess.Popen(["openbox", "--config-file", str(rc)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(40):
        root = subprocess.run(["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"], capture_output=True, text=True).stdout
        if "window id" in root:
            break
        time.sleep(0.25)
    else:
        wm.kill()
        print("test-pe-window-props : openbox n'a pas démarré")
        return 2
    try:
        return run()
    finally:
        wm.terminate()


def run() -> int:
    with tempfile.TemporaryDirectory() as home:
        for d in ("Documents", "Bureau", "Téléchargements", "Images", "Musique", "Vidéos"):
            Path(home, d).mkdir()
        base = dict(os.environ, HOME=home, WINLUX_AUDIO="null", WINLUX_NO_SPLASH="1",
                    WINLUX_NO_LOGIN="1", WINLUX_NO_OOBE="1")
        errors = []
        errors += check(
            "Explorateur Winlux",
            [str(WINLUX), str(PE / "gui_demo.exe")],
            dict(base, WINLUX_NATIVE_SHELL="1", WINLUX_SHELL_APP="explorer"),
            cycle=True,
        )
        errors += check(
            "fenêtre Win32 (controls_suite)",
            [str(WINLUX), str(PE / "controls_suite.exe")],
            dict(base, WINLUX_CONTROLS_SHOW="20000"),
        )
    for e in errors:
        print(f"  ECHEC  {e}")
    if errors:
        return 1
    print("test-pe-window-props : propriétés X11 présentes sur les fenêtres PE")
    return 0


if __name__ == "__main__":
    sys.exit(main())
