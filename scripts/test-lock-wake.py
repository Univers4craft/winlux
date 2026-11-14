#!/usr/bin/python3
"""Écran verrouillé : la première touche réveille seulement.

Test VM du 3 novembre 2026 : après 10 min d'inactivité, l'écran verrouillé
affiche l'horloge (ou est éteint) ; la première touche partait directement
dans le mot de passe et le testeur y a tapé « par » sans le voir. Ici, avec
xdotool sur le vrai écran (winlux-lock en vue horloge) : « p » affiche la
connexion avec un champ vide, les touches suivantes s'y écrivent.

Vérifie aussi que l'heure, la date et l'invite restent au-dessus du losange
du fond d'écran (1366x768 comme 1920x1080).

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

tmp = Path(tempfile.mkdtemp(prefix="winlux-lock-wake-"))
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

lock = runpy.run_path(str(HERE / "winlux-lock.py"), run_name="lock_wake_gui")
digest = lock["_crypt"]("pa", "$6$winluxwake$")
(tmp / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000:Alice:/home/alice:/bin/bash\n")
(tmp / "etc/shadow").write_text(f"root:!:19000::::::\nalice:{digest}:19000:0:99999:7:::\n")
(tmp / "etc/winlux/users.conf").write_text("alice:shadow\n")

import gi  # noqa: E402
gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk  # noqa: E402

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


def key(name):
    subprocess.run(["xdotool", "key", "--clearmodifiers", name], check=True, timeout=5)
    pump(0.3)


def wallpaper_apex(path):
    """Première ligne (en 1920x1080) où le centre du fond d'écran s'écarte
    nettement du ciel voisin : le sommet du prisme."""
    from gi.repository import GdkPixbuf
    image = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path), 1920, 1080, False)
    pixels, stride, channels = image.get_pixels(), image.get_rowstride(), image.get_n_channels()
    def at(x, y):
        offset = y * stride + x * channels
        return pixels[offset:offset + 3]
    for y in range(150, 540):
        if sum(abs(a - b) for a, b in zip(at(958, y), at(900, y))) > 80:
            return y
    raise AssertionError(f"sommet du prisme introuvable dans {path}")


BUILD = (HERE / "build-live-iso.sh").read_text(encoding="utf-8")
DEFAULT_WALLPAPER = BUILD.split('wallpaper_src="$root/', 1)[1].split('"', 1)[0]
APEX_1080 = wallpaper_apex(HERE.parent / DEFAULT_WALLPAPER)
print(f"fond par défaut {DEFAULT_WALLPAPER} : sommet du prisme à {APEX_1080}/1080")


def scenario():
    app = find_lock()
    try:
        if app is None:
            failures.append("écran verrouillé introuvable")
            return False
        pump(0.5)
        window = app.main.get_window()
        x, y = window.get_origin()[1:]
        subprocess.run(["xdotool", "mousemove", str(x + 50), str(y + 50)], check=True)
        pump(0.2)
        if app.view != "clock":
            failures.append(f"vue de départ {app.view!r} au lieu de l'horloge")
        # Le prisme du fond d'écran par défaut (mis à l'échelle « cover ») :
        # l'horloge doit finir au-dessus. En 1366x768 il recouvrait la date et
        # l'invite (VM, 3 novembre) ; en 1920x1080 l'invite (#2160, fond
        # Aurore). Sommet mesuré dans l'image elle-même, pas recopié du code.
        width, height = window.get_width(), window.get_height()
        cover = max(width / 1920, height / 1080)
        logo_top = (height - 1080 * cover) / 2 + APEX_1080 * cover
        hint = app.clock.get_children()[-1]
        bottom = hint.translate_coordinates(hint.get_toplevel(), 0, 0)[1] + hint.get_allocation().height
        print(f"écran {width}x{height} : horloge jusqu'à {bottom}, losange dès {logo_top:.0f}")
        if bottom > logo_top - 8:
            failures.append(f"horloge et invite (jusqu'à {bottom} px) sur le losange du fond (dès {logo_top:.0f} px)")
        key("p")
        if app.view != "signin":
            failures.append("la première touche n'affiche pas la connexion")
        if app.password_entry.get_text():
            failures.append(f"première touche tapée dans le mot de passe : {app.password_entry.get_text()!r}")
        key("p")
        key("a")
        if app.password_entry.get_text() != "pa":
            failures.append(f"saisie après réveil : {app.password_entry.get_text()!r} au lieu de 'pa'")
    except Exception as exc:  # noqa: BLE001
        failures.append(f"exception : {exc}")
    finally:
        real_quit()
    return False


def fake_main():
    GLib.timeout_add(800, scenario)
    GLib.timeout_add_seconds(30, lambda: (failures.append("délai dépassé"), real_quit()) and False)
    real_main()


Gtk.main, Gtk.main_quit = fake_main, real_quit
try:
    lock["run_lock"]("clock")
finally:
    Gtk.main = real_main

if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    sys.exit(1)
print("OK : écran verrouillé, la première touche réveille sans rien taper")
