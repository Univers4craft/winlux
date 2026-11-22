#!/usr/bin/python3
"""Menu Démarrer préchargé : l'ouverture ne dépend pas de l'historique (#2858).

Ce qui est mesuré : le temps entre la demande d'ouverture par D-Bus (ce que
fait la touche Windows) et la fenêtre visible pour X (xdotool --onlyvisible),
médiane de cinq ouvertures, d'abord avec un dossier personnel vide, puis avec
1 000 fichiers récents dans recently-used.xbel et 3 000 fichiers dans
Téléchargements. Avant #2858, le second cas prenait 6 à 10 fois plus de temps
(270 à 470 ms contre 40 ms) : le volet « Pour vous » relisait tout
l'historique deux fois à chaque ouverture et bloquait le menu.

Ce qui est supposé : le rapport entre les deux cas, mesuré ici sur une
machine rapide, se retrouve sur un PC lent ; les valeurs absolues, elles,
n'y sont pas comparables.

Vérifie aussi que l'historique relu seulement quand il change reste juste :
fichier ajouté, fichier supprimé.

À lancer dans un Xvfb et un bus de session dédiés :
  xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session -- /usr/bin/python3 -B scripts/test-start-menu-ouverture.py
"""
import os
import runpy
import statistics
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
        time.sleep(0.005)
    return condition()


def activate():
    subprocess.run(["gdbus", "call", "--session", "--dest", "org.winlux.Start", "--object-path",
                    "/org/winlux/Start", "--method", "org.freedesktop.Application.Activate", "{}"],
                   capture_output=True, timeout=10)


def xbel(home, count):
    docs = home / "Documents"
    docs.mkdir(parents=True, exist_ok=True)
    lines = ['<?xml version="1.0" encoding="UTF-8"?>',
             '<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">']
    for index in range(count):
        path = docs / f"document {index:04d}.txt"
        path.write_text("x")
        stamp = f"2026-11-{1 + index % 20:02d}T10:{index % 60:02d}:00Z"
        lines.append(f'<bookmark href="file://{path.as_posix().replace(" ", "%20")}" added="{stamp}" '
                     f'modified="{stamp}" visited="{stamp}"><info><metadata owner="http://freedesktop.org">'
                     f'<bookmark:applications><bookmark:application name="mousepad" exec="mousepad %u" '
                     f'modified="{stamp}" count="1"/></bookmark:applications></metadata></info></bookmark>')
    lines.append("</xbel>")
    data = home / ".local/share"
    data.mkdir(parents=True, exist_ok=True)
    (data / "recently-used.xbel").write_text("\n".join(lines))


def opening_ms(files, downloads):
    home = Path(tempfile.mkdtemp(prefix="winlux-start-ouverture-"))
    apps = home / ".local/share/applications"
    apps.mkdir(parents=True)
    for index in range(150):
        (apps / f"essai{index:03d}.desktop").write_text(
            f"[Desktop Entry]\nType=Application\nName=Application {index:03d}\nExec=true\n"
            f"Icon=application-x-executable\nCategories=Utility;\n")
    if files:
        xbel(home, files)
    folder = home / "Téléchargements"
    for index in range(downloads):
        sub = folder / f"lot {index // 200}"
        sub.mkdir(parents=True, exist_ok=True)
        (sub / f"fichier {index}.bin").write_bytes(b"x")
    env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config"),
           "XDG_DATA_HOME": str(home / ".local/share"), "WINLUX_LANG": "fr", "WINLUX_OSK_ROOT": str(home)}
    menu = subprocess.Popen([sys.executable, "-B", str(SCRIPT), "--preload"], env=env,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    times = []
    try:
        if not wait(owned, 30):
            return None
        time.sleep(1)
        for _ in range(6):
            start = time.monotonic()
            activate()
            if not wait(visible, 10):
                return None
            times.append((time.monotonic() - start) * 1000)
            # Le volet « Pour vous » se complète après l'affichage : on lui
            # laisse le temps avant de refermer.
            time.sleep(0.8)
            activate()
            wait(lambda: not visible(), 5)
            time.sleep(0.3)
    finally:
        menu.kill()
        menu.wait()
        wait(lambda: not owned(), 5)
    return statistics.median(times[1:])


# Historique relu seulement quand il change : le résultat reste juste.
module = runpy.run_path(str(SCRIPT), run_name="start_ouverture_test")
home = Path(tempfile.mkdtemp(prefix="winlux-start-recent-"))
xbel(home, 5)
path = home / ".local/share/recently-used.xbel"
first = module["recent_files"](path, limit=3, home=home)
ok([target.name for target, _, _ in first] == ["document 0004.txt", "document 0003.txt", "document 0002.txt"],
   "fichiers récents : les trois plus récents, le plus récent d'abord")
(home / "Documents/document 0004.txt").unlink()
ok([target.name for target, _, _ in module["recent_files"](path, limit=3, home=home)]
   == ["document 0003.txt", "document 0002.txt", "document 0001.txt"],
   "fichier supprimé depuis la dernière lecture : il disparaît sans que l'historique ait changé")
xbel(home, 8)
ok(module["recent_files"](path, limit=1, home=home)[0][0].name == "document 0007.txt",
   "historique modifié : relu, le nouveau fichier apparaît")

empty = opening_ms(0, 0)
heavy = opening_ms(1000, 3000)
ok(empty is not None and heavy is not None, "le menu préchargé s'ouvre dans les deux cas")
if empty is not None and heavy is not None:
    print(f"      dossier vide {empty:.0f} ms, 1 000 fichiers récents et 3 000 téléchargements {heavy:.0f} ms (médianes)")
    ok(heavy <= empty * 2 + 40,
       "ouverture avec un historique chargé au plus deux fois plus lente qu'avec un dossier vide (#2858)")

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("OK: ouverture du menu Démarrer indépendante de l'historique (#2858)")
