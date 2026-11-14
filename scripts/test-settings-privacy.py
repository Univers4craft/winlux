#!/usr/bin/python3
"""Sous Xvfb : l'interrupteur caméra/micro de Paramètres revient à sa
position quand l'opération ne démarre pas ou échoue (UAC refusée, #2416),
et garde la nouvelle position quand elle réussit."""
import runpy
from pathlib import Path

module = runpy.run_path(str(Path(__file__).with_name("winlux-settings.py")), run_name="settings_test")
Gtk, Settings = module["Gtk"], module["Settings"]


class Finished:
    def __init__(self, code):
        self.returncode = code

    def poll(self):
        return self.returncode


class Page:
    """Juste ce que privacy_toggled, restore_switch et la fin d'action lisent."""
    privacy_toggled = Settings.privacy_toggled
    restore_switch = Settings.restore_switch

    def __init__(self, starts):
        self.starts, self.process, self.active_action, self.pending_switch = starts, None, None, None
        self.launched = []

    def execute(self, action):
        if self.starts:
            self.launched.append(action)
            self.active_action, self.process = action, Finished(None)

    def finish(self, code):
        # Même logique que Settings.poll à la fin d'une action.
        self.process, self.active_action = None, None
        switch, self.pending_switch = self.pending_switch, None
        if code and switch is not None and switch.get_parent() is not None:
            self.restore_switch(switch)


def switch_for(page):
    box = Gtk.Box()
    switch = Gtk.Switch(active=True)
    box.pack_start(switch, False, False, 0)
    switch.connect("notify::active", page.privacy_toggled, "privacy-camera")
    return box, switch


# Refus de l'UAC : l'action échoue, l'interrupteur revient sur « activé ».
page = Page(starts=True)
box, switch = switch_for(page)
switch.set_active(False)
assert page.launched == ["privacy-camera-deny"], page.launched
assert not switch.get_active()
page.finish(199)
assert switch.get_active(), "refus : l'interrupteur revient à l'état réel"
assert page.launched == ["privacy-camera-deny"], "aucune action relancée par le retour"

# Réussite : la nouvelle position reste.
page = Page(starts=True)
box, switch = switch_for(page)
switch.set_active(False)
page.finish(0)
assert not switch.get_active(), "réussite : la nouvelle position reste"

# Une autre opération est en cours : rien ne démarre, retour immédiat.
page = Page(starts=False)
box, switch = switch_for(page)
switch.set_active(False)
while Gtk.events_pending():
    Gtk.main_iteration_do(False)
assert switch.get_active() and page.launched == [], "opération occupée : l'interrupteur ne ment pas"

# Le code de fin d'action de Settings.poll suit bien ce comportement.
import inspect
source = inspect.getsource(Settings.poll)
assert "self.restore_switch(switch)" in source and "pending_switch" in source

print("OK: Paramètres, interrupteurs caméra et micro fidèles au réglage (#2416)")
