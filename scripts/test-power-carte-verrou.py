#!/usr/bin/python3
"""#2300 : sur un écran verrouillé, la carte « Ces applications empêchent
l'arrêt » n'offre que « Annuler » (par défaut) et « Déverrouiller ». Forcer
l'arrêt exige le mot de passe : « Déverrouiller » efface l'écran d'arrêt et
rend la saisie au verrou ; une fois la session déverrouillée, la carte
complète revient avec « Arrêter quand même ». À lancer sous Xvfb (xvfb-run)."""
import os
from pathlib import Path
import runpy
import tempfile

root = tempfile.mkdtemp(prefix="winlux-2300-")
os.makedirs(root + "/run/winlux/lock")
os.environ["WINLUX_LANG"] = "fr"
os.environ["WINLUX_CONTROL_ROOT"] = root
power = runpy.run_path(str(Path(__file__).resolve().with_name("winlux-power.py")), run_name="power_test")
failures = []
marker = root + "/run/winlux/lock/active" + power["_screen_suffix"]()


def set_locked(locked):
    """Marqueur de winlux-lock : un pid vivant (le nôtre) = écran verrouillé."""
    if locked:
        Path(marker).write_text(f"{os.getpid()}\n")
    elif os.path.exists(marker):
        os.unlink(marker)


blockers = {"0x1": ("mousepad.Mousepad", "*Courrier confidentiel - Mousepad", 1)}


def check(cond, label):
    if not cond:
        failures.append(label)


def pump(ui, count=50):
    for _ in range(count):
        while ui.Gtk.events_pending():
            ui.Gtk.main_iteration()


def card_texts(ui):
    labels, buttons = [], []

    def walk(widget):
        if isinstance(widget, ui.Gtk.Button):
            buttons.append(widget.get_label())
        elif isinstance(widget, ui.Gtk.Label):
            labels.append(widget.get_text())
        if isinstance(widget, ui.Gtk.Container):
            for child in widget.get_children():
                walk(child)
    walk(ui.card)
    return labels, buttons


for action, forced in (("poweroff", "Arrêter quand même"), ("reboot", "Redémarrer quand même")):
    set_locked(True)
    ui = power["PowerUI"](action, confirm=True)
    check(ui.locked, f"{action} : écran verrouillé non détecté")
    ui.windows = dict(blockers)
    ui.show_blocked(dict(blockers))
    pump(ui)
    labels, buttons = card_texts(ui)
    check(buttons == ["Annuler", "Déverrouiller"], f"{action} : boutons sur le verrou {buttons!r}")
    check(forced not in buttons, f"{action} : « {forced} » offert sans mot de passe")
    check(not any("Courrier confidentiel" in text for text in labels), f"{action} : titre de fenêtre visible sur le verrou")
    focus = ui.window.get_focus()
    check(focus is not None and focus.get_label() == "Annuler", f"{action} : focus sur {focus and focus.get_label()!r}")
    # « Déverrouiller » : l'écran d'arrêt s'efface et rend la saisie au verrou.
    ui.unlock_first()
    pump(ui)
    check(not ui.window.get_visible(), f"{action} : écran d'arrêt encore visible pendant le déverrouillage")
    check(not os.path.exists(power["power_request_path"]()), f"{action} : demande de saisie au verrou non retirée")
    check(ui.result == 1, f"{action} : arrêt lancé sans réponse")
    # Toujours verrouillé : rien ne bouge, l'arrêt attend sans délai.
    check(ui.wait_unlock() is True and ui.mode == "unlock", f"{action} : l'attente du déverrouillage s'arrête")
    # Session déverrouillée : la carte complète revient, titres compris.
    set_locked(False)
    check(ui.wait_unlock() is False, f"{action} : déverrouillage non vu")
    pump(ui)
    labels, buttons = card_texts(ui)
    check(ui.window.get_visible(), f"{action} : carte non réaffichée après déverrouillage")
    check(buttons == ["Annuler", forced], f"{action} : boutons après déverrouillage {buttons!r}")
    check(any("Courrier confidentiel" in text for text in labels), f"{action} : titre absent après déverrouillage")
    check(ui.result == 1, f"{action} : arrêt lancé sans réponse après déverrouillage")
    ui.window.destroy()
    pump(ui)

if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    raise SystemExit(1)
print("OK : sur le verrou, « Annuler » et « Déverrouiller » seulement ; « Arrêter quand même » après le mot de passe (#2300)")
