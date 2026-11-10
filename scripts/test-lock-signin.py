#!/usr/bin/python3
"""Écran de connexion : code PIN et empreinte (#659), sur le vrai écran GTK.

Relais root factice : alice a un code PIN (2580) et un doigt enregistré ;
bob n'a que son mot de passe. Vérifié avec xdotool :
- le champ propose « Code PIN » d'office, et « Options de connexion » ramène
  le mot de passe ;
- un code faux affiche le message, le bon déverrouille ;
- bob sélectionné : champ « Mot de passe », pas d'options ;
- un doigt reconnu déverrouille sans rien taper, même depuis l'horloge ;
- un code bloqué ramène le mot de passe.

À lancer avec xvfb-run (xdotool requis).
"""
import gc
import json
import os
from pathlib import Path
import runpy
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
if not shutil.which("xdotool"):
    print("xdotool manquant", file=sys.stderr)
    sys.exit(1)

tmp = Path(tempfile.mkdtemp(prefix="winlux-lock-signin-"))
os.environ["WINLUX_LOCK_ROOT"] = str(tmp)
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
os.environ["WINLUX_SESSION_USER"] = "alice"
os.environ["WINLUX_LANG"] = "fr"
os.environ["HOME"] = str(tmp / "home/alice")
(tmp / "etc/winlux").mkdir(parents=True)
(tmp / "run/winlux").mkdir(parents=True)
(tmp / "home/alice").mkdir(parents=True)
(tmp / "proc/bus/input").mkdir(parents=True)
(tmp / "proc/bus/input/devices").write_text(
    'I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\nN: Name="AT Translated Set 2 keyboard"\n'
    "H: Handlers=sysrq kbd event0 leds\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n")
(tmp / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000:Alice:/home/alice:/bin/bash\n"
                                "bob:x:1001:1001:Bob Neuf:/home/bob:/bin/bash\n")
(tmp / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\n")
lock = runpy.run_path(str(HERE / "winlux-lock.py"), run_name="lock_signin_gui")

import gi  # noqa: E402
gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

# ── Relais root factice ─────────────────────────────────────────────────────
state = {"finger": threading.Event(), "pin_locked": False, "requests": []}
relay = socket.socket(socket.AF_UNIX)
relay.bind(str(tmp / "run/winlux-root.sock"))
relay.listen(8)


def answer(client, request):
    state["requests"].append(request)
    op, action, user = request.get("op"), request.get("action"), request.get("user")
    if op == "signin" and action == "status":
        alice = user == "alice"
        return {"ok": True, "pin": alice, "pin_locked": alice and state["pin_locked"], "reader": True,
                "fingers": 1 if alice else 0, "password": True}
    if op == "signin" and action == "pin-check":
        if state["pin_locked"]:
            return {"ok": False, "error": "pin-locked", "locked": True}
        return {"ok": True} if request.get("pin") == "2580" else {"ok": False, "error": "pin-wrong"}
    if op == "signin" and action == "fp-verify":
        # Le lecteur attend un doigt ; le client raccroche s'il change d'avis.
        while not state["finger"].wait(0.1):
            try:
                client.setblocking(False)
                if client.recv(1, socket.MSG_PEEK) == b"":
                    return None
            except BlockingIOError:
                pass
            except OSError:
                return None
            finally:
                client.setblocking(True)
        state["finger"].clear()
        return {"ok": True}
    return {"ok": False}


def handle(client):
    with client:
        line = client.makefile("r", encoding="utf-8").readline()
        try:
            result = answer(client, json.loads(line or "{}"))
            if result is not None:
                client.sendall((json.dumps(result) + "\n").encode())
        except OSError:
            pass


def serve():
    while True:
        client, _ = relay.accept()
        threading.Thread(target=handle, args=(client,), daemon=True).start()


threading.Thread(target=serve, daemon=True).start()
failures = []
quits = []
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


def type_text(text):
    subprocess.run(["xdotool", "type", "--delay", "20", text], check=True, timeout=10)
    subprocess.run(["xdotool", "key", "Return"], check=True, timeout=5)
    pump(1.0)


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
        pump(1.0)
        entry = app.password_entry
        check(app.method == "pin" and entry.get_placeholder_text() == "Code PIN",
              "code PIN proposé d'office pour alice")
        check(entry.is_focus() and app.method_hint.get_visible() and app.method_hint.get_text() == "Code PIN",
              "champ au focus : « Code PIN » reste écrit au-dessus (#1499)")
        check(app.options_link.get_visible() and app.finger_label.get_visible()
              and "doigt" in app.finger_label.get_text(), "« Options de connexion » et invite du lecteur affichées")
        type_text("0000")
        check(quits == [] and "code PIN est incorrect" in app.error.get_text(), "mauvais code : message, toujours verrouillé")
        app.options_link.clicked()
        pump(0.3)
        check(app.options_box.get_visible(), "« Options de connexion » montre mot de passe et code PIN")
        if os.environ.get("WINLUX_SIGNIN_SHOTS"):
            subprocess.run(["import", "-window", "root", os.path.join(os.environ["WINLUX_SIGNIN_SHOTS"],
                                                                      "ecran-connexion.png")], check=False)
        app.method_buttons["password"].clicked()
        pump(0.3)
        check(app.method == "password" and entry.get_placeholder_text() == "Mot de passe"
              and not app.method_hint.get_visible(), "retour au mot de passe (indication PIN masquée)")
        app.method_buttons["pin"].clicked()
        pump(0.3)
        # Bob : mot de passe seulement.
        app.select_account("bob")
        pump(1.0)
        check(app.method == "password" and not app.options_link.get_visible() and not app.finger_label.get_visible(),
              "bob (sans code ni doigt) : mot de passe seul")
        check(not app.method_hint.get_visible(), "bob : pas d'indication « Code PIN »")
        app.select_account("alice")
        pump(1.0)
        check(app.method == "pin", "alice resélectionnée : code PIN")
        # Doigt reconnu depuis l'horloge : déverrouillé sans rien taper.
        app.show_clock()
        pump(0.3)
        verifies = [r for r in state["requests"] if r.get("action") == "fp-verify"]
        check(len(verifies) >= 1, "le lecteur attend un doigt dès l'écran verrouillé")
        state["finger"].set()
        pump(1.5)
        check(len(quits) == 1, "doigt reconnu : session déverrouillée depuis l'horloge")
        # Bon code PIN (nouvel essai sur le même écran).
        quits.clear()
        app.show_signin()
        pump(0.5)
        app.set_method("pin")
        entry.grab_focus()
        pump(0.3)
        type_text("2580")
        check(len(quits) == 1, "bon code PIN : déverrouillé")
        # Code bloqué : le mot de passe redevient obligatoire.
        quits.clear()
        state["pin_locked"] = True
        app.set_method("pin")
        entry.grab_focus()
        pump(0.3)
        type_text("2580")
        check(quits == [] and app.method == "password" and "bloqué" in app.error.get_text()
              and not app.options_link.get_visible(), "code bloqué : retour au mot de passe, message affiché")
    except Exception as exc:  # noqa: BLE001
        failures.append(f"exception : {exc}")
    finally:
        app.stop_finger() if app is not None else None
        real_quit()
    return False


def fake_quit():
    quits.append(True)


def fake_main():
    GLib.timeout_add(800, scenario)
    GLib.timeout_add_seconds(60, lambda: (failures.append("délai dépassé"), real_quit()) and False)
    real_main()


Gtk.main, Gtk.main_quit = fake_main, fake_quit
try:
    lock["run_lock"]("signin")
finally:
    Gtk.main, Gtk.main_quit = real_main, real_quit

if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    sys.exit(1)
print("OK : code PIN et empreinte à l'écran de connexion")
