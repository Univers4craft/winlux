#!/usr/bin/python3
"""Menu Démarrer préchargé à l'ouverture de session, réveillé par D-Bus.

winlux-start-menu --preload construit le menu caché : aucune fenêtre ne
s'affiche, et un second --preload rend la main sans rien ouvrir. Le lanceur
/usr/bin/winlux-start-menu (winlux-start-menu.sh) ouvre et referme le menu
résident par gdbus, sans démarrer Python, bien plus vite qu'un lancement à
froid (mesure indicative) ; menu absent, il le lance comme avant.

À lancer dans un Xvfb et un bus de session dédiés :
  xvfb-run -a dbus-run-session -- /usr/bin/python3 -B scripts/test-start-menu-preload.py
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
    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Démarrer Winlux"],
                           capture_output=True, text=True).stdout.split()
    return bool(found)


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


home = Path(tempfile.mkdtemp(prefix="winlux-start-preload-"))
env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"),
       "XDG_DATA_HOME": str(home / ".local/share"), "WINLUX_LANG": "fr", "WINLUX_OSK_ROOT": str(home)}
menu = [sys.executable, "-B", str(SCRIPT)]
program = home / "winlux-start-menu"
program.write_text(f"#!/bin/sh\nexec {sys.executable} -B {SCRIPT} \"$@\"\n")
program.chmod(0o755)
env["WINLUX_START_MENU_PROGRAM"] = str(program)
launcher = [str(SCRIPT.with_name("winlux-start-menu.sh"))]

# Menu absent : le lanceur le démarre comme avant (mesure de référence).
start = time.monotonic()
cold = subprocess.Popen(launcher, env=env)
ok(wait(visible, 30), "lanceur, menu absent : le menu démarre et s'ouvre")
cold_ms = (time.monotonic() - start) * 1000
cold.kill(); cold.wait()
wait(lambda: not owned(), 5)

resident = subprocess.Popen(menu + ["--preload"], env=env)
try:
    ok(wait(owned, 30), "préchargement : le menu tourne pour la session")
    time.sleep(1)
    ok(not visible(), "préchargement : aucune fenêtre affichée")
    second = subprocess.run(menu + ["--preload"], env=env, timeout=30)
    time.sleep(0.5)
    ok(second.returncode == 0 and not visible() and resident.poll() is None,
       "second préchargement : rend la main sans ouvrir le menu")
    start = time.monotonic()
    subprocess.run(launcher, env=env, timeout=30)
    ok(wait(visible, 10), "premier appui : le menu s'ouvre")
    warm_ms = (time.monotonic() - start) * 1000
    print(f"      à froid {cold_ms:.0f} ms, préchargé {warm_ms:.0f} ms")
    ok(warm_ms * 2 < cold_ms, "premier appui au moins deux fois plus rapide qu'un lancement à froid")
    subprocess.run(launcher, env=env, timeout=30)
    ok(wait(lambda: not visible(), 5), "second appui : le menu se referme")
    time.sleep(0.5)
    subprocess.run(menu, env=env, timeout=30)
    ok(wait(visible, 10), "lancement direct en Python : rouvre le menu résident")
    subprocess.run(launcher, env=env, timeout=30)
    wait(lambda: not visible(), 5)
    ok(resident.poll() is None, "le menu reste résident une fois refermé")
finally:
    resident.kill()
    resident.wait()

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("OK: menu Démarrer préchargé à l'ouverture de session")
