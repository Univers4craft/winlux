#!/usr/bin/python3
"""#3198 : l'icône d'une fenêtre .exe est posée dès que la fenêtre apparaît.

1. Cache décodé : la 1re fenêtre d'une application décode son icône dans un
   fils (glycin) ; une fenêtre suivante de la même application la reçoit du
   cache, dans le démon, sans fork ni décodage. Une icône modifiée est décodée
   de nouveau ; un fichier de cache modifiable par d'autres est ignoré.
2. Événement : le démon est réveillé par le PropertyNotify de
   _NET_CLIENT_LIST sur la racine (EWMH) au lieu d'attendre jusqu'à 5 s
   (vérifié sous Xvfb quand il est installé)."""
import ctypes
import os
import runpy
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
problems = []
c = runpy.run_path(str(HERE / "winlux-applications.py"), run_name="icone_immediate")
g = c["window_icons"].__globals__

with tempfile.TemporaryDirectory() as directory:
    directory = Path(directory)
    exe = directory / "prog.exe"
    exe.write_bytes(b"MZ")
    png = next((p for p in Path("/usr/share/icons").rglob("*.png") if p.stat().st_size > 512), None)
    icone = directory / "icone.png"
    if png: shutil.copy(png, icone)
    fenetre = subprocess.Popen(["winlux", "-c", "sleep 60; :", str(exe)], executable="/bin/sh", start_new_session=True)
    try:
        journal = directory / "publications"
        g["WINDOW_ICON_CACHE"] = directory / "window-icons.json"
        g["ARGB_CACHE"] = directory / "argb"
        g["icon_for"] = lambda target, identity: str(icone) if png else "application-x-executable"
        def publish(window, values):
            with open(journal, "a") as handle: handle.write(f"{os.getpid()} {window} {len(values)}\n")
        g["publish_icon"] = publish
        fenetres = []
        g["subprocess"].check_output = lambda *a, **k: "".join(f"{w}  0 {fenetre.pid}   hote prog\n" for w in fenetres)
        forks, vrai_fork = [], os.fork
        def fork():
            forks.append(1)
            return vrai_fork()
        g["os"].fork = fork
        def tick(window):
            fenetres.append(window)
            avant = len(forks)
            debut = time.perf_counter()
            c["window_icons"]()
            duree = (time.perf_counter() - debut) * 1000
            lignes = journal.read_text().split("\n")[:-1] if journal.exists() else []
            derniere = lignes[-1].split() if lignes else ["", "", "0"]
            return len(forks) - avant, derniere, duree
        try:
            if png is None:
                print("(aucun PNG de thème dans ce conteneur : cache non vérifié)")
            else:
                f1, p1, d1 = tick("0x04000001")
                f2, p2, d2 = tick("0x04000002")
                print(f"1re fenêtre : fork {f1}, publiée par {p1[0]} en {d1:.1f} ms ; 2e fenêtre : fork {f2}, publiée par {p2[0]} en {d2:.1f} ms (démon {os.getpid()})")
                if f1 != 1 or p1[1] != "0x04000001" or p1[0] == str(os.getpid()):
                    problems.append("1re fenêtre : icône non décodée dans un fils")
                if f2 != 0 or p2[1] != "0x04000002" or p2[0] != str(os.getpid()):
                    problems.append("2e fenêtre de la même application : pas servie par le cache dans le démon")
                if p1[2] != p2[2] or int(p2[2]) < 8:
                    problems.append(f"pixels du cache différents du décodage : {p1[2]} contre {p2[2]}")
                if c["load_argb"](str(icone)) is None:
                    problems.append("cache illisible après écriture")
                os.utime(icone, ns=(time.time_ns(), time.time_ns() + 10**9))
                if c["load_argb"](str(icone)) is not None:
                    problems.append("icône modifiée : l'ancien cache est encore servi")
                f3, p3, _ = tick("0x04000003")
                if f3 != 1:
                    problems.append("icône modifiée : pas décodée de nouveau")
                for item in (directory / "argb").iterdir(): item.chmod(0o666)
                if c["load_argb"](str(icone)) is not None:
                    problems.append("un cache modifiable par d'autres comptes est servi")
                modes = {oct(item.stat().st_mode & 0o777) for item in (directory / "argb").iterdir()}
                print("droits du cache après le test :", modes)
        finally:
            g["os"].fork = vrai_fork
    finally:
        os.killpg(fenetre.pid, 9); fenetre.wait()

    # Fichier neuf : 0600, dossier 0700.
    g["ARGB_CACHE"] = directory / "argb2"
    c["save_argb"](str(exe), [1, 1, 0xFF00FF00])
    neuf = next((directory / "argb2").iterdir())
    if neuf.stat().st_mode & 0o777 != 0o600 or (directory / "argb2").stat().st_mode & 0o777 != 0o700:
        problems.append(f"droits du cache : {oct(neuf.stat().st_mode)} / {oct((directory / 'argb2').stat().st_mode)}")
    if c["load_argb"](str(exe)) != [1, 1, 0xFF00FF00]:
        problems.append("aller-retour du cache faux")

# 2. Réveil par _NET_CLIENT_LIST.
if shutil.which("Xvfb"):
    for numero in range(57, 90):
        if not Path(f"/tmp/.X11-unix/X{numero}").exists(): break
    serveur = subprocess.Popen(["Xvfb", f":{numero}", "-nolisten", "tcp"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        os.environ["DISPLAY"] = f":{numero}"
        for _ in range(100):
            if Path(f"/tmp/.X11-unix/X{numero}").exists(): break
            time.sleep(0.05)
        time.sleep(0.2)
        watch = c["ClientListWatch"]()
        debut = time.perf_counter()
        calme = watch.wait(0.3)
        attente = time.perf_counter() - debut
        x = ctypes.CDLL("libX11.so.6")
        x.XOpenDisplay.restype = ctypes.c_void_p
        x.XDefaultRootWindow.argtypes, x.XDefaultRootWindow.restype = [ctypes.c_void_p], ctypes.c_ulong
        x.XInternAtom.argtypes, x.XInternAtom.restype = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int], ctypes.c_ulong
        x.XChangeProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_void_p, ctypes.c_int]
        x.XFlush.argtypes = [ctypes.c_void_p]
        wm = x.XOpenDisplay(None)
        racine = x.XDefaultRootWindow(wm)
        def propriete(nom):
            valeur = (ctypes.c_ulong * 1)(0x400001)
            x.XChangeProperty(wm, racine, x.XInternAtom(wm, nom, 0), 33, 32, 0, ctypes.cast(valeur, ctypes.c_void_p), 1)
            x.XFlush(wm)
        propriete(b"_NET_ACTIVE_WINDOW")
        autre = watch.wait(0.3)
        propriete(b"_NET_CLIENT_LIST")
        debut = time.perf_counter()
        nouvelle = watch.wait(5.0)
        reveil = (time.perf_counter() - debut) * 1000
        print(f"sans fenêtre : {calme} après {attente:.2f} s ; autre propriété : {autre} ; nouvelle fenêtre : {nouvelle} en {reveil:.1f} ms")
        if calme or attente < 0.25: problems.append("réveil sans changement de la liste des fenêtres")
        if autre: problems.append("réveil pour une autre propriété que _NET_CLIENT_LIST")
        if not nouvelle or reveil > 100: problems.append(f"nouvelle fenêtre vue en {reveil:.0f} ms au lieu d'aussitôt")
    finally:
        serveur.kill(); serveur.wait()
else:
    print("(Xvfb absent : réveil par _NET_CLIENT_LIST non vérifié ici)")

for problem in problems:
    print("ÉCHEC :", problem)
if problems:
    sys.exit(1)
print("Icône d'une fenêtre .exe posée dès son apparition : réveil par _NET_CLIENT_LIST, pixels servis par le cache (#3198).")
