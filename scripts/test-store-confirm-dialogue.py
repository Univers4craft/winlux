#!/usr/bin/python3
"""Boutique, « Installer … ? » : « Annuler » est le bouton actif visible (#1448).

Vraie fenêtre GTK sous Xvfb : à l'ouverture de la confirmation de
téléchargement, « Annuler » a le focus et le défaut, porte seul la couleur
d'accent, le cadre de focus est visible, et « Installer » n'a ni focus ni
défaut. Dialog.run est remplacé par une sonde qui observe la fenêtre affichée
puis répond « Annuler » : le script doit alors rendre 1 (refus).
"""
import importlib.util
from pathlib import Path
import sys

import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("winlux_store_confirm", HERE / "winlux-store-confirm.py")
confirm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(confirm)

failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


seen = {}


def probe(dialog):
    dialog.show_all()
    while Gtk.events_pending():
        Gtk.main_iteration()
    cancel = dialog.get_widget_for_response(Gtk.ResponseType.CANCEL)
    install = dialog.get_widget_for_response(Gtk.ResponseType.OK)
    seen.update(
        cancel=cancel.get_label(), install=install.get_label(),
        focus=dialog.get_focus() is cancel, focus_visible=dialog.get_focus_visible(),
        default=dialog.get_default_widget() is cancel,
        cancel_accent=cancel.get_style_context().has_class("suggested-action"),
        install_accent=install.get_style_context().has_class("suggested-action"),
        install_focus=install.has_focus(), install_default=install.has_default(),
    )
    return Gtk.ResponseType.CANCEL


Gtk.Dialog.run = probe
sys.argv = ["winlux-store-confirm", "--title", "Installer Éditeur ?",
            "--text", "Éditeur 8.6, Éditeur libre, https://exemple.invalid, SHA-256 0123",
            "--ok", "Installer", "--cancel", "Annuler"]
check(confirm.main() == 1, "« Annuler » n'a pas rendu le refus (1)")
check(bool(seen), "la confirmation ne s'est pas ouverte")
if seen:
    check(seen["focus"], f"« {seen['cancel']} » n'a pas le focus à l'ouverture")
    check(seen["focus_visible"], "le cadre de focus n'est pas visible à l'ouverture")
    check(seen["default"], f"« {seen['cancel']} » n'est pas le bouton par défaut")
    check(seen["cancel_accent"], f"« {seen['cancel']} » ne porte pas la couleur d'accent")
    check(not seen["install_accent"], f"« {seen['install']} » porte la couleur d'accent")
    check(not seen["install_focus"] and not seen["install_default"],
          f"« {seen['install']} » a le focus ou le défaut")

if failures:
    sys.exit(1)
print("Boutique : « Annuler » est le bouton actif visible de la confirmation (#1448)")
