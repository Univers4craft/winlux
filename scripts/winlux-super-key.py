#!/usr/bin/env python3
"""Touche Windows seule : ouvre (ou ferme) le menu Démarrer (#605).

Openbox ne sait lier une touche qu'à l'appui : une liaison « Super_L » se
déclenchait aussi au début de Windows+Gauche, Windows+Z, etc., d'où son
retrait du rc.xml. Ce démon écoute les événements bruts XInput 2 (livrés à
la racine même quand Openbox ou une application a saisi le clavier) et lance
winlux-start-menu quand la touche Windows est relâchée sans qu'aucune autre
touche ni aucun bouton de la souris n'ait été pressé entre-temps, comme sous
Windows. Le menu Démarrer est une application unique : un second appui le
referme.

    winlux-super-key        surveille l'affichage $DISPLAY
WINLUX_SUPER_KEY_COMMAND remplace la commande lancée (tests).
"""
import ctypes
import ctypes.util
import os
import shlex
import subprocess
import sys

XK_SUPER_L, XK_SUPER_R = 0xFFEB, 0xFFEC
GENERIC_EVENT = 35
XI_RAW_KEY_PRESS, XI_RAW_KEY_RELEASE, XI_RAW_BUTTON_PRESS = 13, 14, 15
XI_ALL_MASTER_DEVICES = 1
XI_KEY_REPEAT = 1 << 16


class XIEventMask(ctypes.Structure):
    _fields_ = [("deviceid", ctypes.c_int), ("mask_len", ctypes.c_int), ("mask", ctypes.POINTER(ctypes.c_ubyte))]


class XGenericEventCookie(ctypes.Structure):
    _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int),
                ("display", ctypes.c_void_p), ("extension", ctypes.c_int), ("evtype", ctypes.c_int),
                ("cookie", ctypes.c_uint), ("data", ctypes.c_void_p)]


class XEvent(ctypes.Union):
    _fields_ = [("type", ctypes.c_int), ("cookie", XGenericEventCookie), ("pad", ctypes.c_long * 24)]


class XIRawEvent(ctypes.Structure):
    # Début de la structure : seuls detail et flags servent.
    _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int),
                ("display", ctypes.c_void_p), ("extension", ctypes.c_int), ("evtype", ctypes.c_int),
                ("time", ctypes.c_ulong), ("deviceid", ctypes.c_int), ("sourceid", ctypes.c_int),
                ("detail", ctypes.c_int), ("flags", ctypes.c_int)]


class Tracker:
    """Appui-relâché de la touche Windows sans autre touche ni clic.
    is_super(code) dit si un code de touche porte Super_L ou Super_R."""

    def __init__(self, is_super):
        self.is_super = is_super
        self.down = set()
        self.armed = False

    def event(self, kind, detail, repeat=False):
        """Retourne True quand le menu Démarrer doit s'ouvrir ou se fermer."""
        if kind == XI_RAW_KEY_PRESS:
            if repeat: return False
            if self.is_super(detail):
                # Seule touche enfoncée : Windows seule possible.
                self.armed = not self.down
            else:
                self.armed = False
            self.down.add(detail)
        elif kind == XI_RAW_BUTTON_PRESS:
            self.armed = False
        elif kind == XI_RAW_KEY_RELEASE:
            self.down.discard(detail)
            if self.is_super(detail):
                fire = self.armed and not self.down
                self.armed = False
                return fire
        return False


def toggle_start():
    command = shlex.split(os.environ.get("WINLUX_SUPER_KEY_COMMAND", "winlux-start-menu"))
    try: subprocess.Popen(command, start_new_session=True, stdin=subprocess.DEVNULL)
    except OSError as error: print(f"winlux-super-key : {error}", file=sys.stderr, flush=True)


def single_instance():
    display = "".join(c if c.isalnum() else "_" for c in os.environ.get("DISPLAY", "none"))
    lock = os.path.join(os.environ.get("XDG_RUNTIME_DIR") or "/tmp", f"winlux-super-key-{display}-{os.getuid()}.pid")
    try:
        with open(lock) as handle: os.kill(int(handle.read().strip()), 0)
        return False
    except (OSError, ValueError):
        pass
    try:
        with open(lock, "w") as handle: handle.write(str(os.getpid()))
    except OSError:
        pass
    return True


def main():
    x11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
    xi = ctypes.CDLL(ctypes.util.find_library("Xi") or "libXi.so.6")
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x11.XkbKeycodeToKeysym.restype = ctypes.c_ulong
    x11.XkbKeycodeToKeysym.argtypes = [ctypes.c_void_p, ctypes.c_ubyte, ctypes.c_int, ctypes.c_int]
    x11.XQueryExtension.argtypes = [ctypes.c_void_p, ctypes.c_char_p] + [ctypes.POINTER(ctypes.c_int)] * 3
    x11.XNextEvent.argtypes = [ctypes.c_void_p, ctypes.POINTER(XEvent)]
    x11.XGetEventData.argtypes = [ctypes.c_void_p, ctypes.POINTER(XGenericEventCookie)]
    x11.XFreeEventData.argtypes = [ctypes.c_void_p, ctypes.POINTER(XGenericEventCookie)]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    xi.XIQueryVersion.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int)]
    xi.XISelectEvents.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(XIEventMask), ctypes.c_int]

    display = x11.XOpenDisplay(None)
    if not display:
        print("winlux-super-key : affichage introuvable", file=sys.stderr)
        return 1
    opcode, first_event, first_error = ctypes.c_int(), ctypes.c_int(), ctypes.c_int()
    if not x11.XQueryExtension(display, b"XInputExtension", ctypes.byref(opcode), ctypes.byref(first_event), ctypes.byref(first_error)):
        print("winlux-super-key : XInput absent", file=sys.stderr)
        return 1
    # 2.2 : les événements bruts arrivent même pendant une saisie du clavier.
    major, minor = ctypes.c_int(2), ctypes.c_int(2)
    if xi.XIQueryVersion(display, ctypes.byref(major), ctypes.byref(minor)) != 0 or (major.value, minor.value) < (2, 1):
        print("winlux-super-key : XInput 2.1 requis", file=sys.stderr)
        return 1
    if not single_instance(): return 0
    bits = (ctypes.c_ubyte * 4)()
    for kind in (XI_RAW_KEY_PRESS, XI_RAW_KEY_RELEASE, XI_RAW_BUTTON_PRESS):
        bits[kind >> 3] |= 1 << (kind & 7)
    mask = XIEventMask(XI_ALL_MASTER_DEVICES, len(bits), bits)
    xi.XISelectEvents(display, x11.XDefaultRootWindow(display), ctypes.byref(mask), 1)
    x11.XFlush(display)
    # Lu à chaque touche : la disposition peut changer (setxkbmap, clavier branché).
    tracker = Tracker(lambda code: 8 <= code <= 255 and x11.XkbKeycodeToKeysym(display, code, 0, 0) in (XK_SUPER_L, XK_SUPER_R))
    print("winlux-super-key : prêt", flush=True)
    event = XEvent()
    while True:
        x11.XNextEvent(display, ctypes.byref(event))
        cookie = event.cookie
        if event.type != GENERIC_EVENT or cookie.extension != opcode.value: continue
        if not x11.XGetEventData(display, ctypes.byref(cookie)): continue
        try:
            raw = ctypes.cast(cookie.data, ctypes.POINTER(XIRawEvent)).contents
            fire = tracker.event(cookie.evtype, raw.detail, bool(raw.flags & XI_KEY_REPEAT))
        finally:
            x11.XFreeEventData(display, ctypes.byref(cookie))
        if fire: toggle_start()


if __name__ == "__main__":
    sys.exit(main())
