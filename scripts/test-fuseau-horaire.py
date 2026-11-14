#!/usr/bin/python3
"""Paramètres › Heure et langue : fuseau horaire tapé à la main (#2249).

Un nom qui n'est pas un fuseau connu est signalé et « Appliquer » reste
grisé ; un fuseau de la liste, différent de l'actuel, s'applique.
"""
import os
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
os.environ.setdefault("WINLUX_LANG", "fr")
failures = []


def check(condition, message):
    print(("OK   " if condition else "ÉCHEC ") + message, flush=True)
    if not condition:
        failures.append(message)


settings = runpy.run_path(str(HERE / "winlux-settings.py"), run_name="fuseau_test")
Gtk, GLib = settings["Gtk"], settings["GLib"]
window = settings["Settings"]()
calls = []
window.execute = lambda action, *args: calls.append((action, *args))


def walk(widget):
    yield widget
    if isinstance(widget, Gtk.Container):
        for child in widget.get_children():
            yield from walk(child)


def named(name):
    return next((w for w in walk(window.body) if w.get_name() == name), None)


def scenario():
    try:
        window.open_page(14)
        apply, unknown = named("timezone-apply"), named("timezone-unknown")
        check(apply is not None and unknown is not None, "page Heure et langue : bouton Appliquer et message prévus")
        entry = apply.get_parent().get_children()[0].get_child()
        current = entry.get_text()
        check(not apply.get_sensitive() and not unknown.get_visible(), "fuseau actuel : rien à appliquer, aucun message")
        entry.set_text("Europe/Atlantis")
        check(not apply.get_sensitive() and unknown.get_visible() and "inconnu" in unknown.get_text(),
              "fuseau inconnu tapé : « Appliquer » grisé et message affiché (#2249)")
        apply.clicked()
        check(not any(c[0].startswith("tz-") for c in calls), "fuseau inconnu : aucune action lancée")
        other = "America/New_York" if current != "America/New_York" else "Europe/Paris"
        entry.set_text(other)
        check(apply.get_sensitive() and not unknown.get_visible(), "fuseau de la liste : « Appliquer » actif, message retiré")
        apply.clicked()
        check(("tz-" + other,) in calls, "fuseau de la liste appliqué (tz-…)")
        message = settings["EXTRA"]._TR.get("Fuseau horaire inconnu : choisissez-le dans la liste.")
        check(message is not None and len(message) == 3 and all(message),
              "message en anglais, espagnol et allemand")
    finally:
        window.destroy()
        Gtk.main_quit()
    return False


GLib.idle_add(scenario)
Gtk.main()
print("test-fuseau-horaire :", "réussi" if not failures else f"{len(failures)} échec(s)")
sys.exit(1 if failures else 0)
