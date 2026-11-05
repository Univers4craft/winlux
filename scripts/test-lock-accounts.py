#!/usr/bin/python3
"""Écran verrouillé : le clic qui réveille l'écran sélectionne le compte visé.

Test VM du 5 novembre 2026 : après un mot de passe refusé pour un autre
compte, 30 s sans saisie ramenaient l'horloge et le propriétaire de la
session ; le clic suivant sur l'autre compte, en bas à gauche, ne faisait
qu'afficher la connexion, et le mot de passe tapé partait sur le mauvais
compte. Ici, avec xdotool sur le vrai écran : chaque clic sur un compte le
sélectionne, un clic ailleurs garde le propriétaire.

À lancer avec xvfb-run (xdotool requis).
"""
import gc
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
if not shutil.which("xdotool"):
    print("xdotool manquant", file=sys.stderr)
    sys.exit(1)

tmp = Path(tempfile.mkdtemp(prefix="winlux-lock-accounts-"))
os.environ["WINLUX_LOCK_ROOT"] = str(tmp)
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
os.environ["WINLUX_SESSION_USER"] = "alice"
os.environ["WINLUX_LANG"] = "fr"
os.environ["HOME"] = str(tmp / "home/alice")
(tmp / "etc/winlux").mkdir(parents=True)
(tmp / "run/winlux").mkdir(parents=True)
(tmp / "home/alice").mkdir(parents=True)
# Un clavier physique : pas de clavier visuel, on tape au vrai clavier.
(tmp / "proc/bus/input").mkdir(parents=True)
(tmp / "proc/bus/input/devices").write_text(
    'I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\nN: Name="AT Translated Set 2 keyboard"\n'
    "H: Handlers=sysrq kbd event0 leds\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n")

lock = runpy.run_path(str(HERE / "winlux-lock.py"), run_name="lock_accounts_gui")
(tmp / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000:Alice:/home/alice:/bin/bash\n"
                                "bob:x:1001:1001:Bob Neuf:/home/bob:/bin/bash\n")
# Système installé : /etc/shadow illisible par le bureau, le relais root vérifie.
(tmp / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\n")

import gi  # noqa: E402
import socket  # noqa: E402
import threading  # noqa: E402
gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

# Relais root factice : refuse tout mot de passe, après un temps de vérification.
relay = socket.socket(socket.AF_UNIX)
relay.bind(str(tmp / "run/winlux-root.sock"))
relay.listen(5)


def serve():
    while True:
        client, _ = relay.accept()
        client.makefile().readline()
        time.sleep(0.5)
        client.sendall(b'{"ok": false}\n')
        client.close()


threading.Thread(target=serve, daemon=True).start()
failures = []
real_main, real_quit = Gtk.main, Gtk.main_quit


def find_lock():
    for obj in gc.get_objects():
        if type(obj).__name__ == "Lock" and hasattr(obj, "password_entry"):
            return obj
    return None


def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.01)


def click_at(x, y):
    subprocess.run(["xdotool", "mousemove", str(x), str(y), "click", "1"], check=True, timeout=5)
    pump(0.5)


def centre(widget):
    origin = widget.translate_coordinates(widget.get_toplevel(), 0, 0)
    left, top = widget.get_toplevel().get_window().get_origin()[1:]
    box = widget.get_allocation()
    return left + origin[0] + box.width // 2, top + origin[1] + box.height // 2


def check(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


def scenario():
    app = find_lock()
    try:
        if app is None:
            failures.append("écran verrouillé introuvable")
            return False
        pump(0.5)
        buttons = app.accounts.get_children()
        bob = centre(buttons[1])
        click_at(*bob)
        check(app.user == "bob", "un clic sur « Bob Neuf » le sélectionne")
        subprocess.run(["xdotool", "type", "faux"], check=True, timeout=5)
        subprocess.run(["xdotool", "key", "Return"], check=True, timeout=5)
        pump(1.5)
        check(app.user == "bob" and "incorrect" in app.error.get_text(),
              "mot de passe refusé : Bob reste sélectionné, message affiché")
        # 30 s sans saisie : retour à l'horloge, propriétaire sélectionné.
        app.last_typing -= 31
        pump(1.5)
        check(app.view == "clock" and app.user == "alice", "inactivité : horloge, propriétaire sélectionné")
        click_at(*bob)
        check(app.view == "signin", "le clic réveille l'écran")
        check(app.user == "bob", "ce même clic sélectionne « Bob Neuf » (il était ignoré)")
        # Clic ailleurs depuis l'horloge : le propriétaire reste sélectionné.
        app.last_typing -= 31
        pump(1.5)
        width = app.main.get_window().get_width()
        left, top = app.main.get_window().get_origin()[1:]
        click_at(left + width // 2, top + 60)
        check(app.view == "signin" and app.user == "alice", "clic hors des comptes : propriétaire gardé")
    except Exception as exc:  # noqa: BLE001
        failures.append(f"exception : {exc}")
    finally:
        real_quit()
    return False


def fake_main():
    GLib.timeout_add(800, scenario)
    GLib.timeout_add_seconds(40, lambda: (failures.append("délai dépassé"), real_quit()) and False)
    real_main()


Gtk.main, Gtk.main_quit = fake_main, real_quit
try:
    lock["run_lock"]("signin")
finally:
    Gtk.main = real_main

if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    sys.exit(1)
print("OK : écran verrouillé, le clic qui réveille sélectionne le compte visé")
