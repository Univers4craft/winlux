#!/usr/bin/python3
"""Après un clic sur la croix (ou Alt+F4) : « <Nom> ne répond pas » si rien ne se ferme.

    winlux-close-watch [WID]

Openbox envoie Close (WM_DELETE_WINDOW) puis lance ce script avec $wid. Au
bout de 5 s, si la fenêtre est toujours là, que son programme vit encore et
qu'il n'a ouvert aucune nouvelle fenêtre ni question rattachée à celle-ci
(« Enregistrer les modifications ? » prouve qu'il répond), winlux-force-close propose
« Fermer le programme » ou « Attendre », comme Windows.

WINLUX_CLOSE_WATCH_DELAY change le délai (tests).
"""
import os
from pathlib import Path
import re
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
# Fenêtres de la session elle-même : jamais de « ne répond pas » pour elles.
IGNORED = {"xfce4-panel", "xfdesktop", "wrapper-2.0", "winlux-force-close", "winlux-popup"}


def xprop(*args):
    try:
        return subprocess.run(["xprop", *args], capture_output=True, text=True, timeout=3).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def clients():
    """Fenêtres gérées par le gestionnaire de fenêtres (_NET_CLIENT_LIST)."""
    return {int(item, 16) for item in re.findall(r"0x[0-9a-fA-F]+", xprop("-root", "_NET_CLIENT_LIST").split("=", 1)[-1])}


def pid_of(window):
    found = re.search(r"= (\d+)", xprop("-id", str(window), "_NET_WM_PID"))
    return int(found[1]) if found else 0


def transient_for(window):
    found = re.search(r"window id # (0x[0-9a-fA-F]+)", xprop("-id", str(window), "WM_TRANSIENT_FOR"))
    return int(found[1], 16) if found else 0


def windows_of(pid, among):
    return {window for window in among if pid_of(window) == pid}


def main(argv):
    try:
        window = int(argv[1], 0) if len(argv) > 1 else 0
    except ValueError:
        window = 0
    if not window:
        found = re.search(r"window id # (0x[0-9a-fA-F]+)", xprop("-root", "_NET_ACTIVE_WINDOW"))
        window = int(found[1], 16) if found else 0
    if not window:
        return 0
    klass = re.search(r'WM_CLASS\(\w+\) = "(.*)", "(.*)"', xprop("-id", str(window), "WM_CLASS"))
    if klass and (klass[1] in IGNORED or klass[2].lower() in IGNORED):
        return 0
    pid = pid_of(window)
    if pid <= 1:
        return 0
    # Un seul guet par fenêtre, même si l'on clique plusieurs fois sur la croix.
    lock = Path(os.environ.get("XDG_RUNTIME_DIR") or "/tmp") / f"winlux-close-watch-{os.getuid()}-{window}"
    try:
        if lock.exists() and time.time() - lock.stat().st_mtime > 60:
            lock.unlink()
        descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        os.close(descriptor)
    except OSError:
        return 0
    try:
        before = windows_of(pid, clients())
        time.sleep(float(os.environ.get("WINLUX_CLOSE_WATCH_DELAY", "5")))
        now = clients()
        if window not in now or not Path(f"/proc/{pid}").exists() or pid_of(window) != pid:
            return 0
        # Le programme répond s'il a ouvert une fenêtre depuis, ou s'il pose une
        # question rattachée à celle-ci (souvent ouverte avant même ce guet).
        mine = windows_of(pid, now)
        if mine - before or any(transient_for(other) == window for other in mine if other != window):
            return 0
        tool = HERE / "winlux-force-close.py"
        command = [sys.executable, str(tool)] if tool.is_file() else ["winlux-force-close"]
        return subprocess.run([*command, "--hung", str(window)]).returncode
    finally:
        try:
            lock.unlink()
        except OSError:
            pass


if __name__ == "__main__":
    sys.exit(main(sys.argv))
