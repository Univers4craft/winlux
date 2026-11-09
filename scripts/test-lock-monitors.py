#!/usr/bin/python3
"""Écran verrouillé et plusieurs écrans (#158).

Un écran branché (ou redéfini par xrandr, ou retrouvé au réveil) pendant le
verrouillage montrait le bureau : l'écran verrouillé ne posait ses fenêtres
qu'au démarrage. Ici, sur un Xvfb de 2732x768, l'affichage vu par GDK est
remplacé par un faux (deux écrans 1366x768, puis un seul) : chaque écran
doit être couvert, la fenêtre principale suit l'écran principal, et le cache
d'un écran débranché disparaît.

À lancer avec xvfb-run -s "-screen 0 2732x768x24".
"""
import gc
import os
from pathlib import Path
import runpy
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
tmp = Path(tempfile.mkdtemp(prefix="winlux-lock-monitors-"))
os.environ["WINLUX_LOCK_ROOT"] = str(tmp)
os.environ["WINLUX_OSK_ROOT"] = str(tmp)
os.environ["WINLUX_SESSION_USER"] = "alice"
os.environ["WINLUX_LANG"] = "fr"
os.environ["HOME"] = str(tmp / "home/alice")
for d in ("etc/winlux", "run/winlux", "home/alice", "proc/bus/input"):
    (tmp / d).mkdir(parents=True)
(tmp / "proc/bus/input/devices").write_text(
    'I: Bus=0011 Vendor=0001 Product=0001 Version=ab41\nN: Name="AT Translated Set 2 keyboard"\n'
    "H: Handlers=sysrq kbd event0 leds\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n")

lock = runpy.run_path(str(HERE / "winlux-lock.py"), run_name="lock_monitors")
plan = lock["cover_plan"]
failures = []

# Plan seul : un écran ajouté est couvert, un écran retiré libéré.
a, b = (0, 0, 1366, 768), (1366, 0, 1366, 768)
if plan([a], [a, b]) != ([b], []):
    failures.append(f"plan d'ajout : {plan([a], [a, b])}")
if plan([a, b], [a]) != ([], [b]):
    failures.append(f"plan de retrait : {plan([a, b], [a])}")

digest = lock["_crypt"]("pa", "$6$winluxmon$")
(tmp / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000:Alice:/home/alice:/bin/bash\n")
(tmp / "etc/shadow").write_text(f"root:!:19000::::::\nalice:{digest}:19000:0:99999:7:::\n")
(tmp / "etc/winlux/users.conf").write_text("alice:shadow\n")

import gi  # noqa: E402
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk  # noqa: E402

real_main, real_quit = Gtk.main, Gtk.main_quit
real_display = Gdk.Display.get_default


class FakeMonitor:
    def __init__(self, geo):
        self.geo = geo

    def get_geometry(self):
        rect = Gdk.Rectangle()
        rect.x, rect.y, rect.width, rect.height = self.geo
        return rect


class FakeDisplay:
    """Affichage vu par cover_monitors ; le reste passe au vrai."""

    def __init__(self, geos):
        self.monitors = [FakeMonitor(g) for g in geos]

    def get_n_monitors(self):
        return len(self.monitors)

    def get_monitor(self, index):
        return self.monitors[index]

    def get_primary_monitor(self):
        return self.monitors[0]

    def __getattr__(self, name):
        return getattr(real_display(), name)


def find_lock():
    for obj in gc.get_objects():
        if type(obj).__name__ == "Lock" and hasattr(obj, "cover_monitors"):
            return obj
    return None


def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.01)


def placed(window):
    gdk = window.get_window()
    origin = gdk.get_origin()[1:]
    return (origin[0], origin[1], gdk.get_width(), gdk.get_height())


def scenario():
    app = find_lock()
    try:
        if app is None:
            failures.append("écran verrouillé introuvable")
            return False
        pump(0.5)
        start = len(app.windows)
        # Deux écrans : le second, branché pendant le verrouillage, est couvert.
        Gdk.Display.get_default = staticmethod(lambda: FakeDisplay([a, b]))
        app.cover_monitors()
        pump(0.6)
        where = sorted(placed(w) for w in app.windows)
        print(f"{start} fenêtre(s) au départ, puis {where}")
        if where != [a, b]:
            failures.append(f"deux écrans : fenêtres en {where} au lieu de {[a, b]}")
        if placed(app.main) != a:
            failures.append(f"fenêtre principale en {placed(app.main)} au lieu de l'écran principal {a}")
        # Écran débranché : son cache part, la saisie reste sur l'écran principal.
        Gdk.Display.get_default = staticmethod(lambda: FakeDisplay([a]))
        app.cover_monitors()
        pump(0.4)
        if len(app.windows) != 1 or app.covers:
            failures.append(f"écran débranché : {len(app.windows)} fenêtre(s), caches {list(app.covers)}")
    except Exception as exc:  # noqa: BLE001
        failures.append(f"exception : {exc!r}")
    finally:
        Gdk.Display.get_default = real_display
        real_quit()
    return False


def fake_main():
    GLib.timeout_add(800, scenario)
    GLib.timeout_add_seconds(30, lambda: (failures.append("délai dépassé"), real_quit()) and False)
    real_main()


Gtk.main = fake_main
try:
    lock["run_lock"]("clock")
finally:
    Gtk.main = real_main

if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    sys.exit(1)
print("OK : écran verrouillé, chaque écran branché ou débranché reste couvert")
