#!/usr/bin/python3
"""Écran de connexion sans clavier physique : on se connecte au clavier visuel.

À lancer avec xvfb-run (xdotool requis). Lance le vrai écran (winlux-lock
« switch »), clique sur les touches du clavier visuel avec le pointeur comme
sur une tablette, puis sur ↵ : le champ doit se remplir et la session se
déverrouiller. Vérifie aussi que les boutons des autres comptes et le bouton
du clavier ne recouvrent pas les touches. Régression : chaque touche vidait le
champ (test VM du 2 novembre 2026, tablette de damien).
WINLUX_CAPTURE=fichier.png enregistre une capture.
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

tmp = Path(tempfile.mkdtemp(prefix="winlux-lock-osk-"))
os.environ["WINLUX_LOCK_ROOT"] = str(tmp)
os.environ["WINLUX_OSK_ROOT"] = str(tmp)  # aucun /proc/bus/input/devices : pas de clavier
os.environ["WINLUX_SESSION_USER"] = "alice"
os.environ["WINLUX_LANG"] = "fr"
os.environ["HOME"] = str(tmp / "home/alice")
(tmp / "etc/winlux").mkdir(parents=True)
(tmp / "run/winlux").mkdir(parents=True)
(tmp / "home/alice").mkdir(parents=True)
(tmp / "etc/winlux/input.conf").write_text("XKB_LAYOUT=fr\n")

lock = runpy.run_path(str(HERE / "winlux-lock.py"), run_name="lock_osk_gui")
PASSWORD = "azé"
digest = lock["_crypt"](PASSWORD, "$6$winluxosk$")
(tmp / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/bash\n"
                                "alice:x:1000:1000:Alice Martin:/home/alice:/bin/bash\n"
                                "bob:x:1001:1001:Bob:/home/bob:/bin/bash\n")
(tmp / "etc/shadow").write_text(f"root:!:19000::::::\nalice:{digest}:19000:0:99999:7:::\nbob:{digest}:19000::::::\n")
(tmp / "etc/winlux/users.conf").write_text("alice:shadow\nbob:shadow\n")

import gi  # noqa: E402
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402

failures = []
result = {"unlocked": False}
real_main, real_quit = Gtk.main, Gtk.main_quit


def find_lock():
    for obj in gc.get_objects():
        if type(obj).__name__ == "Lock" and hasattr(obj, "password_entry"):
            return obj
    return None


def screen_rect(widget):
    window = widget.get_toplevel().get_window()
    ox, oy = window.get_origin()[1:]
    x, y = widget.translate_coordinates(widget.get_toplevel(), 0, 0)
    alloc = widget.get_allocation()
    return ox + x, oy + y, alloc.width, alloc.height


def key_button(osk, label):
    stack = [osk]
    while stack:
        widget = stack.pop()
        if isinstance(widget, Gtk.Button) and widget.get_label() == label:
            return widget
        if isinstance(widget, Gtk.Container):
            stack.extend(widget.get_children())
    return None


def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.01)


def tap(widget):
    x, y, w, h = screen_rect(widget)
    subprocess.run(["xdotool", "mousemove", str(x + w // 2), str(y + h // 2), "click", "1"], check=True)
    pump(0.25)


def overlaps(a, b):
    ax, ay, aw, ah = a
    bx, by, bw, bh = b
    return ax < bx + bw and bx < ax + aw and ay < by + bh and by < ay + ah


def scenario():
    app = find_lock()
    if app is None:
        failures.append("écran de connexion introuvable")
        real_quit()
        return False
    try:
        pump(0.5)
        if app.osk is None:
            failures.append("clavier visuel absent")
            return False
        if not app.osk.get_visible():
            failures.append("clavier visuel non ouvert d'office sans clavier physique")
        if not app.osk_button.get_visible():
            failures.append("bouton du clavier invisible")
        keys_rect = screen_rect(app.osk)
        if overlaps(screen_rect(app.osk_button), keys_rect):
            failures.append("le bouton du clavier est caché sous les touches")
        if not app.accounts.get_visible():
            failures.append("autres comptes absents")
        elif overlaps(screen_rect(app.accounts), keys_rect):
            failures.append("les boutons des autres comptes recouvrent les touches")
        entry_rect = screen_rect(app.password_entry)
        if overlaps(entry_rect, keys_rect):
            failures.append("le champ du mot de passe est sous le clavier")
        tap(app.password_entry)
        for label in ("a", "z", "e"):
            button = key_button(app.osk, label)
            if button is None:
                failures.append(f"touche {label} introuvable")
                return False
            tap(button)
        if app.password_entry.get_text() != "aze":
            failures.append(f"saisie au clavier visuel : {app.password_entry.get_text()!r} au lieu de 'aze'")
        tap(key_button(app.osk, "⌫"))
        # Accent par appui long sur « e » : on tape « é » directement.
        app.osk_send("é", [])
        if app.password_entry.get_text() != PASSWORD:
            failures.append(f"correction : {app.password_entry.get_text()!r} au lieu de {PASSWORD!r}")
        capture = os.environ.get("WINLUX_CAPTURE")
        if capture:
            window = app.main.get_window()
            Gdk.pixbuf_get_from_window(window, 0, 0, window.get_width(), window.get_height()).savev(
                capture, "png", [], [])
        tap(key_button(app.osk, "↵"))
        pump(0.5)
        if not result["unlocked"]:
            failures.append("↵ du clavier visuel n'a pas ouvert la session")
    except Exception as exc:  # noqa: BLE001
        import traceback
        traceback.print_exc()
        failures.append(f"exception : {exc}")
    finally:
        if not result["unlocked"]:
            real_quit()
    return False


def fake_main():
    GLib.timeout_add(800, scenario)
    GLib.timeout_add_seconds(40, lambda: (failures.append("délai dépassé"), real_quit()) and False)
    real_main()


def fake_quit():
    result["unlocked"] = True
    real_quit()


Gtk.main, Gtk.main_quit = fake_main, fake_quit
try:
    lock["run_lock"]("signin")
finally:
    Gtk.main, Gtk.main_quit = real_main, real_quit

if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    sys.exit(1)
print("OK : connexion au clavier visuel sans clavier physique")
