#!/usr/bin/env python3
"""Mesure de la latence d'une application Windows sous Winlux, vue de X11.

Lance la sonde testc/latence/latence.c (fenêtre unie rouge, verte/bleue à
chaque touche ou clic) et mesure DEHORS, par le serveur X :

  lancement  : instant du fork -> premier pixel rouge lu dans la fenêtre
               (XGetImage), et instant où la fenêtre devient visible ;
  touche     : XTestFakeKeyEvent -> pixel de la nouvelle couleur ;
  clic       : XTestFakeButtonEvent -> pixel de la nouvelle couleur.

Aucun chiffre ne vient de la sonde ni du runtime : seul ce script chronomètre
(time.monotonic), avec une scrutation toutes les 0,2 ms, donc une résolution
de l'ordre de la milliseconde. Résultat : médiane, minimum et 90e centile en
millisecondes, une ligne JSON par grandeur avec --json.

Usage :
  mesure-latence.py [--essais N] [--touches N] [--json] -- COMMANDE... SONDE.exe
Exemple :
  xvfb-run -a python3 -I scripts/mesure-latence.py -- target/release/winlux latence64.exe
"""
import ctypes
import ctypes.util
import json
import os
import signal
import statistics
import subprocess
import sys
import time

TITRE = b"Sonde de latence"
ROUGE, VERT, BLEU = 0xFF0000, 0x00FF00, 0x0000FF
ZPIXMAP = 2
ALL_PLANES = 0xFFFFFFFF
IS_VIEWABLE = 2


class XWindowAttributes(ctypes.Structure):
    _fields_ = [("x", ctypes.c_int), ("y", ctypes.c_int), ("width", ctypes.c_int),
                ("height", ctypes.c_int), ("border_width", ctypes.c_int), ("depth", ctypes.c_int),
                ("visual", ctypes.c_void_p), ("root", ctypes.c_ulong), ("c_class", ctypes.c_int),
                ("bit_gravity", ctypes.c_int), ("win_gravity", ctypes.c_int),
                ("backing_store", ctypes.c_int), ("backing_planes", ctypes.c_ulong),
                ("backing_pixel", ctypes.c_ulong), ("save_under", ctypes.c_int),
                ("colormap", ctypes.c_ulong), ("map_installed", ctypes.c_int),
                ("map_state", ctypes.c_int), ("all_event_masks", ctypes.c_long),
                ("your_event_mask", ctypes.c_long), ("do_not_propagate_mask", ctypes.c_long),
                ("override_redirect", ctypes.c_int), ("screen", ctypes.c_void_p)]


def charger_x11():
    x = ctypes.CDLL(ctypes.util.find_library("X11"))
    xt = ctypes.CDLL(ctypes.util.find_library("Xtst"))
    x.XOpenDisplay.restype = ctypes.c_void_p
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XDefaultRootWindow.restype = ctypes.c_ulong
    x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
                             ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
                             ctypes.POINTER(ctypes.c_uint)]
    x.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]
    x.XFree.argtypes = [ctypes.c_void_p]
    x.XGetWindowAttributes.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(XWindowAttributes)]
    x.XGetImage.restype = ctypes.c_void_p
    x.XGetImage.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint,
                            ctypes.c_uint, ctypes.c_ulong, ctypes.c_int]
    x.XGetPixel.restype = ctypes.c_ulong
    x.XGetPixel.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
    x.XDestroyImage.argtypes = [ctypes.c_void_p]
    x.XKeysymToKeycode.restype = ctypes.c_ubyte
    x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    x.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
    x.XTranslateCoordinates.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int,
                                        ctypes.c_int, ctypes.POINTER(ctypes.c_int),
                                        ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong)]
    x.XFlush.argtypes = [ctypes.c_void_p]
    x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    x.XSetErrorHandler.restype = ctypes.c_void_p
    x.XSetErrorHandler.argtypes = [ctypes.c_void_p]
    xt.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    xt.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    xt.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                        ctypes.c_ulong]
    return x, xt


X, XT = charger_x11()
# Une fenêtre qui disparaît pendant la scrutation ne doit pas tuer le script.
_ERREUR = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)(lambda d, e: 0)
X.XSetErrorHandler(ctypes.cast(_ERREUR, ctypes.c_void_p))


def enfants(d, w):
    racine, parent = ctypes.c_ulong(), ctypes.c_ulong()
    liste, n = ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
    if not X.XQueryTree(d, w, ctypes.byref(racine), ctypes.byref(parent), ctypes.byref(liste), ctypes.byref(n)):
        return []
    r = [liste[i] for i in range(n.value)]
    if liste:
        X.XFree(liste)
    return r


def chercher(d, w, profondeur=3):
    for c in enfants(d, w):
        nom = ctypes.c_char_p()
        if X.XFetchName(d, c, ctypes.byref(nom)) and nom.value is not None:
            trouve = nom.value == TITRE
            X.XFree(nom)
            if trouve:
                return c
        if profondeur > 1:
            t = chercher(d, c, profondeur - 1)
            if t:
                return t
    return 0


def attributs(d, w):
    a = XWindowAttributes()
    if not X.XGetWindowAttributes(d, w, ctypes.byref(a)):
        return None
    return a


def pixel(d, w, a):
    img = X.XGetImage(d, w, a.width // 2, a.height // 2, 1, 1, ALL_PLANES, ZPIXMAP)
    if not img:
        return None
    p = X.XGetPixel(img, 0, 0) & 0xFFFFFF
    X.XDestroyImage(img)
    return p


def attendre_couleur(d, w, a, couleur, limite):
    while time.monotonic() < limite:
        if pixel(d, w, a) == couleur:
            return time.monotonic()
        time.sleep(0.0002)
    return None


def stats(valeurs):
    v = sorted(valeurs)
    if not v:
        return None
    return {"mediane": round(statistics.median(v), 2), "min": round(v[0], 2),
            "p90": round(v[min(len(v) - 1, int(len(v) * 0.9))], 2), "n": len(v)}


def un_lancement(d, racine, commande, touches, delai):
    env = dict(os.environ)
    t0 = time.monotonic()
    proc = subprocess.Popen(commande, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env,
                            start_new_session=True)
    limite = t0 + delai
    w, visible, peint = 0, None, None
    try:
        while time.monotonic() < limite and proc.poll() is None:
            if not w:
                w = chercher(d, racine)
            if w:
                a = attributs(d, w)
                if a and a.map_state == IS_VIEWABLE:
                    visible = visible or time.monotonic()
                    if pixel(d, w, a) == ROUGE:
                        peint = time.monotonic()
                        break
            time.sleep(0.0002)
        if not peint:
            return None
        res = {"visible": (visible - t0) * 1000, "premier_pixel": (peint - t0) * 1000,
               "touche": [], "clic": []}
        a = attributs(d, w)
        X.XSetInputFocus(d, w, 1, 0)
        X.XSync(d, 0)
        cle = X.XKeysymToKeycode(d, 0x20)  # espace
        attendu = ROUGE
        for i in range(touches):
            attendu = VERT if attendu != VERT else BLEU
            t1 = time.monotonic()
            XT.XTestFakeKeyEvent(d, cle, 1, 0)
            X.XFlush(d)
            t2 = attendre_couleur(d, w, a, attendu, t1 + 2)
            XT.XTestFakeKeyEvent(d, cle, 0, 0)
            X.XFlush(d)
            if t2 is None:
                break
            res["touche"].append((t2 - t1) * 1000)
            time.sleep(0.02)
        cx, cy, enfant = ctypes.c_int(), ctypes.c_int(), ctypes.c_ulong()
        X.XTranslateCoordinates(d, w, racine, a.width // 2, a.height // 2, ctypes.byref(cx), ctypes.byref(cy),
                                ctypes.byref(enfant))
        XT.XTestFakeMotionEvent(d, -1, cx.value, cy.value, 0)
        X.XSync(d, 0)
        time.sleep(0.05)
        for i in range(touches):
            attendu = VERT if attendu != VERT else BLEU
            t1 = time.monotonic()
            XT.XTestFakeButtonEvent(d, 1, 1, 0)
            X.XFlush(d)
            t2 = attendre_couleur(d, w, a, attendu, t1 + 2)
            XT.XTestFakeButtonEvent(d, 1, 0, 0)
            X.XFlush(d)
            if t2 is None:
                break
            res["clic"].append((t2 - t1) * 1000)
            time.sleep(0.02)
        # Échap : la sonde sort d'elle-même.
        X.XSetInputFocus(d, w, 1, 0)
        echap = X.XKeysymToKeycode(d, 0xFF1B)
        XT.XTestFakeKeyEvent(d, echap, 1, 0)
        XT.XTestFakeKeyEvent(d, echap, 0, 0)
        X.XFlush(d)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
        return res
    finally:
        if proc.poll() is None:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except OSError:
                pass
            proc.wait()
        # La fenêtre doit être partie avant l'essai suivant.
        fin = time.monotonic() + 3
        while time.monotonic() < fin and chercher(d, racine):
            time.sleep(0.01)


def main(argv):
    essais, touches, en_json = 10, 10, False
    if "--" not in argv:
        print(__doc__, file=sys.stderr)
        return 2
    i = argv.index("--")
    opts, commande = argv[:i], argv[i + 1:]
    j = 0
    while j < len(opts):
        if opts[j] == "--essais":
            essais = int(opts[j + 1]); j += 2
        elif opts[j] == "--touches":
            touches = int(opts[j + 1]); j += 2
        elif opts[j] == "--json":
            en_json = True; j += 1
        else:
            print("option inconnue : " + opts[j], file=sys.stderr)
            return 2
    d = X.XOpenDisplay(None)
    if not d:
        print("pas d'écran X (DISPLAY)", file=sys.stderr)
        return 2
    racine = X.XDefaultRootWindow(d)
    mesures = {"visible": [], "premier_pixel": [], "touche": [], "clic": []}
    echecs = 0
    # Un premier lancement à blanc : caches disque et page chauds, comme un
    # second double-clic. Il n'entre pas dans les chiffres.
    un_lancement(d, racine, commande, 0, 30)
    for _ in range(essais):
        r = un_lancement(d, racine, commande, touches, 30)
        if r is None:
            echecs += 1
            continue
        for k in ("visible", "premier_pixel"):
            mesures[k].append(r[k])
        mesures["touche"] += r["touche"]
        mesures["clic"] += r["clic"]
    resultat = {k: stats(v) for k, v in mesures.items()}
    resultat["echecs"] = echecs
    if en_json:
        print(json.dumps(resultat, ensure_ascii=False))
    else:
        for k, s in resultat.items():
            if isinstance(s, dict):
                print(f"{k:14} médiane {s['mediane']:8.2f} ms   min {s['min']:8.2f}   p90 {s['p90']:8.2f}   n={s['n']}")
        print(f"échecs        {echecs}")
    return 0 if echecs == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
