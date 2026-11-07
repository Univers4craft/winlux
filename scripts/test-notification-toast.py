#!/usr/bin/python3
"""Bulle de notification (#624) : en bas à droite au-dessus de la barre des
tâches, jamais sur les boutons d'une fenêtre agrandie ; icône et nom de
l'application, titre et texte alignés à gauche, croix, clic = action default.

    xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session -- python3 -B scripts/test-notification-toast.py
"""
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
problems = []


def check(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition: problems.append(label)


os.environ["WINLUX_LANG"] = "fr"
module = runpy.run_path(str(HERE / "winlux-notifications.py"), run_name="toast_test")
Gtk, Gio, GLib = module["Gtk"], module["Gio"], module["GLib"]


class Rect:
    def __init__(self, x, y, width, height): self.x, self.y, self.width, self.height = x, y, width, height


position = module["toast_position"]
check(position(Rect(0, 0, 1366, 720), Rect(0, 0, 1366, 768), 364, 100) == (1366 - 364 - 12, 720 - 100 - 12),
      "au-dessus de la barre des tâches réservée (zone de travail)")
check(position(Rect(0, 0, 1366, 768), Rect(0, 0, 1366, 768), 364, 100) == (1366 - 364 - 12, 768 - 48 - 100 - 12),
      "barre non réservée : 48 px laissés en bas")
x, y = position(Rect(0, 0, 3840, 2112), Rect(0, 0, 3840, 2160), 364, 120)
check(y > 2160 // 2 and x > 3840 // 2, "4K : coin inférieur droit")
name, icon = module["app_identity"]("Firefox", "", {})
check(name == "Firefox" and icon is not None, "nom envoyé et icône par défaut")
check(module["app_identity"]("", "", {})[0] == "Notification", "sans nom : « Notification »")

if not os.environ.get("DISPLAY") or not os.environ.get("DBUS_SESSION_BUS_ADDRESS") or not shutil.which("xdotool"):
    print("affichage, bus de session ou xdotool absent : bulle réelle non vérifiée")
else:
    tmp = Path(tempfile.mkdtemp(prefix="winlux-toast-"))
    env = dict(os.environ, XDG_STATE_HOME=str(tmp / "state"), XDG_CONFIG_HOME=str(tmp / "config"), WINLUX_LANG="fr",
               WINLUX_SOUNDS_DIR=str(tmp / "sons"))
    service = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-notifications.py")], env=env)
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

        def call(method, params=None):
            return bus.call_sync("org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications",
                                 method, params, None, Gio.DBusCallFlags.NONE, 3000, None)
        for attempt in range(80):
            try:
                call("GetCapabilities"); break
            except GLib.Error:
                time.sleep(0.05)
        invoked = []
        bus.signal_subscribe(None, "org.freedesktop.Notifications", "ActionInvoked", "/org/freedesktop/Notifications", None,
                             Gio.DBusSignalFlags.NONE, lambda *args: invoked.append(args[5].unpack()), )
        identity = call("Notify", GLib.Variant("(susssasa{sv}i)", ("Téléchargements", 0, "folder-download", "Titre du test",
                                                                   "Un texte assez long pour la bulle", ["default", "Ouvrir"], {}, 8000))).unpack()[0]
        window = ""
        for attempt in range(50):
            window = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Titre du test$"],
                                    capture_output=True, text=True).stdout.split()
            if window: break
            time.sleep(0.1)
        check(bool(window), "bulle affichée")
        if window:
            geometry = subprocess.run(["xdotool", "getwindowgeometry", "--shell", window[0]], capture_output=True, text=True).stdout
            values = dict(line.split("=", 1) for line in geometry.split())
            left, top = int(values["X"]), int(values["Y"])
            width, height = int(values["WIDTH"]), int(values["HEIGHT"])
            screen = subprocess.run(["xdotool", "getdisplaygeometry"], capture_output=True, text=True).stdout.split()
            screen_w, screen_h = int(screen[0]), int(screen[1])
            print(f"     bulle {width}x{height} en {left},{top} sur {screen_w}x{screen_h}")
            check(top > 40, "pas sur les boutons d'une fenêtre agrandie (haut de l'écran libre)")
            check(top + height <= screen_h - 48 and top + height >= screen_h - 48 - 40, "en bas, juste au-dessus de la barre des tâches")
            check(left + width <= screen_w and left + width >= screen_w - 40, "à droite")
            # Clic sur la bulle : action « default » transmise, bulle fermée.
            subprocess.run(["xdotool", "mousemove", str(left + width // 2), str(top + height - 15), "click", "1"], check=True)
            context = GLib.MainContext.default()
            deadline = time.time() + 3
            while time.time() < deadline and not invoked:
                context.iteration(False); time.sleep(0.05)
            check(invoked == [(identity, "default")], f"clic : action default ({invoked})")
            time.sleep(0.3)
            left_over = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "^Titre du test$"],
                                       capture_output=True, text=True).stdout.split()
            check(not left_over, "bulle fermée après le clic")
    finally:
        service.terminate()
        service.wait(timeout=5)
        shutil.rmtree(tmp, ignore_errors=True)

# Contenu : en-tête icône + nom, titre et texte alignés à gauche, croix.
if os.environ.get("DISPLAY"):
    service = module["Notifications"].__new__(module["Notifications"])
    service.toast = None
    service.live = {}
    service.show(1, "Titre", "Texte", 0, app="Lecteur", icon="audio-x-generic", hints={})
    labels = []

    def walk(widget):
        if isinstance(widget, Gtk.Label): labels.append(widget)
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): walk(child)
    walk(service.toast)
    check([label.get_text() for label in labels] == ["Lecteur", "Titre", "Texte"], f"textes : {[label.get_text() for label in labels]}")
    check(all(label.get_xalign() == 0 for label in labels), "texte aligné à gauche")
    images, buttons = [], []

    def walk_other(widget):
        if isinstance(widget, Gtk.Image): images.append(widget)
        if isinstance(widget, Gtk.Button): buttons.append(widget)
        if isinstance(widget, Gtk.Container):
            for child in widget.get_children(): walk_other(child)
    walk_other(service.toast)
    check(len(images) >= 2 and buttons and buttons[0].get_tooltip_text() == "Fermer", "icône de l'application et croix")
    buttons[0].clicked()
    check(service.toast is None, "croix : bulle fermée")

if problems:
    print(f"{len(problems)} échec(s)")
    sys.exit(1)
print("OK: bulle de notification en bas à droite (#624)")
