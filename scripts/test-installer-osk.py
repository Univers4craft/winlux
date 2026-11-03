#!/usr/bin/python3
"""Clavier tactile de l'installateur (tablette sans clavier physique).

À lancer avec xvfb-run (xdotool requis). Les touches sont tapées au pointeur,
comme sur une tablette (test VM v0.2.16 : rien ne s'écrivait, Tab ne changeait
pas de champ, frappes rapides perdues, page « Compte local » rognée).

Sans clavier physique (WINLUX_OSK_ROOT sans /proc) :
- Licence : clavier fermé, la case « J'accepte » reste visible ;
- Compte local : clavier ouvert d'office ; un clic dans un champ puis des
  touches tapées toutes les 0,3 s l'écrivent en entier ; Tab passe au champ
  suivant, qui défile au-dessus du clavier ; le bouton « Clavier tactile »
  ferme et rouvre le clavier sans voler le focus du champ ;
- disposition et langue suivent les choix de l'installateur.
Avec un clavier (--clavier) : jamais ouvert d'office, le bouton l'ouvre.
WINLUX_CAPTURE=fichier.png enregistre une capture de « Compte local ».
"""
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
if not shutil.which("xdotool"):
    print("xdotool manquant", file=sys.stderr)
    sys.exit(1)
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

from gi.repository import Gdk, GLib, Gtk  # noqa: E402

app = installer.InstallerApp()
failures = []


def expect(cond, message):
    if not cond:
        failures.append(message)


def pump(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.01)


def screen_rect(widget):
    top = widget.get_toplevel()
    ox, oy = top.get_window().get_origin()[1:]
    x, y = widget.translate_coordinates(top, 0, 0)
    alloc = widget.get_allocation()
    return ox + x, oy + y, alloc.width, alloc.height


def tap(widget, wait=0.25):
    x, y, w, h = screen_rect(widget)
    subprocess.run(["xdotool", "mousemove", str(x + w // 2), str(y + h // 2), "click", "1"], check=True)
    pump(wait)


def key(label):
    stack = [app.osk_widget]
    while stack:
        widget = stack.pop()
        if isinstance(widget, Gtk.Button) and widget.get_label() == label:
            return widget
        if isinstance(widget, Gtk.Container):
            stack.extend(widget.get_children())
    raise AssertionError(f"touche {label} introuvable")


def go(step):
    app.current_step = installer.step_index(step)
    app.update_step()
    pump(0.6)


def shown_in_page(widget):
    """Le widget est dans la partie visible de la page (pas sous le clavier)."""
    scroll = app.pages.get_visible_child()
    _sx, sy, _sw, sh = screen_rect(scroll)
    _x, y, _w, h = screen_rect(widget)
    return widget.get_mapped() and y >= sy and y + h <= sy + sh


def check():
    try:
        expect(app.osk is not None, "module winlux-osk introuvable")
        expect(app.btn_osk.get_visible(), "bouton « Clavier tactile » absent")
        go("license")
        if with_keyboard:
            go("account")
            expect(not app.osk_box.get_visible(), "clavier ouvert alors qu'un clavier physique est branché")
            tap(app.btn_osk)
            expect(app.osk_box.get_visible(), "le bouton n'ouvre pas le clavier")
            tap(app.btn_osk)
            expect(not app.osk_box.get_visible(), "le bouton ne ferme pas le clavier")
            return
        expect(not app.osk_box.get_visible(), "Licence : clavier ouvert sur une page sans champ")
        go("account")
        expect(app.osk_box.get_visible(), "Compte local : clavier non ouvert sans clavier physique")
        page = app.pages.get_visible_child().get_allocation().height
        screen_height = app.win.get_screen().get_height()
        expect(page >= min(250, int(screen_height * 0.3)), f"Compte local : page rognée à {page} px")
        entries = app.account_entries
        full = entries["fullname"] if "fullname" in entries else list(entries.values())[1]
        tap(full)
        expect(app.win.get_focus() is full, "le clic ne donne pas le focus au champ")
        # Fermer puis rouvrir par le bouton : le champ garde le focus.
        tap(app.btn_osk)
        tap(app.btn_osk)
        expect(app.osk_box.get_visible() and app.win.get_focus() is full, "le bouton vole le focus du champ")
        # Frappes rapides, une toutes les 0,3 s.
        for letter in "azerty":
            tap(key(letter), wait=0.3)
        pump(0.6)
        expect(full.get_text() == "azerty", f"frappes rapides : {full.get_text()!r} au lieu de 'azerty'")
        expect(app.osk_box.get_visible(), "le clavier s'est fermé pendant la saisie")
        # Tab jusqu'au mot de passe : chaque champ défile au-dessus du clavier.
        password = entries["password"]
        for _ in range(6):
            if app.win.get_focus() is password:
                break
            tap(key("Tab"), wait=0.4)
        expect(app.win.get_focus() is password, "Tab n'atteint pas le champ « Mot de passe »")
        pump(0.4)
        expect(shown_in_page(password), "le champ « Mot de passe » reste caché sous le clavier")
        for letter in "secret":
            tap(key(letter), wait=0.3)
        expect(password.get_text() == "secret", f"mot de passe : {password.get_text()!r}")
        tap(key("Tab"), wait=0.4)
        confirm = entries["password_confirm"]
        expect(app.win.get_focus() is confirm and shown_in_page(confirm), "« Confirmer » inaccessible")
        capture = os.environ.get("WINLUX_CAPTURE")
        if capture:
            window = app.win.get_window()
            Gdk.pixbuf_get_from_window(window, 0, 0, window.get_width(), window.get_height()).savev(
                capture, "png", [], [])
        # Disposition et langue suivent les choix de l'installateur.
        app.config["keyboard"] = "de"
        installer.tr._lang = "de_DE"
        app.show_touch_keyboard(True)
        expect(app.osk_layout == ("de", "de"), f"disposition/langue : {app.osk_layout}")
        for lang, text in (("fr_FR", "Clavier tactile"), ("en_US", "Touch keyboard"),
                           ("de_DE", "Bildschirmtastatur"), ("es_ES", "Teclado táctil")):
            expect(installer.TRANSLATIONS[lang].get("btn_osk") == text, f"traduction {lang}")
        installer.tr._lang = "fr_FR"
        go("license")
        expect(not app.osk_box.get_visible(), "retour à Licence : le clavier reste ouvert")
    except Exception as exc:  # noqa: BLE001
        import traceback
        traceback.print_exc()
        failures.append(f"exception : {exc}")
    finally:
        app.quit()


def startup(_app):
    GLib.timeout_add(800, lambda: check() or False)


app.connect("activate", startup)
app.run([])
if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    sys.exit(1)
print("OK : clavier tactile de l'installateur" + (" (clavier branché)" if with_keyboard else ""))
