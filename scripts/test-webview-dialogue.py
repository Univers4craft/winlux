#!/usr/bin/python3
"""Dialogue « Affichage web » : « Plus tard » est le bouton par défaut visible (#2602).

Vraie fenêtre GTK sous Xvfb : à l'ouverture, « Plus tard » est la réponse par
défaut (#2589), il a le focus et le défaut, il porte seul la couleur d'accent,
le cadre de focus est visible, et « Télécharger » n'a ni focus ni défaut.
Dialog.run est remplacé par une sonde qui observe puis répond « Plus tard » :
rien n'est installé.
"""
import importlib.util
import os
from pathlib import Path
import sys

import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("winlux_webview", HERE / "winlux-webview.py")
webview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(webview)

failures = []


def check(condition, message):
    if not condition:
        failures.append(message)
        print("ÉCHEC :", message, file=sys.stderr)


installs = []
webview.install_command = lambda: ["faux-apt-get", "install"]
webview.package_details = lambda: ("2.0", "Debian", "https://deb.debian.org")
webview.subprocess.call = lambda *a, **k: installs.append(a) or 1
os.environ.pop("WINLUX_WEBVIEW_NO_INSTALL", None)
seen = {}


def probe(dialog):
    dialog.show_all()
    while Gtk.events_pending():
        Gtk.main_iteration()
    later = dialog.get_widget_for_response(Gtk.ResponseType.CANCEL)
    download = dialog.get_widget_for_response(Gtk.ResponseType.OK)
    seen.update(
        later=later.get_label(), download=download.get_label(),
        focus=dialog.get_focus() is later, focus_visible=dialog.get_focus_visible(),
        default=dialog.get_default_widget() is later,
        later_accent=later.get_style_context().has_class("suggested-action"),
        download_accent=download.get_style_context().has_class("suggested-action"),
        download_focus=download.has_focus(), download_default=download.has_default(),
    )
    return Gtk.ResponseType.CANCEL


Gtk.Dialog.run = probe
check(webview.offer_install() is False, "« Plus tard » n'a pas refusé l'installation")
check(not installs, "une installation a été lancée")
check(bool(seen), "le dialogue ne s'est pas ouvert")
if seen:
    check(seen["focus"], f"« {seen['later']} » n'a pas le focus à l'ouverture")
    check(seen["focus_visible"], "le cadre de focus n'est pas visible à l'ouverture")
    check(seen["default"], f"« {seen['later']} » n'est pas le bouton par défaut")
    check(seen["later_accent"], f"« {seen['later']} » ne porte pas la couleur d'accent")
    check(not seen["download_accent"], f"« {seen['download']} » porte la couleur d'accent")
    check(not seen["download_focus"] and not seen["download_default"],
          f"« {seen['download']} » a le focus ou le défaut")

if failures:
    sys.exit(1)
print("Affichage web : « Plus tard » est le bouton par défaut visible (#2602)")
