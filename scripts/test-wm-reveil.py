#!/usr/bin/python3
"""Fenêtre affichée pendant le démarrage du gestionnaire de fenêtres (#3190).

La session attend « wmctrl -m » (_NET_SUPPORTING_WM_CHECK, posé par openbox
dès qu'il prend l'écran) puis ouvre ses fenêtres. Openbox 3.6 n'a pas fini de
démarrer à ce moment-là : il lit les événements X dans sa propre file en
examinant les fenêtres, et sa boucle ne regarde ensuite que XPending(). Une
fenêtre affichée dans cet intervalle n'était traitée qu'au prochain événement
X reçu par openbox : jamais, sur un écran au repos. Mesuré sous Xvfb avec
l'openbox d'Ubuntu 3.6.1-12ubuntu3 : 10 fenêtres bloquées sur 10.

À chaque tour : Xvfb neuf, openbox avec le rc.xml de la session lancé avec
winlux-wm-reveil comme dans les deux scripts de session, puis une fenêtre GTK
affichée dès que _NET_SUPPORTING_WM_CHECK apparaît. Elle doit être gérée
(_NET_CLIENT_LIST) en moins de 5 s, à chaque tour. Un tour témoin sans
winlux-wm-reveil dit si le gestionnaire testé a encore le défaut.

    python3 scripts/test-wm-reveil.py        (openbox du PATH ; WINLUX_OPENBOX pour un autre)
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
OPENBOX = os.environ.get("WINLUX_OPENBOX") or shutil.which("openbox") or ""
REVEIL = ROOT / "scripts/winlux-wm-reveil.sh"
RC = ROOT / "integration/native/config/openbox/rc.xml"
TOURS = int(os.environ.get("WINLUX_TOURS", "10"))
TITRE = "Sonde-3190"

# Un tour, dans un sous-processus qui a son propre DISPLAY.
SONDE = r"""
import subprocess, sys, time
import gi
gi.require_version("Gtk", "3.0"); gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk
openbox_bin, titre, rc, reveil = sys.argv[1:5]
# Fenêtre construite d'avance : seul son affichage tombe dans le démarrage.
w = Gtk.Window(title=titre); w.set_default_size(300, 200)
w.realize()
procs = [subprocess.Popen([openbox_bin, "--sm-disable", "--config-file", rc],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)]
if reveil:
    procs.append(subprocess.Popen(["sh", reveil, str(procs[0].pid)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
try:
    for _ in range(3000):
        if "window id" in subprocess.run(["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"],
                                         capture_output=True, text=True).stdout:
            break
    w.show_all(); Gdk.flush()
    debut = time.monotonic()
    while time.monotonic() - debut < 5:
        while Gtk.events_pending(): Gtk.main_iteration()
        if titre in subprocess.run(["wmctrl", "-l"], capture_output=True, text=True).stdout:
            print(f"gérée en {(time.monotonic() - debut) * 1000:.0f} ms"); sys.exit(0)
        time.sleep(0.02)
    etat = subprocess.run(["xwininfo", "-id", hex(w.get_window().get_xid())], capture_output=True, text=True).stdout
    print("jamais gérée :", " ".join(l.strip() for l in etat.splitlines() if "Map State" in l)); sys.exit(1)
finally:
    for p in reversed(procs):
        p.kill(); p.wait()
"""


def tour(avec_reveil):
    lu, ecrit = os.pipe()
    xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(ecrit), "-screen", "0", "1024x600x24", "-nolisten", "tcp"],
                            pass_fds=(ecrit,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    os.close(ecrit)
    ecran = os.read(lu, 16).decode().strip()
    os.close(lu)
    try:
        env = dict(os.environ, DISPLAY=f":{ecran}", NO_AT_BRIDGE="1")
        r = subprocess.run([sys.executable, "-c", SONDE, OPENBOX, TITRE, str(RC), str(REVEIL) if avec_reveil else ""],
                           env=env, capture_output=True, text=True, timeout=60)
        sortie = (r.stdout + r.stderr).strip().splitlines()
        return r.returncode == 0, sortie[-1] if sortie else str(r.returncode)
    finally:
        xvfb.kill(); xvfb.wait()


def main():
    for outil in ("Xvfb", "xprop", "wmctrl", "xwininfo"):
        if not shutil.which(outil):
            print(f"{outil} absent : test sauté"); return 0
    if not OPENBOX:
        print("openbox absent : test sauté"); return 0
    ok, texte = tour(False)
    print(f"témoin sans winlux-wm-reveil ({OPENBOX}) : {texte}"
          + ("" if ok else " (défaut #3190 du gestionnaire présent)"))
    echecs = 0
    for n in range(1, TOURS + 1):
        ok, texte = tour(True)
        print(f"tour {n} avec winlux-wm-reveil : {texte}")
        echecs += not ok
    if echecs:
        print(f"ÉCHEC : {echecs} fenêtre(s) sur {TOURS} affichée(s) pendant le démarrage d'openbox jamais gérée(s) (#3190)")
        return 1
    print(f"Fenêtre affichée pendant le démarrage d'openbox gérée {TOURS} fois sur {TOURS} avec winlux-wm-reveil (#3190)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
