#!/usr/bin/python3
"""Page Licence de l'installateur : « Suivant » sans avoir accepté ne reste pas muet.

À lancer avec xvfb-run (1366x768). Sans la case cochée, « Suivant » reste
cliquable ; un clic garde la page Licence, entoure la case de l'accent Prisme
et affiche sous elle une phrase dans la langue de l'installateur (6 langues).
Cocher la case efface le message et « Suivant » avance. Avec le clavier tactile
ouvert, la case et le message restent au-dessus du clavier.
WINLUX_CAPTURE=fichier.png enregistre une capture de la page avec le message.
"""
import importlib.util
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
# Pas de /proc/bus/input dans cette racine : aucun clavier physique, comme
# sur une tablette.
os.environ["WINLUX_OSK_ROOT"] = tempfile.mkdtemp(prefix="winlux-licence-root-")
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


def pump(seconds=0.0):
    end = GLib.get_monotonic_time() + int(seconds * 1e6)
    while True:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        if GLib.get_monotonic_time() >= end:
            return
        GLib.usleep(20000)


def check():
    try:
        tr = installer.tr
        for lang in ("fr_FR", "en_US", "de_DE", "es_ES", "it_IT", "pt_BR"):
            text = installer.TRANSLATIONS[lang].get("license_required")
            expect(text and text != installer.TRANSLATIONS["fr_FR"]["license_required"] or lang == "fr_FR",
                   f"message de licence non traduit : {lang}")
        app.current_step = installer.step_index("license")
        app.update_step()
        pump(0.5)
        ctx = app.license_accept_box.get_style_context()
        expect(app.btn_next.get_sensitive(), "« Suivant » grisé : un clic ne montrerait rien")
        expect(not app.license_hint.get_visible(), "message affiché avant tout clic")
        expect(not ctx.has_class("license-attention"), "case déjà mise en évidence")

        app.btn_next.clicked()
        pump(0.1)
        expect(installer.STEP_KEYS[app.current_step] == "license", "« Suivant » a passé la licence sans acceptation")
        expect(app.license_hint.get_visible(), "pas de message sous la case")
        expect(app.license_hint.get_label() == tr("license_required"), "message inattendu")
        pump(1.2)  # fin du clignotement : la mise en évidence reste
        expect(ctx.has_class("license-attention"), "la case n'est plus mise en évidence après le clignotement")
        expect(app.license_accept.has_focus(), "le focus n'est pas sur la case")

        # Tablette : la Licence n'a aucun champ, le clavier tactile n'y est pas
        # ouvert d'office (#374) ; ouvert à la main, la case et le message
        # restent visibles au-dessus.
        expect(not app.osk_box.get_visible(), "clavier tactile ouvert d'office sur la Licence (aucun champ)")
        app.show_touch_keyboard(True)
        pump(0.3)
        expect(app.osk_box.get_visible(), "le clavier tactile ne s'ouvre pas à la main")
        # Coordonnées dans la fenêtre : la case et le message tiennent dans la partie
        # visible de la page qui défile, donc au-dessus du clavier et des boutons.
        page = app.license_accept_box.get_ancestor(Gtk.ScrolledWindow)
        box = app.license_accept_box.translate_coordinates(app.win, 0, 0)
        hint = app.license_hint.translate_coordinates(app.win, 0, app.license_hint.get_allocated_height())
        view = page.translate_coordinates(app.win, 0, 0)
        view_bottom = view[1] + page.get_allocated_height()
        osk = app.osk_box.translate_coordinates(app.win, 0, 0)
        expect(box[1] >= view[1], f"case au-dessus de la partie visible : {box} / {view}")
        expect(hint[1] <= view_bottom, f"message sous la partie visible : {hint} / {view_bottom}")
        expect(hint[1] <= osk[1], f"message caché par le clavier tactile : {hint} / {osk}")
        width, height = app.win.get_size()
        expect(height <= 768 and width <= 1366, f"fenêtre plus grande que l'écran : {width}x{height}")

        capture = os.environ.get("WINLUX_CAPTURE")
        if capture:
            from gi.repository import Gdk
            win = app.win.get_window()
            Gdk.pixbuf_get_from_window(win, 0, 0, win.get_width(), win.get_height()).savev(capture, "png", [], [])

        app.license_accept.set_active(True)
        pump(0.1)
        expect(not app.license_hint.get_visible(), "message resté après acceptation")
        expect(not ctx.has_class("license-attention"), "mise en évidence restée après acceptation")
        app.btn_next.clicked()
        pump(0.1)
        expect(installer.STEP_KEYS[app.current_step] != "license", "« Suivant » n'avance pas une fois la licence acceptée")

        # Retour arrière puis décocher : on bloque de nouveau, message en allemand.
        app.current_step = installer.step_index("license")
        app.update_step()
        app.license_accept.set_active(False)
        installer.tr._lang = "de_DE"
        app.btn_next.clicked()
        pump(0.1)
        expect(installer.STEP_KEYS[app.current_step] == "license", "licence décochée mais « Suivant » avance")
        expect(app.license_hint.get_label() == installer.TRANSLATIONS["de_DE"]["license_required"],
               "message pas dans la langue de l'installateur")
        installer.tr._lang = "fr_FR"
    except Exception as exc:  # noqa: BLE001
        import traceback
        traceback.print_exc()
        failures.append(f"exception : {exc}")
    finally:
        app.quit()


def startup(_app):
    GLib.timeout_add(600, lambda: check() or False)


app.connect("activate", startup)
app.run([])
if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    sys.exit(1)
print("OK : licence non acceptée signalée sous la case (6 langues, au-dessus du clavier tactile)")
