#!/usr/bin/python3
"""#2299 : la carte « Ces applications empêchent l'arrêt » donne le focus à
« Annuler ». Sur un écran verrouillé, la touche qui réveille l'écran
déclenchait « Arrêter quand même » et perdait le travail non enregistré.
À lancer sous Xvfb (xvfb-run)."""
import os
from pathlib import Path
import runpy

os.environ["WINLUX_LANG"] = "fr"
power = runpy.run_path(str(Path(__file__).resolve().with_name("winlux-power.py")), run_name="power_test")
failures = []
for action in ("poweroff", "reboot"):
    ui = power["PowerUI"](action, confirm=True)
    ui.show_blocked({"0x1": ("mousepad.Mousepad", "*Sans titre 1 - Mousepad", 1)})
    Gtk = ui.Gtk
    for _ in range(50):
        while Gtk.events_pending():
            Gtk.main_iteration()
    focus = ui.window.get_focus()
    label = focus.get_label() if focus is not None else None
    if label != "Annuler":
        failures.append(f"{action} : focus sur {label!r} au lieu de « Annuler »")
    ui.window.destroy()
if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    raise SystemExit(1)
print("OK : carte « Ces applications empêchent l'arrêt » avec « Annuler » par défaut (#2299)")
