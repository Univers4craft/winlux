#!/usr/bin/python3
"""Contrôle parental : nom tapé dans « Choisir une application » (#2350).

Avant, taper « mouse » puis cliquer « Bloquer » enregistrait bin:mouse, un
programme qui n'existe pas : Mousepad restait permis, sans un mot. Le panneau
doit maintenant, sous Xvfb avec le vrai GTK :
  - proposer l'application qui existe sous un autre nom (« mouse » → Mousepad),
    la présélectionner, et ne rien bloquer tant que l'adulte n'a pas confirmé ;
  - refuser un texte inventé, avec un message visible sous le champ ;
  - bloquer comme avant un nom exact, un programme installé ou un .exe.
Sans root : racine factice (WINLUX_PARENTAL_ROOT), enregistrement simulé.
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import os
from pathlib import Path
import shutil
import sys
import tempfile

HERE = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="winlux-parental-saisie."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"
os.environ["NO_AT_BRIDGE"] = "1"
failures = 0


def check(cond, label):
    global failures
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures += 1


try:
    import gi
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gtk
except (ImportError, ValueError) as error:
    print(f"ÉCHEC prérequis manquant : PyGObject et GTK 3 (python3-gi, gir1.2-gtk-3.0) : {error}")
    sys.exit(1)
if not Gtk.init_check(sys.argv)[0]:
    print("ÉCHEC prérequis manquant : affichage X (lancer sous xvfb-run)")
    sys.exit(1)

apps = work / "usr/share/applications"
apps.mkdir(parents=True)
for file, body in {
    "org.xfce.mousepad.desktop": "Name=Mousepad\nExec=mousepad %U\n",
    "xterm.desktop": "Name=XTerm\nExec=xterm\n",
    "uxterm.desktop": "Name=UXTerm\nExec=uxterm\n",
    "ristretto.desktop": "Name=Ristretto Image Viewer\nExec=ristretto %U\n",
}.items():
    (apps / file).write_text("[Desktop Entry]\nType=Application\n" + body, encoding="utf-8")
(work / "usr/bin").mkdir(parents=True)
tool = work / "usr/bin/outil-console"
tool.write_text("#!/bin/sh\n", encoding="utf-8")
tool.chmod(0o755)

loader = SourceFileLoader("winlux_parental", str(HERE / "winlux-parental.py"))
spec = importlib.util.spec_from_loader("winlux_parental", loader)
parental = importlib.util.module_from_spec(spec)
loader.exec_module(parental)

# Fonctions pures.
choices = parental.desktop_choices()
check(parental.typed_app("mouse", choices) is None, "« mouse » n'est pas un programme : rien n'est bloqué (#2350)")
check(parental.typed_app("Mousepad", choices) == "bin:mousepad" and parental.typed_app("mousepad", choices) == "bin:mousepad"
      and parental.typed_app("Mousepad (mousepad)", choices) == "bin:mousepad",
      "nom exact, programme ou libellé de la liste : application reconnue")
check(parental.typed_app("outil-console", choices) == "bin:outil-console",
      "programme installé hors du menu : accepté")
check(parental.typed_app(r"C:\Jeux\Jeu.EXE", choices) == "exe:jeu.exe", ".exe tapé : accepté comme avant")
check(parental.typed_suggestions("mouse", choices) == ["bin:mousepad"]
      and parental.typed_suggestions("xterm", choices) == ["bin:uxterm", "bin:xterm"],
      "suggestions : applications dont le nom contient le texte")


# Le vrai panneau GTK, enregistrement simulé.
class Panel(parental.ParentalPanel):
    def __init__(self):
        self.Gtk = Gtk
        self.box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.cfg = {"apps": []}
        self.saved = 0
        self.message = Gtk.Label()

    def save(self, refresh=False):
        self.saved += 1
        return False


def find(widget, name):
    if widget.get_name() == name:
        return widget
    for child in widget.get_children() if isinstance(widget, Gtk.Container) else ():
        found = find(child, name)
        if found is not None:
            return found
    return None


def build(panel):
    for child in panel.box.get_children():
        child.destroy()
    panel.apps_section({"apps": []})
    window = Gtk.Window()
    window.add(panel.box)
    window.show_all()
    combo = find(panel.box, "parental-app-choice")
    return window, combo, find(panel.box, "parental-block"), find(panel.box, "parental-app-unknown")


def settle():
    while Gtk.events_pending():
        Gtk.main_iteration()


panel = Panel()
window, combo, block, unknown = build(panel)
check(combo is not None and block is not None and unknown is not None and not unknown.get_visible(),
      "panneau construit : liste, bouton Bloquer, message caché au départ")
completion = combo.get_child().get_completion()
check(completion is not None, "saisie : propositions pendant la frappe (complétion GTK)")

combo.get_child().set_text("mouse")
settle()
block.clicked()
settle()
check(panel.cfg["apps"] == [] and panel.saved == 0, "« mouse » + Bloquer : rien n'est enregistré (#2350)")
check(unknown.get_visible() and "Mousepad" in unknown.get_text() and "Rien n’a été bloqué" in unknown.get_text(),
      f"« mouse » : message visible sous le champ, qui propose Mousepad ({unknown.get_text()!r})")
check(combo.get_active_id() == "bin:mousepad", "« mouse » : Mousepad présélectionné, à confirmer")
block.clicked()
settle()
check(panel.cfg["apps"] == [{"id": "bin:mousepad", "name": "Mousepad"}] and panel.saved == 1,
      "second clic sur Bloquer : Mousepad bloqué sous son nom")
window.destroy()

panel = Panel()
window, combo, block, unknown = build(panel)
combo.get_child().set_text("zzz-invente")
settle()
block.clicked()
settle()
check(panel.cfg["apps"] == [] and panel.saved == 0 and unknown.get_visible()
      and "Application introuvable : zzz-invente" in unknown.get_text(),
      "texte inventé : refusé, message visible au moment du clic")
combo.get_child().set_text("Mousepad")
settle()
check(not unknown.get_visible(), "message retiré dès que le texte change")
block.clicked()
settle()
check(panel.cfg["apps"] == [{"id": "bin:mousepad", "name": "Mousepad"}],
      "nom exact tapé : bloqué comme avant")
window.destroy()

panel = Panel()
window, combo, block, unknown = build(panel)
combo.get_child().set_text("term")
settle()
block.clicked()
settle()
check(panel.cfg["apps"] == [] and combo.get_active_id() is None and "« UXTerm » ou « XTerm »" in unknown.get_text(),
      "texte ambigu : plusieurs propositions, aucune présélection, rien de bloqué")
window.destroy()

panel = Panel()
window, combo, block, unknown = build(panel)
combo.get_child().set_text("devoirs.exe")
settle()
block.clicked()
settle()
check(panel.cfg["apps"] == [{"id": "exe:devoirs.exe", "name": "devoirs"}], ".exe tapé : bloqué comme avant")
window.destroy()

check(all(key in parental._TR for key in (
    "Application introuvable : {name}. Rien n’a été bloqué ; choisissez-la dans la liste.",
    "Rien n’a été bloqué. Vouliez-vous dire {names} ? Choisissez-la puis cliquez sur Bloquer.",
    "Rien n’a été bloqué. Vouliez-vous dire {names} ? Elle est sélectionnée : cliquez à nouveau sur Bloquer.",
    " ou ")),
      "messages traduits en anglais, espagnol et allemand")

shutil.rmtree(work, ignore_errors=True)
print("test-parental-saisie :", "réussi" if failures == 0 else f"{failures} échec(s)")
sys.exit(1 if failures else 0)
