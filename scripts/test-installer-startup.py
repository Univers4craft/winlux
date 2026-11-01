#!/usr/bin/python3
"""Lance l'installateur sous X (Xvfb) et vérifie que la page affichée suit l'étape active.

À lancer avec xvfb-run. Régression : la page « Partitionnement » s'affichait au
démarrage alors que la barre latérale indiquait « Langue ».
"""
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
spec = importlib.util.spec_from_file_location("installer", os.path.join(HERE, "winlux-installer.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
installer.detect_disks = lambda: []

from gi.repository import GLib  # noqa: E402

app = installer.InstallerApp()
failures = []


def visible_page():
    return app.pages.get_visible_child_name()


def active_label():
    for lbl in app.step_labels:
        if lbl.get_style_context().has_class("active"):
            return lbl.get_text().strip()
    return None


def check():
    try:
        names = [app.pages.child_get_property(c, "name") for c in app.pages.get_children()]
        if visible_page() != names[app.current_step]:
            failures.append(f"démarrage : page {visible_page()} affichée, étape {active_label()} active")
        if app.btn_back.get_visible():
            failures.append("démarrage : le bouton Retour ne doit pas être visible")
        for step in range(len(names) - 2):
            app.current_step = step
            app.update_step()
            if visible_page() != names[step]:
                failures.append(f"étape {step} ({active_label()}) : page {visible_page()} affichée")
    finally:
        app.quit()
    return False


GLib.timeout_add(500, check)
app.run([])
if failures:
    print("\n".join(failures), file=sys.stderr)
    sys.exit(1)
print("Pages de l'installateur cohérentes avec la barre latérale.")
