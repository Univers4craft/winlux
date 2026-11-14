#!/usr/bin/python3
"""Bulles du contrôle parental (#2245) : application bloquée, « il reste
5 minutes », « il reste 1 minute ».

notify-send n'est pas dans l'image (#630) : le service doit passer par
org.freedesktop.Notifications avec gdbus (GLib). Le test :
  1. vérifie la commande produite quand notify-send est absent ;
  2. la lance sur un vrai bus de session privé (dbus-daemon) où un faux
     serveur de notifications relève ce qu'il reçoit, texte compris
     (apostrophes, guillemets, accents, retour à la ligne) ;
  3. vérifie que notify() lance cette commande sous l'identité du compte.
Sans root ni session : la partie setpriv est simulée.
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from unittest import mock

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-parental.py"
# Le Python du système porte PyGObject (python3-gi) pour le faux serveur.
PYGTK = os.environ.get("WINLUX_TEST_PYGTK", "/usr/bin/python3")
work = Path(tempfile.mkdtemp(prefix="winlux-parental-bulles."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work / "racine")
os.environ["WINLUX_LANG"] = "fr"
failures = 0


def require(tools, pygtk, modules):
    """Prérequis du test : chacun manquant est nommé avec son paquet, puis le
    test échoue sans aucun autre contrôle ; il n'est jamais sauté (#2296)."""
    missing = [f"{tool} (paquet {package})" for tool, package in tools if not shutil.which(tool)]
    code = "import gi\n" + "".join(f"gi.require_version({name!r}, {version!r})\n" for name, version in modules) + \
        "".join(f"from gi.repository import {name}\n" for name, _version in modules)
    try:
        probe = subprocess.run([pygtk, "-I", "-c", code], capture_output=True, timeout=60).returncode
    except (OSError, subprocess.SubprocessError):
        probe = 1
    if probe != 0:
        names = ", ".join(f"{name} {version}" for name, version in modules)
        missing.append(f"PyGObject ({names}) pour {pygtk} (paquets python3-gi, gir1.2-gtk-3.0 ; "
                       "ou WINLUX_TEST_PYGTK=un Python qui les a)")
    if missing:
        print("ÉCHEC prérequis manquant : " + " ; ".join(missing), flush=True)
        sys.exit(1)


require([("dbus-daemon", "dbus-daemon"), ("gdbus", "libglib2.0-bin")], PYGTK, [("Gio", "2.0")])


def check(cond, label):
    global failures
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures += 1


loader = SourceFileLoader("winlux_parental", str(TOOL))
spec = importlib.util.spec_from_loader("winlux_parental", loader)
parental = importlib.util.module_from_spec(spec)
loader.exec_module(parental)

TITLE = "Temps d’écran : il reste 5 minutes"
BODY = "« Mousepad » est bloquée par l'adulte\n\"guillemets\" \\ fin"

# 1. Commande sans notify-send.
real_which = shutil.which
with mock.patch.object(parental.shutil, "which",
                       side_effect=lambda name: None if name == "notify-send" else real_which(name)):
    command = parental.notify_command(TITLE, BODY)
check(command is not None and os.path.basename(command[0]) == "gdbus"
      and "org.freedesktop.Notifications.Notify" in command,
      "notify-send absent : la bulle passe par gdbus et org.freedesktop.Notifications (#2245)")
check(parental.gvariant_string("l'a\\b\nc") == "'l\\'a\\\\b\\nc'",
      "texte échappé au format GVariant (apostrophe, barre oblique inverse, retour à la ligne)")

# 2. Vrai bus privé, faux serveur de notifications.
bus_dir = work / "bus"
bus_dir.mkdir()
address = f"unix:path={bus_dir / 'bus'}"
daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", f"--address={address}"],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
received = work / "recu.json"
server_code = work / "serveur.py"
server_code.write_text('''
import json, sys
from gi.repository import Gio, GLib
XML = """<node><interface name="org.freedesktop.Notifications">
<method name="Notify"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/>
<arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="as" direction="in"/>
<arg type="a{sv}" direction="in"/><arg type="i" direction="in"/><arg type="u" direction="out"/></method>
</interface></node>"""
info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0]
loop = GLib.MainLoop()
def call(connection, sender, path, interface, method, params, invocation):
    with open(sys.argv[1], "w", encoding="utf-8") as out:
        json.dump(params.unpack(), out, ensure_ascii=False)
    invocation.return_value(GLib.Variant("(u)", (7,)))
def acquired(connection, name):
    connection.register_object("/org/freedesktop/Notifications", info, call, None, None)
Gio.bus_own_name(Gio.BusType.SESSION, "org.freedesktop.Notifications", Gio.BusNameOwnerFlags.NONE,
                 acquired, lambda *a: print("pret", flush=True), None)
GLib.timeout_add_seconds(30, loop.quit)
loop.run()
''', encoding="utf-8")
bus_env = {"PATH": "/usr/bin:/bin", "HOME": str(work), "DBUS_SESSION_BUS_ADDRESS": address, "LANG": "C.UTF-8"}
server = None
try:
    for _ in range(50):
        if (bus_dir / "bus").exists():
            break
        time.sleep(0.1)
    server = subprocess.Popen([PYGTK, "-I", "-B", str(server_code), str(received)], env=bus_env,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    ready = server.stdout.readline().strip() == "pret"
    check(ready, "faux serveur de notifications sur un bus de session privé")
    result = subprocess.run(command, env=bus_env, capture_output=True, text=True, timeout=20)
    got = json.loads(received.read_text(encoding="utf-8")) if received.exists() else None
    check(result.returncode == 0 and got is not None,
          f"gdbus call accepté par le serveur (code {result.returncode} {result.stderr.strip()[:200]})")
    check(got is not None and got[0] == "Contrôle parental" and got[2] == "dialog-warning"
          and got[3] == TITLE and got[4] == BODY and got[5] == [] and got[7] == -1,
          "bulle reçue telle quelle : application, icône, titre et texte exacts")
finally:
    for child in (server, daemon):
        if child is not None:
            child.kill()
            child.wait()

# 3. notify() : commande lancée sous l'identité du compte, sur son bus.
launched = []
with mock.patch.object(parental, "ROOT", ""), \
        mock.patch.object(parental, "passwd", return_value={"enfant": (1003, 1003, "", "/home/enfant")}), \
        mock.patch.object(parental, "user_session_env",
                          return_value={"DBUS_SESSION_BUS_ADDRESS": "unix:path=/run/user/1003/bus", "LANG": "fr_FR.UTF-8"}), \
        mock.patch.object(parental.shutil, "which",
                          side_effect=lambda name: None if name == "notify-send" else real_which(name)), \
        mock.patch.object(parental.subprocess, "Popen", side_effect=lambda argv, **kw: launched.append((argv, kw))):
    shown = parental.notify("enfant", ":1", TITLE, BODY)
argv, options = launched[0] if launched else ([], {})
check(shown and argv[:4] == ["setpriv", "--reuid=1003", "--regid=1003", "--init-groups"]
      and os.path.basename(argv[4]) == "gdbus"
      and options.get("env", {}).get("DBUS_SESSION_BUS_ADDRESS") == "unix:path=/run/user/1003/bus",
      "notify() lance gdbus sous l'identité du compte, sur le bus de sa session")
with mock.patch.object(parental, "ROOT", ""), \
        mock.patch.object(parental, "passwd", return_value={"enfant": (1003, 1003, "", "/home/enfant")}), \
        mock.patch.object(parental, "user_session_env", return_value=None):
    check(parental.notify("enfant", ":1", TITLE, BODY) is False,
          "sans session sur cet écran, notify() le dit (le service le note au journal)")

shutil.rmtree(work, ignore_errors=True)
print("test-parental-bulles :", "réussi" if failures == 0 else f"{failures} échec(s)")
sys.exit(1 if failures else 0)
