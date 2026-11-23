#!/usr/bin/python3
"""Centre de notifications (#3057) : la pastille de la cloche compte les
notifications nouvelles. Une bulle expirée ou une notification reçue en Ne pas
déranger reste nouvelle ; ouvrir le centre les marque vues, sans les retirer
de l'historique. Le compte est privé au compte (0600, sous XDG_STATE_HOME).

    xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session -- python3 -B scripts/test-notification-nouvelles.py
"""
import json
import os
import re
from pathlib import Path
import runpy
import stat
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
problems = []


def check(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label, flush=True)
    if not condition: problems.append(label)


home = Path(tempfile.mkdtemp(prefix="notifications-nouvelles-"))
os.environ.update(HOME=str(home), XDG_STATE_HOME=str(home / "state"), XDG_CONFIG_HOME=str(home / "config"),
                  XDG_CACHE_HOME=str(home / "cache"), WINLUX_LANG="fr", NO_AT_BRIDGE="1")
state = home / "state/winlux"
count_file = state / "notifications-nouvelles"
python = sys.executable
daemon = subprocess.Popen([python, "-B", str(HERE / "winlux-notifications.py")], stderr=subprocess.DEVNULL)
tray = runpy.run_path(str(HERE / "winlux-tray.py"), run_name="test_3057")

from gi.repository import Gio, GLib  # noqa: E402  (après l'environnement)
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)


def notify(title, timeout=500):
    bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications",
                  "Notify", GLib.Variant("(susssasa{sv}i)", ("Essai", 0, "", title, "texte", [], {}, timeout)),
                  None, Gio.DBusCallFlags.NONE, 3000, None)


def wait(condition, seconds=8):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if condition(): return True
        time.sleep(0.1)
    return condition()


def unseen():
    try: return int(count_file.read_text().strip())
    except (OSError, ValueError): return -1


popup = None
try:
    check(wait(lambda: subprocess.run(["gdbus", "introspect", "--session", "-d", "org.freedesktop.Notifications",
                                       "-o", "/org/freedesktop/Notifications"], capture_output=True).returncode == 0),
          "démon de notifications sur le bus")
    notify("Première"); notify("Deuxième")
    time.sleep(1.2)  # bulles expirées sans clic : manquées
    print(f"compte après deux bulles expirées : {unseen()}", flush=True)
    check(unseen() == 2, "deux bulles expirées : deux notifications nouvelles")
    (home / "config/winlux").mkdir(parents=True, exist_ok=True)
    (home / "config/winlux/do-not-disturb").touch()
    notify("Pendant Ne pas déranger")
    # Le démon répond avant d'écrire (#3343) : le compte suit dans la seconde.
    arrived = wait(lambda: unseen() == 3, 1)
    print(f"compte après Ne pas déranger : {unseen()}", flush=True)
    check(arrived, "reçue en Ne pas déranger : nouvelle aussi")
    line = tray["render"]("notifications")
    print(f"cloche : {line}", flush=True)
    check("3 nouvelles notifications" in line and "notifications-3-" in line,
          "cloche : pastille « 3 » et infobulle « 3 nouvelles notifications »")
    check(stat.S_IMODE(count_file.stat().st_mode) == 0o600, "compte privé au compte (0600)")
    popup = subprocess.Popen([python, "-B", str(HERE / "winlux-popup.py"), "notifications"], stderr=subprocess.DEVNULL)
    ok = wait(lambda: unseen() == 0, 15)
    print(f"compte après ouverture du centre : {unseen()}", flush=True)
    check(ok, "centre ouvert : pastille éteinte")
    history = json.loads((state / "notifications.json").read_text())
    print(f"historique : {[item['title'] for item in history]}", flush=True)
    check(len(history) == 3 and not any(item.get("nouvelle") for item in history),
          "historique gardé, les trois notifications marquées vues")
    line = tray["render"]("notifications")
    print(f"cloche : {line}", flush=True)
    check("nouvelle" not in line and re.search(r"/notifications-\d", line) is None,
          "cloche sans pastille une fois vues")
    popup.kill(); popup.wait(); popup = None
    notify("Après le centre")
    check(wait(lambda: unseen() == 1, 1), "nouvelle notification après fermeture du centre : pastille rallumée")
finally:
    for process in (popup, daemon):
        if process: process.kill()

if problems:
    print(f"{len(problems)} échec(s)"); sys.exit(1)
print("Notifications nouvelles : tout est conforme (#3057)")
