#!/usr/bin/python3
"""#3196 : les icônes des fenêtres .exe sont rendues hors du démon résident.

« winlux-applications watch » appelle window_icons() toutes les 5 s. Le
rendu (GdkPixbuf, glycin, GIO, fontconfig) et la connexion X se faisaient dans
le démon et y laissaient ~27 Mo de tas C et 5 fils pour toute la session. Ici :
une vraie fenêtre « winlux » simulée, icon_argb réel, publish_icon remplacé par
un journal. On vérifie que l'icône est bien publiée, depuis un autre processus,
que le démon n'a chargé ni GdkPixbuf ni X11 et reste à un seul fil, et qu'un
tick sans fenêtre nouvelle ne crée aucun processus."""
import os
import runpy
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
problems = []

def charge(nom):
    return any(nom in line for line in open("/proc/self/maps"))

with tempfile.TemporaryDirectory() as directory:
    directory = Path(directory)
    exe = directory / "prog.exe"
    exe.write_bytes(b"MZ")
    png = next((p for p in Path("/usr/share/icons").rglob("*.png") if p.stat().st_size > 512), None)
    # Un processus dont argv[0] est « winlux » et qui porte un .exe, comme une vraie fenêtre.
    fenetre = subprocess.Popen(["winlux", "-c", "sleep 60; :", str(exe)], executable="/bin/sh", start_new_session=True)
    try:
        c = runpy.run_path(str(HERE / "winlux-applications.py"), run_name="icones_fils")
        g = c["window_icons"].__globals__
        journal = directory / "publications"
        g["WINDOW_ICON_CACHE"] = directory / "window-icons.json"
        g["ARGB_CACHE"] = directory / "argb"
        g["icon_for"] = lambda target, identity: str(png) if png else "application-x-executable"
        def publish(window, values):
            with open(journal, "a") as handle: handle.write(f"{os.getpid()} {window} {len(values)}\n")
        g["publish_icon"] = publish
        wmctrl = f"0x04000007  0 {fenetre.pid}   hote prog\n"
        g["subprocess"].check_output = lambda *a, **k: wmctrl
        forks = []
        vrai_fork = os.fork
        def fork():
            forks.append(1)
            return vrai_fork()
        g["os"].fork = fork
        try:
            c["window_icons"]()
            premier = len(forks)
            c["window_icons"]()
        finally:
            g["os"].fork = vrai_fork
        lignes = journal.read_text().split("\n")[:-1] if journal.exists() else []
        print("publications :", lignes, "| forks :", len(forks), "| fils du démon :", len(os.listdir("/proc/self/task")))
        t = open("/proc/self/smaps_rollup").read()
        print("démon :", " ".join(l.split()[0] + l.split()[1] for l in t.splitlines() if l.startswith(("Pss:", "Pss_Anon:"))), "kB")
        print("démon : GdkPixbuf chargé =", charge("libgdk_pixbuf"), "| X11 chargé =", charge("libX11"))
        if png is None:
            print("(aucun PNG de thème dans ce conteneur : rendu non vérifié)")
        elif len(lignes) != 1 or not lignes[0].split()[2].isdigit() or int(lignes[0].split()[2]) < 8:
            problems.append(f"icône non publiée une fois : {lignes}")
        if lignes and lignes[0].split()[0] == str(os.getpid()):
            problems.append("l'icône est rendue dans le démon lui-même")
        if charge("libgdk_pixbuf") or charge("libX11"):
            problems.append("le démon a chargé GdkPixbuf ou X11")
        if len(os.listdir("/proc/self/task")) != 1:
            problems.append("le démon a gagné des fils")
        if premier != 1 or len(forks) != 1:
            problems.append(f"forks : {premier} au 1er tick, {len(forks)} en tout (attendu 1 puis 1)")
        cache = directory / "window-icons.json"
        if not cache.exists() or "0x04000007" not in cache.read_text():
            problems.append("cache des icônes non écrit")
    finally:
        os.killpg(fenetre.pid, 9); fenetre.wait()

for problem in problems:
    print("ÉCHEC :", problem)
if problems:
    sys.exit(1)
print("Icônes des fenêtres rendues dans un fils, seulement à l'apparition d'une fenêtre ; démon sans GdkPixbuf ni X11 (#3196).")
