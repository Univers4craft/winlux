#!/usr/bin/python3
"""Clavier tactile de l'installateur (tablette sans clavier physique).

À lancer avec xvfb-run. Sans clavier physique (WINLUX_OSK_ROOT sans /proc), le
clavier visuel s'ouvre seul en bas de la fenêtre ; le bouton « Clavier tactile »
l'ouvre et le ferme ; ses touches écrivent dans le champ qui a le focus, dans la
disposition choisie et la langue de l'installateur. Avec un clavier, il reste
fermé mais le bouton est là. WINLUX_CAPTURE=fichier.png enregistre une capture.
"""
import importlib.util
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
root = tempfile.mkdtemp(prefix="winlux-osk-root-")
with_keyboard = "--clavier" in sys.argv
if with_keyboard:
    os.makedirs(os.path.join(root, "proc/bus/input"))
    with open(os.path.join(root, "proc/bus/input/devices"), "w") as f:
        f.write('I: Bus=0011\nN: Name="AT Translated Set 2 keyboard"\nH: Handlers=sysrq kbd event0\n'
                'B: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
os.environ["WINLUX_OSK_ROOT"] = root
spec = importlib.util.spec_from_file_location("installer", os.path.join(HERE, "winlux-installer.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
installer.detect_disks = lambda: []

from gi.repository import GLib, Gtk  # noqa: E402

app = installer.InstallerApp()
failures = []


def expect(cond, message):
    if not cond:
        failures.append(message)


def check():
    try:
        expect(app.osk is not None, "module winlux-osk introuvable")
        expect(app.btn_osk.get_visible(), "bouton « Clavier tactile » absent")
        if with_keyboard:
            expect(not app.osk_box.get_visible(), "clavier ouvert alors qu'un clavier physique est branché")
            app.btn_osk.clicked()
            expect(app.osk_box.get_visible(), "le bouton n'ouvre pas le clavier")
            app.btn_osk.clicked()
            expect(not app.osk_box.get_visible(), "le bouton ne ferme pas le clavier")
            return
        expect(app.osk_box.get_visible(), "clavier non ouvert sans clavier physique")
        entry = Gtk.Entry()
        app.nav_box.pack_start(entry, False, False, 0)
        entry.show()
        entry.grab_focus()
        if app.win.get_focus() is entry:
            for key in ("a", "é", "c", "BackSpace", "Left", "Z"):
                app.touch_key(key)
            expect(entry.get_text() == "aZé", f"saisie : {entry.get_text()!r} au lieu de 'aZé'")
            activated = []
            entry.connect("activate", lambda _e: activated.append(1))
            app.touch_key("Return")
            expect(activated, "Entrée n'active pas le champ")
        else:
            failures.append("le champ de test n'a pas le focus")
        # Disposition et langue suivent les choix de l'installateur.
        app.config["keyboard"] = "de"
        installer.tr._lang = "de_DE"
        app.show_touch_keyboard(True)
        expect(app.osk_layout == ("de", "de"), f"disposition/langue : {app.osk_layout}")
        expect(installer.tr("btn_osk") == "Bildschirmtastatur", "traduction allemande")
        for lang, text in (("fr_FR", "Clavier tactile"), ("en_US", "Touch keyboard"), ("es_ES", "Teclado táctil")):
            expect(installer.TRANSLATIONS[lang].get("btn_osk") == text, f"traduction {lang}")
        installer.tr._lang = "fr_FR"
        app.config["keyboard"] = "fr"
        app.show_touch_keyboard(True)
        capture = os.environ.get("WINLUX_CAPTURE")
        if capture:
            def shot():
                win = app.win.get_window()
                from gi.repository import Gdk
                pix = Gdk.pixbuf_get_from_window(win, 0, 0, win.get_width(), win.get_height())
                pix.savev(capture, "png", [], [])
                app.quit()
                return False
            GLib.timeout_add(800, shot)
            return
        app.osk_box.get_children()  # widget présent
        app.show_touch_keyboard(False)
        expect(not app.osk_box.get_visible(), "le clavier ne se ferme pas")
    except Exception as exc:  # noqa: BLE001
        import traceback
        traceback.print_exc()
        failures.append(f"exception : {exc}")
    finally:
        if not os.environ.get("WINLUX_CAPTURE") or with_keyboard:
            app.quit()


def startup(_app):
    GLib.timeout_add(600, lambda: check() or False)


app.connect("activate", startup)
app.run([])
if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    sys.exit(1)
print("OK : clavier tactile de l'installateur" + (" (clavier branché)" if with_keyboard else ""))
