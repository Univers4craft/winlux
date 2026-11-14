#!/usr/bin/python3
"""Écran de blocage du contrôle parental sur une session gelée (#2244).

Au deuxième écran fermé de force, winlux-parental gèle (SIGSTOP) tous les
processus du compte surveillé, dont son bus d'accessibilité. L'écran relancé
ensuite ne doit pas l'attendre : il doit couvrir l'écran en quelques secondes.

Le test lance son propre Xvfb, un faux bus d'accessibilité « gelé » (il accepte la
connexion et ne répond jamais) annoncé par AT_SPI_BUS, comme le fait la
session du compte, puis :
  1. témoin : une fenêtre GTK ordinaire reste bloquée (sinon le test ne
     prouverait rien sur ce système) ;
  2. « winlux-parental screen » affiche sa fenêtre plein écran.
Sans root : racine factice (WINLUX_PARENTAL_ROOT) ; Xlib par ctypes.
"""
import ctypes
import ctypes.util
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-parental.py"
# Le Python du système porte PyGObject et GTK 3 (python3-gi, gir1.2-gtk-3.0).
PYGTK = os.environ.get("WINLUX_TEST_PYGTK", "/usr/bin/python3")
WIDTH, HEIGHT = 1024, 768
failures = 0


def require(tools, pygtk, modules):
    """Prérequis du test : chacun manquant est nommé avec son paquet, puis le
    test échoue sans aucun autre contrôle ; il n'est jamais sauté (#2296)."""
    missing = [f"{tool} (paquet {package})" for tool, package in tools if not shutil.which(tool)]
    code = "import gi\n" + "".join(f"gi.require_version({name!r}, {version!r})\n" for name, version in modules) + \
        "".join(f"from gi.repository import {name}\n" for name, _version in modules)
    try:
        probe = subprocess.run([pygtk, "-I", "-c", code], capture_output=True, timeout=60).returncode
    except (OSError, subprocess.SubprocessError):
        probe = 1
    if probe != 0:
        names = ", ".join(f"{name} {version}" for name, version in modules)
        missing.append(f"PyGObject ({names}) pour {pygtk} (paquets python3-gi, gir1.2-gtk-3.0 ; "
                       "ou WINLUX_TEST_PYGTK=un Python qui les a)")
    if missing:
        print("ÉCHEC prérequis manquant : " + " ; ".join(missing), flush=True)
        sys.exit(1)


def check(cond, label):
    global failures
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures += 1


require([("Xvfb", "xvfb")], PYGTK, [("Gtk", "3.0")])

work = Path(tempfile.mkdtemp(prefix="winlux-parental-gel."))
number = 90 + os.getpid() % 300
while Path(f"/tmp/.X11-unix/X{number}").exists() or Path(f"/tmp/.X{number}-lock").exists():
    number += 1
display = f":{number}"
xvfb = subprocess.Popen(["Xvfb", display, "-noreset", "-nolisten", "tcp", "-screen", "0", f"{WIDTH}x{HEIGHT}x24"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
children = [xvfb]
env = {"PATH": "/usr/bin:/bin", "HOME": str(work), "DISPLAY": display, "LANG": "C.UTF-8"}

# Faux bus d'accessibilité gelé : la connexion aboutit, rien ne répond.
bus_path = work / "a11y-bus"
bus = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
bus.bind(str(bus_path))
bus.listen(16)
accepted = []


def serve():
    while True:
        try:
            accepted.append(bus.accept()[0])
        except OSError:
            return


threading.Thread(target=serve, daemon=True).start()


class XWindowAttributes(ctypes.Structure):
    _fields_ = [("x", ctypes.c_int), ("y", ctypes.c_int), ("width", ctypes.c_int), ("height", ctypes.c_int),
                ("border_width", ctypes.c_int), ("depth", ctypes.c_int), ("visual", ctypes.c_void_p),
                ("root", ctypes.c_ulong), ("klass", ctypes.c_int), ("bit_gravity", ctypes.c_int),
                ("win_gravity", ctypes.c_int), ("backing_store", ctypes.c_int), ("backing_planes", ctypes.c_ulong),
                ("backing_pixel", ctypes.c_ulong), ("save_under", ctypes.c_int), ("colormap", ctypes.c_ulong),
                ("map_installed", ctypes.c_int), ("map_state", ctypes.c_int), ("all_event_masks", ctypes.c_long),
                ("your_event_mask", ctypes.c_long), ("do_not_propagate_mask", ctypes.c_long),
                ("override_redirect", ctypes.c_int), ("screen", ctypes.c_void_p)]


# Xlib par ctypes (libX11 vient avec GTK) : pas besoin de xprop ni de xwininfo.
xlib = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
xlib.XOpenDisplay.restype = ctypes.c_void_p
xlib.XOpenDisplay.argtypes = [ctypes.c_char_p]
xlib.XDefaultRootWindow.restype = ctypes.c_ulong
xlib.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
xlib.XInternAtom.restype = ctypes.c_ulong
xlib.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
xlib.XChangeProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int,
                                 ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
xlib.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
                            ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
                            ctypes.POINTER(ctypes.c_uint)]
xlib.XGetWindowAttributes.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(XWindowAttributes)]
xlib.XFree.argtypes = [ctypes.c_void_p]
xlib.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
IS_VIEWABLE = 2


def open_display():
    for _ in range(100):
        handle = xlib.XOpenDisplay(display.encode())
        if handle:
            return handle
        time.sleep(0.1)
    return None


def mapped_windows(handle):
    """[(fenêtre, largeur, hauteur)] des fenêtres affichées, enfants de la racine."""
    root_window, parent = ctypes.c_ulong(), ctypes.c_ulong()
    kids, count = ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
    xlib.XSync(handle, 0)
    if not xlib.XQueryTree(handle, xlib.XDefaultRootWindow(handle), ctypes.byref(root_window),
                           ctypes.byref(parent), ctypes.byref(kids), ctypes.byref(count)):
        return []
    found = []
    for index in range(count.value):
        attributes = XWindowAttributes()
        if xlib.XGetWindowAttributes(handle, kids[index], ctypes.byref(attributes)) and \
                attributes.map_state == IS_VIEWABLE:
            found.append((hex(kids[index]), attributes.width, attributes.height))
    if kids:
        xlib.XFree(kids)
    return found


def wait_fullscreen(handle, seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if any(w >= WIDTH and h >= HEIGHT for _, w, h in mapped_windows(handle)):
            return True
        time.sleep(0.25)
    return False


try:
    handle = open_display()
    check(handle is not None, f"Xvfb répond sur {display}")
    if handle is None:
        raise SystemExit(1)
    address = f"unix:path={bus_path}".encode()
    xlib.XChangeProperty(handle, xlib.XDefaultRootWindow(handle), xlib.XInternAtom(handle, b"AT_SPI_BUS", 0),
                         xlib.XInternAtom(handle, b"STRING", 0), 8, 0, address, len(address))
    xlib.XSync(handle, 0)

    # 1. Témoin : une fenêtre GTK ordinaire attend le bus gelé et ne s'affiche pas.
    probe = work / "temoin.py"
    probe.write_text(
        "import gi\n"
        "gi.require_version('Gtk', '3.0')\n"
        "from gi.repository import Gtk\n"
        "w = Gtk.Window(type=Gtk.WindowType.POPUP)\n"
        f"w.set_default_size({WIDTH}, {HEIGHT})\n"
        "w.show_all()\n"
        "Gtk.main()\n", encoding="utf-8")
    witness = subprocess.Popen([PYGTK, "-I", "-B", str(probe)], env=env, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL)
    children.append(witness)
    stuck = not wait_fullscreen(handle, 6)
    check(stuck and bool(accepted), "témoin : une fenêtre GTK ordinaire reste bloquée sur le bus gelé "
                              f"({len(accepted)} connexion(s) au faux bus)")
    witness.kill()
    witness.wait()

    # 2. L'écran de blocage, lui, s'affiche malgré le bus gelé.
    root = work / "racine"
    root.mkdir()
    screen_env = dict(env, WINLUX_PARENTAL_ROOT=str(root), WINLUX_LANG="fr")
    screen = subprocess.Popen([PYGTK, "-B", str(TOOL), "screen", "enfant", display], env=screen_env,
                              stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    children.append(screen)
    shown = wait_fullscreen(handle, 10)
    check(shown, "écran de blocage affiché en plein écran malgré le bus d'accessibilité gelé (#2244)")
    if not shown:
        print("  fenêtres affichées :", mapped_windows(handle))
        if screen.poll() is not None:
            print("  écran terminé :", screen.returncode, screen.stderr.read().decode(errors="replace")[-2000:])
finally:
    for child in reversed(children):
        try:
            child.kill()
            child.wait(timeout=5)
        except (OSError, subprocess.SubprocessError):
            pass
    bus.close()
    shutil.rmtree(work, ignore_errors=True)

print("test-parental-ecran-gel :", "réussi" if failures == 0 else f"{failures} échec(s)")
sys.exit(1 if failures else 0)
