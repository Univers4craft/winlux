#!/usr/bin/python3
"""#3341 : le service org.freedesktop.ScreenSaver ne grossit pas avec les
appels D-Bus. Avant, chaque appel servi par register_object gardait son
GDBusMethodInvocation : environ 2,3 Ko par appel, sans plafond.

À lancer dans dbus-run-session. xset et xscreensaver-command sont remplacés
par des faux : rien ne touche à l'écran réel.
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from gi.repository import Gio, GLib

HERE = Path(__file__).resolve().parent
NAME = "org.freedesktop.ScreenSaver"
failures = []


def check(cond, label):
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures.append(label)


def rss(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmRSS"):
            return int(line.split()[1])
    return 0


def remote_error(call):
    try:
        call()
    except GLib.Error as error:
        return Gio.DBusError.get_remote_error(error)
    return None


with tempfile.TemporaryDirectory(prefix="winlux-3341.") as temporary:
    root = Path(temporary)
    (root / "bin").mkdir()
    for name in ("xset", "xscreensaver-command"):
        (root / "bin" / name).write_text("#!/bin/sh\nexit 0\n")
        (root / "bin" / name).chmod(0o755)
    env = dict(os.environ, HOME=temporary, XDG_RUNTIME_DIR=temporary, PATH=f"{root / 'bin'}:{os.environ['PATH']}",
               WINLUX_POWER_INHIBIT_DIR=str(root / "inhibit"), WINLUX_SESSION_LOGS=str(root / "logs"))
    service = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-screensaver-inhibit.py")], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

        def call(method, params=None, path="/org/freedesktop/ScreenSaver", interface=NAME):
            return bus.call_sync(NAME, path, interface, method, params, None, Gio.DBusCallFlags.NONE, 3000, None)

        for _ in range(100):
            try:
                call("GetActive"); break
            except GLib.Error:
                if service.poll() is not None:
                    sys.exit("ÉCHEC : le service s'est arrêté au démarrage")
                time.sleep(0.05)

        cookie = call("Inhibit", GLib.Variant("(ss)", ("test", "vidéo")), "/ScreenSaver").unpack()[0]
        check(cookie > 0 and any((root / "inhibit").iterdir()), "Inhibit par /ScreenSaver : cookie et demande posée")
        call("UnInhibit", GLib.Variant("(u)", (cookie,)))
        check(not any((root / "inhibit").iterdir()), "UnInhibit : demande retirée")
        check(call("GetActive").unpack() == (False,), "GetActive répond")
        check(remote_error(lambda: call("UnInhibit", GLib.Variant("(s)", ("x",)))) == "org.freedesktop.DBus.Error.InvalidArgs",
              "mauvaise signature refusée")
        check(remote_error(lambda: call("Inexistante")) == "org.freedesktop.DBus.Error.UnknownMethod", "méthode inconnue refusée")
        xml = call("Introspect", path="/ScreenSaver", interface="org.freedesktop.DBus.Introspectable").unpack()[0]
        check('name="Inhibit"' in xml, "introspection")

        def pair():
            value = call("Inhibit", GLib.Variant("(ss)", ("test", "vidéo"))).unpack()[0]
            call("UnInhibit", GLib.Variant("(u)", (value,)))

        # Mise en route : réserves de l'interpréteur remplies (palier vers 2 000 appels).
        for _ in range(1500):
            pair()
        time.sleep(1)
        before = rss(service.pid)
        count = 2000
        for _ in range(count):
            pair()
        time.sleep(1)
        after = rss(service.pid)
        per = (after - before) / (2 * count)
        print(f"VmRSS : {before} Ko puis {after} Ko après {2 * count} appels, {per:.2f} Ko par appel")
        # Avant le correctif : 2,3 Ko par appel (conteneur).
        check(per < 0.5, "mémoire stable : moins de 0,5 Ko par appel (#3341)")
        check(service.poll() is None, "service toujours vivant")
    finally:
        service.terminate()
        service.wait(timeout=5)

if failures:
    sys.exit(f"Veille de l'écran, mémoire : {len(failures)} échec(s)")
print("Veille de l'écran : mémoire stable et réponses D-Bus conformes (#3341)")
