#!/usr/bin/python3
"""Menu Démarrer et écran verrouillé (#2994).

Le menu préchargé reçoit l'activation D-Bus (touche Windows, ou tout
programme de la session) pendant que winlux-lock tient l'écran : il ne
s'affiche pas. Le verrou levé, la même activation l'ouvre. Un marqueur dont
le pid est mort ne bloque rien.

À lancer dans un Xvfb et un bus de session dédiés :
  xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session -- /usr/bin/python3 -B scripts/test-start-menu-verrou.py
"""
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "winlux-start-menu.py"
failures = []


def ok(condition, message):
    print(("OK    " if condition else "ÉCHEC ") + message)
    if not condition:
        failures.append(message)


def visible():
    return bool(subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Démarrer Winlux"],
                               capture_output=True, text=True).stdout.split())


def owned():
    result = subprocess.run(["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus",
                             "--object-path", "/org/freedesktop/DBus", "--method",
                             "org.freedesktop.DBus.NameHasOwner", "org.winlux.Start"],
                            capture_output=True, text=True)
    return "true" in result.stdout


def wait(condition, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if condition():
            return True
        time.sleep(0.05)
    return condition()


def activate():
    subprocess.run(["gdbus", "call", "--session", "--dest", "org.winlux.Start", "--object-path",
                    "/org/winlux/Start", "--method", "org.freedesktop.Application.Activate", "{}"],
                   capture_output=True, timeout=10)


home = Path(tempfile.mkdtemp(prefix="winlux-start-verrou-"))
lock_root = home / "racine"
marker = lock_root / "run/winlux/lock" / ("active" + ("" if os.environ.get("DISPLAY", ":0").rpartition(":")[2].split(".")[0] in ("", "0")
                                                      else "-" + "".join(c for c in os.environ["DISPLAY"].rpartition(":")[2].split(".")[0] if c.isdigit())))
marker.parent.mkdir(parents=True)
env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"),
       "XDG_DATA_HOME": str(home / ".local/share"), "WINLUX_LANG": "fr", "WINLUX_OSK_ROOT": str(home),
       "WINLUX_LOCK_ROOT": str(lock_root)}
# Le verrou : un processus vivant dont le pid est dans le marqueur.
locker = subprocess.Popen(["sleep", "120"])
menu = subprocess.Popen([sys.executable, "-B", str(SCRIPT), "--preload"], env=env,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    ok(wait(owned, 30), "menu préchargé")
    time.sleep(1)
    marker.write_text(f"{locker.pid}\n")
    for _ in range(2):
        activate()
        time.sleep(1.5)
        ok(not visible(), "écran verrouillé : l'activation n'affiche pas le menu (#2994)")
    locker.kill()
    locker.wait()
    activate()
    ok(wait(visible, 10), "verrou terminé (pid mort, marqueur resté) : le menu s'ouvre")
    activate()
    ok(wait(lambda: not visible(), 5), "second appui : le menu se referme")
    marker.unlink()
    activate()
    ok(wait(visible, 10), "sans marqueur : le menu s'ouvre")
finally:
    menu.kill()
    menu.wait()
    if locker.poll() is None:
        locker.kill()

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("OK: menu Démarrer fermé tant que l'écran est verrouillé (#2994)")
