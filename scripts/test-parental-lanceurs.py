#!/usr/bin/python3
"""Contrôle parental : liste « Choisir une application » (#2248).

Lanceurs factices dans une racine de test : deux lanceurs pour le même
programme (Mousepad et ses réglages), lanceurs cachés du menu (NoDisplay,
Hidden), lanceur qui n'est pas une application. La liste doit proposer
« Mousepad » sous son nom, rien de caché, et garder les noms affichés.
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import os
from pathlib import Path
import shutil
import sys
import tempfile

HERE = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="winlux-parental-lanceurs."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"
failures = 0


def check(cond, label):
    global failures
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures += 1


apps = work / "usr/share/applications"
apps.mkdir(parents=True)
LAUNCHERS = {
    # Trié avant org.xfce.mousepad.desktop (« - » avant « . ») : c'est le piège.
    "org.xfce.mousepad-settings.desktop": "Name=Text Editor Settings\nExec=mousepad --preferences\nNoDisplay=true\n",
    "org.xfce.mousepad.desktop": "Name=Mousepad\nExec=mousepad %U\n",
    "gcr-prompter.desktop": "Name=Access Prompt\nExec=/usr/libexec/gcr-prompter\nNoDisplay=true\n",
    "nm-applet.desktop": "Name=Network\nExec=nm-applet\nHidden=true\n",
    "lien.desktop": "Type=Link\nName=Site\nExec=sitelink\n",
    "a-firefox-reglages.desktop": "Name=Firefox Settings\nExec=firefox --preferences\n",
    "firefox.desktop": "Name=Firefox\nExec=env MOZ=1 firefox %u\n",
    "xterm.desktop": "Name=XTerm\nExec=xterm\n",
    "openbox.desktop": "Name=Openbox\nExec=openbox\n",
}
for file, body in LAUNCHERS.items():
    (apps / file).write_text("[Desktop Entry]\nType=Application\n" + body if not body.startswith("Type=")
                             else "[Desktop Entry]\n" + body, encoding="utf-8")

loader = SourceFileLoader("winlux_parental", str(HERE / "winlux-parental.py"))
spec = importlib.util.spec_from_loader("winlux_parental", loader)
parental = importlib.util.module_from_spec(spec)
loader.exec_module(parental)

choices = parental.desktop_choices()
names = parental.desktop_names()
check(choices.get("bin:mousepad") == "Mousepad",
      "Mousepad proposé sous son nom, pas « Text Editor Settings » (#2248)")
check(choices.get("bin:firefox") == "Firefox", "lanceur principal préféré à un lanceur à option (« --preferences »)")
check("bin:gcr-prompter" not in choices and "bin:nm-applet" not in choices,
      "lanceurs cachés du menu (NoDisplay, Hidden) absents de la liste")
check("bin:sitelink" not in choices and "bin:sitelink" not in names, "lanceur qui n'est pas une application ignoré")
check("bin:openbox" not in choices, "programme du bureau lui-même jamais proposé")
check(choices.get("bin:xterm") == "XTerm", "application ordinaire proposée")
check(names.get("bin:mousepad") == "Mousepad" and names.get("bin:gcr-prompter") == "Access Prompt",
      "noms affichés dans l'activité : lanceur du menu d'abord, lanceur caché à défaut")

shutil.rmtree(work, ignore_errors=True)
print("test-parental-lanceurs :", "réussi" if failures == 0 else f"{failures} échec(s)")
sys.exit(1 if failures else 0)
