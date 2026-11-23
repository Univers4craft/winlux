#!/usr/bin/python3
"""#3278 : le démon de notifications ne grossit pas avec les notifications
reçues. Avant, chaque appel D-Bus servi par register_object gardait son
GDBusMethodInvocation : environ 3 Ko par notification, sans plafond.

À lancer sous xvfb-run et dbus-run-session (scripts/test-session-bus.conf).
Le démon tourne avec « Ne pas déranger » : aucune bulle, seule la mémoire
du chemin D-Bus et de l'historique (borné à 200) est mesurée.
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from gi.repository import Gio, GLib

HERE = Path(__file__).resolve().parent
PATH = "/org/freedesktop/Notifications"
NAME = "org.freedesktop.Notifications"
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


with tempfile.TemporaryDirectory(prefix="winlux-3278.") as temporary:
    root = Path(temporary)
    env = dict(os.environ, HOME=temporary, XDG_STATE_HOME=str(root / "state"),
               XDG_CONFIG_HOME=str(root / "config"), NO_AT_BRIDGE="1", WINLUX_LANG="fr")
    (root / "config/winlux").mkdir(parents=True)
    (root / "config/winlux/do-not-disturb").touch()
    daemon = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-notifications.py")], env=env)
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

        def call(method, params=None, interface=NAME):
            return bus.call_sync(NAME, PATH, interface, method, params, None, Gio.DBusCallFlags.NONE, 3000, None)

        for _ in range(100):
            try:
                call("GetCapabilities"); break
            except GLib.Error:
                if daemon.poll() is not None:
                    sys.exit("ÉCHEC : le démon s'est arrêté au démarrage")
                time.sleep(0.05)

        def notify(i):
            return call("Notify", GLib.Variant("(susssasa{sv}i)", ("Test 3278", 0, "", f"Essai {i}", "corps", [], {}, 500))).unpack()[0]

        # Les réponses restent celles d'avant : capacités, identifiant, erreurs.
        check(call("GetServerInformation").unpack() == ("Winlux", "Winlux", "1.0", "1.2"), "GetServerInformation répond")
        first = notify(0)
        check(notify(1) == first + 1, "Notify rend des identifiants qui se suivent")
        try:
            call("CloseNotification", GLib.Variant("(s)", ("x",))); bad = None
        except GLib.Error as error:
            bad = Gio.DBusError.get_remote_error(error)
        check(bad == "org.freedesktop.DBus.Error.InvalidArgs", f"mauvaise signature refusée ({bad})")
        try:
            call("Inexistante"); unknown = None
        except GLib.Error as error:
            unknown = Gio.DBusError.get_remote_error(error)
        check(unknown == "org.freedesktop.DBus.Error.UnknownMethod", f"méthode inconnue refusée ({unknown})")
        # Appel sans interface, permis par D-Bus.
        message = Gio.DBusMessage.new_method_call(NAME, PATH, None, "GetCapabilities")
        reply = bus.send_message_with_reply_sync(message, Gio.DBusSendMessageFlags.NONE, 3000, None)[0]
        check(reply.get_message_type() == Gio.DBusMessageType.METHOD_RETURN
              and "body" in reply.get_body().unpack()[0], "appel sans interface servi")
        xml = call("Introspect", interface="org.freedesktop.DBus.Introspectable").unpack()[0]
        check('name="Notify"' in xml and 'name="Dismiss"' in xml, "introspection : les deux interfaces")
        call("Dismiss", GLib.Variant("(u)", (first,)), "org.winlux.Notifications")
        call("Clear", None, "org.winlux.Notifications")

        # Mise en route : l'historique atteint son plafond de 200 et les
        # réserves de l'interpréteur se remplissent (palier vers 1 000 à 5 000).
        for i in range(1500):
            notify(i)
        time.sleep(1)
        before = rss(daemon.pid)
        count = 2000
        for i in range(count):
            notify(i)
        time.sleep(1)
        after = rss(daemon.pid)
        per = (after - before) / count
        print(f"VmRSS : {before} Ko puis {after} Ko après {count} notifications, {per:.2f} Ko par notification")
        # Avant le correctif : 3,0 Ko (conteneur) et 3,6 Ko (ISO) par notification.
        check(per < 0.5, "mémoire stable : moins de 0,5 Ko par notification (#3278)")
        check(daemon.poll() is None, "démon toujours vivant")
    finally:
        daemon.terminate()
        daemon.wait(timeout=5)

if failures:
    sys.exit(f"Démon de notifications, mémoire : {len(failures)} échec(s)")
print("Démon de notifications : mémoire stable et réponses D-Bus conformes (#3278)")
