#!/usr/bin/python3
"""#3343 : Notify répond aussitôt, quel que soit l'historique. Avant, la
réponse attendait la réécriture complète de l'historique et la bulle :
jusqu'à 55 ms par notification (historique plein, corps de 8 192
caractères, bulle affichée), et 3,3 Mo réécrits à chaque fois.

À lancer sous xvfb-run et dbus-run-session (scripts/test-session-bus.conf).
"""
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time

from gi.repository import Gio, GLib

HERE = Path(__file__).resolve().parent
NAME = "org.freedesktop.Notifications"
failures = []


def check(cond, label):
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures.append(label)


with tempfile.TemporaryDirectory(prefix="winlux-3343.") as temporary:
    root = Path(temporary)
    state = root / "state/winlux/notifications.json"
    state.parent.mkdir(parents=True)
    # Historique plein, corps à la borne du démon : le pire cas d'avant.
    state.write_text(json.dumps([dict(id=n, app="Essai", title=f"n° {n}", body="é" * 8192, actions=[], time=0)
                                 for n in range(1, 201)], ensure_ascii=False))
    env = dict(os.environ, HOME=temporary, XDG_STATE_HOME=str(root / "state"), XDG_CONFIG_HOME=str(root / "config"),
               NO_AT_BRIDGE="1", WINLUX_LANG="fr", WINLUX_SOUNDS_DIR=str(root / "sons"))
    daemon = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-notifications.py")], env=env,
                              stderr=subprocess.DEVNULL)
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

        def notify(i):
            start = time.perf_counter()
            identity = bus.call_sync(NAME, "/org/freedesktop/Notifications", NAME, "Notify",
                                     GLib.Variant("(susssasa{sv}i)", ("Essai", 0, "", f"Mesure {i}", "é" * 8192, [], {}, 400)),
                                     None, Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
            return identity, (time.perf_counter() - start) * 1000

        for _ in range(100):
            try:
                bus.call_sync(NAME, "/org/freedesktop/Notifications", NAME, "GetCapabilities", None, None,
                              Gio.DBusCallFlags.NONE, 1000, None); break
            except GLib.Error:
                if daemon.poll() is not None:
                    sys.exit("ÉCHEC : le démon s'est arrêté au démarrage")
                time.sleep(0.05)
        for i in range(5):                       # mise en route (polices et icônes de la bulle)
            notify(i); time.sleep(0.1)
        delays = []
        for i in range(30):
            time.sleep(0.05)                     # une notification à la fois, bulle affichée
            identity, delay = notify(i)
            delays.append(delay)
        median = statistics.median(delays)
        print(f"Notify, historique plein (200 × 8 192 caractères), bulle affichée : médiane {median:.2f} ms, max {max(delays):.1f} ms")
        # Avant : 55 ms de médiane dans le conteneur. Après : 2 ms environ.
        check(median < 10, "Notify répond sans attendre le disque ni la bulle (médiane sous 10 ms)")

        for _ in range(40):
            try:
                if json.loads(state.read_text())[-1]["id"] == identity: break
            except (OSError, ValueError, IndexError):
                pass
            time.sleep(0.05)
        history = json.loads(state.read_text())
        check(history[-1]["id"] == identity and len(history) == 200, "historique écrit dans les 2 s, toujours borné à 200")
        check(daemon.poll() is None, "démon toujours vivant")
    finally:
        daemon.terminate()
        daemon.wait(timeout=5)

if failures:
    sys.exit(f"Notify, temps de réponse : {len(failures)} échec(s)")
print("Notify répond aussitôt, historique plein et bulle affichée (#3343)")
