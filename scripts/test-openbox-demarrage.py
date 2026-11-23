#!/usr/bin/python3
"""Fenêtre affichée pendant le démarrage d'openbox (#3190).

La session attend « wmctrl -m », c'est-à-dire _NET_SUPPORTING_WM_CHECK posé
par openbox dès qu'il prend l'écran, puis lance la barre des tâches et le
bureau. Openbox n'a pas encore fini de démarrer : il lit alors les événements
X dans sa propre file (libobt) pendant qu'il examine les fenêtres existantes,
et la source X de sa boucle ne regardait que XPending(). Une MapRequest
arrivée à ce moment restait sans suite jusqu'au prochain événement X reçu par
openbox : fenêtre jamais affichée sur un écran au repos. Mesuré sous Xvfb avec
l'openbox d'Ubuntu 3.6.1-12ubuntu3 : 10 fenêtres bloquées sur 10.

Ici, à chaque tour : Xvfb neuf, openbox avec le rc.xml de la session, une fenêtre GTK affichée dès que
_NET_SUPPORTING_WM_CHECK apparaît, et rien d'autre ne touche l'écran. Elle doit
être gérée (dans _NET_CLIENT_LIST) en moins de 5 s, à chaque tour.

    WINLUX_OPENBOX=build/openbox/openbox python3 scripts/test-openbox-demarrage.py
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
OPENBOX = Path(os.environ.get("WINLUX_OPENBOX", ROOT / "build/openbox/openbox"))
TOURS = int(os.environ.get("WINLUX_TOURS", "10"))
TITRE = "Sonde-3190"
# Configuration de la session (menus, thème, raccourcis), comme au démarrage réel.
RC = ROOT / "integration/native/config/openbox/rc.xml"

# Un tour : lancé dans un sous-processus qui a son propre DISPLAY.
SONDE = r"""
import subprocess, sys, time
import gi
gi.require_version("Gtk", "3.0"); gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk
# Fenêtre construite d'avance : seul son affichage tombe dans le démarrage.
w = Gtk.Window(title=sys.argv[2]); w.set_default_size(300, 200)
w.realize()
openbox = subprocess.Popen([sys.argv[1], "--sm-disable", "--config-file", sys.argv[3]], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    for _ in range(3000):
        if "window id" in subprocess.run(["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"],
                                         capture_output=True, text=True).stdout:
            break
    w.show_all(); Gdk.flush()
    debut = time.monotonic()
    while time.monotonic() - debut < 5:
        while Gtk.events_pending(): Gtk.main_iteration()
        if sys.argv[2] in subprocess.run(["wmctrl", "-l"], capture_output=True, text=True).stdout:
            print(f"gérée en {(time.monotonic() - debut) * 1000:.0f} ms"); sys.exit(0)
        time.sleep(0.02)
    etat = subprocess.run(["xwininfo", "-id", hex(w.get_window().get_xid())], capture_output=True, text=True).stdout
    print("jamais gérée :", " ".join(l.strip() for l in etat.splitlines() if "Map State" in l)); sys.exit(1)
finally:
    openbox.kill(); openbox.wait()
"""


def main():
    # Un outil manquant fait échouer le test : sauté, il laissait l'étape
    # verte sans rien vérifier (#3407).
    manquants = [o for o in ("Xvfb", "xprop", "wmctrl", "xwininfo") if not shutil.which(o)]
    if manquants:
        print(f"ÉCHEC : outils absents ({', '.join(manquants)}) : le test ne peut rien vérifier (#3407)")
        return 1
    if not OPENBOX.is_file():
        print(f"ÉCHEC : openbox de Winlux absent ({OPENBOX}) : sh scripts/build-openbox.sh build/openbox")
        return 1
    echecs = 0
    for tour in range(1, TOURS + 1):
        lu, ecrit = os.pipe()
        xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(ecrit), "-screen", "0", "1024x600x24", "-nolisten", "tcp"],
                                pass_fds=(ecrit,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        os.close(ecrit)
        ecran = os.read(lu, 16).decode().strip()
        os.close(lu)
        try:
            env = dict(os.environ, DISPLAY=f":{ecran}", NO_AT_BRIDGE="1")
            r = subprocess.run([sys.executable, "-c", SONDE, str(OPENBOX), TITRE, str(RC)], env=env,
                               capture_output=True, text=True, timeout=60)
            sortie = (r.stdout + r.stderr).strip().splitlines()
            print(f"tour {tour} : {sortie[-1] if sortie else r.returncode}")
            echecs += r.returncode != 0
        finally:
            xvfb.kill(); xvfb.wait()
    if echecs:
        print(f"ÉCHEC : {echecs} fenêtre(s) sur {TOURS} affichée(s) pendant le démarrage d'openbox jamais gérée(s) (#3190)")
        return 1
    print(f"Fenêtre affichée pendant le démarrage d'openbox gérée {TOURS} fois sur {TOURS} (#3190)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
