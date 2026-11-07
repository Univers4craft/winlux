#!/usr/bin/python3
"""Coins carrés pour les fenêtres agrandies (session native, openbox + picom).

Windows 11 arrondit les fenêtres, sauf quand elles sont agrandies : elles
touchent alors les bords de l'écran et un coin arrondi laisserait voir le fond
d'écran aux quatre coins. picom 12 ne lit pas les propriétés de la fenêtre de
l'application dans ses règles (seulement celles du cadre d'openbox) : sa
règle ne peut donc pas tester _NET_WM_STATE. Ce veilleur recopie l'état
« agrandie » sur le cadre, sous la forme _WINLUX_MAXIMIZED = 1, et la règle
de ~/.config/picom/picom.conf met alors corner-radius à 0.

    winlux-maximized-watch          veille (lancé par la session)
    winlux-maximized-watch --once   synchronise une fois puis quitte (tests)
"""
import ctypes as C
import sys

X = C.CDLL("libX11.so.6")
ulong = C.c_ulong
X.XOpenDisplay.restype = C.c_void_p
X.XOpenDisplay.argtypes = [C.c_char_p]
X.XDefaultRootWindow.restype = ulong
X.XDefaultRootWindow.argtypes = [C.c_void_p]
X.XInternAtom.restype = ulong
X.XInternAtom.argtypes = [C.c_void_p, C.c_char_p, C.c_int]
X.XSelectInput.argtypes = [C.c_void_p, ulong, C.c_long]
X.XGetWindowProperty.argtypes = [C.c_void_p, ulong, ulong, C.c_long, C.c_long, C.c_int, ulong,
                                 C.POINTER(ulong), C.POINTER(C.c_int), C.POINTER(ulong),
                                 C.POINTER(ulong), C.POINTER(C.c_void_p)]
X.XChangeProperty.argtypes = [C.c_void_p, ulong, ulong, ulong, C.c_int, C.c_int, C.c_void_p, C.c_int]
X.XDeleteProperty.argtypes = [C.c_void_p, ulong, ulong]
X.XQueryTree.argtypes = [C.c_void_p, ulong, C.POINTER(ulong), C.POINTER(ulong),
                         C.POINTER(C.c_void_p), C.POINTER(C.c_uint)]
X.XFree.argtypes = [C.c_void_p]
X.XNextEvent.argtypes = [C.c_void_p, C.c_void_p]
X.XFlush.argtypes = [C.c_void_p]
X.XSync.argtypes = [C.c_void_p, C.c_int]

# Une fenêtre détruite entre deux requêtes donne BadWindow : sans ce
# gestionnaire, Xlib quitterait le programme.
ERROR_HANDLER = C.CFUNCTYPE(C.c_int, C.c_void_p, C.c_void_p)(lambda d, e: 0)
X.XSetErrorHandler(ERROR_HANDLER)

PROPERTY_CHANGE_MASK = 1 << 22
PROPERTY_NOTIFY = 28
XA_ATOM, XA_CARDINAL, XA_WINDOW = 4, 6, 33


def main():
    display = X.XOpenDisplay(None)
    if not display:
        sys.exit("winlux-maximized-watch : affichage X introuvable")
    root = X.XDefaultRootWindow(display)

    def atom(name):
        return X.XInternAtom(display, name.encode(), 0)

    client_list = atom("_NET_CLIENT_LIST")
    wm_state = atom("_NET_WM_STATE")
    max_vert = atom("_NET_WM_STATE_MAXIMIZED_VERT")
    max_horz = atom("_NET_WM_STATE_MAXIMIZED_HORZ")
    marker = atom("_WINLUX_MAXIMIZED")

    def read_list(window, prop, kind):
        real_type, real_format = ulong(), C.c_int()
        count, after, data = ulong(), ulong(), C.c_void_p()
        if X.XGetWindowProperty(display, window, prop, 0, 1024, 0, kind, C.byref(real_type),
                                C.byref(real_format), C.byref(count), C.byref(after),
                                C.byref(data)) != 0 or not data:
            return []
        try:
            if real_format.value != 32:
                return []
            return list(C.cast(data, C.POINTER(ulong))[:count.value])
        finally:
            X.XFree(data)

    def frame_of(window):
        # Le cadre d'openbox est l'ancêtre direct de la racine.
        current = window
        for _ in range(16):
            root_ret, parent = ulong(), ulong()
            children, count = C.c_void_p(), C.c_uint()
            if not X.XQueryTree(display, current, C.byref(root_ret), C.byref(parent),
                                C.byref(children), C.byref(count)):
                return None
            if children:
                X.XFree(children)
            if parent.value in (0, root):
                return current
            current = parent.value
        return None

    marked = {}

    def update(window):
        states = read_list(window, wm_state, XA_ATOM)
        maximized = max_vert in states and max_horz in states
        frame = frame_of(window)
        if frame is None or frame == window:
            return
        if marked.get(window) == (frame, maximized):
            return
        if maximized:
            one = ulong(1)
            X.XChangeProperty(display, frame, marker, XA_CARDINAL, 32, 0, C.byref(one), 1)
        else:
            X.XDeleteProperty(display, frame, marker)
        marked[window] = (frame, maximized)

    watched = set()

    def rescan():
        clients = read_list(root, client_list, XA_WINDOW)
        for window in clients:
            if window not in watched:
                X.XSelectInput(display, window, PROPERTY_CHANGE_MASK)
                watched.add(window)
            update(window)
        for window in list(watched):
            if window not in clients:
                watched.discard(window)
                marked.pop(window, None)

    X.XSelectInput(display, root, PROPERTY_CHANGE_MASK)
    rescan()
    X.XSync(display, 0)
    if "--once" in sys.argv:
        return

    event = (C.c_long * 24)()
    while True:
        X.XNextEvent(display, event)
        if event[0] & 0x7f != PROPERTY_NOTIFY:
            continue
        window, prop = ulong(event[4]).value, ulong(event[5]).value
        if window == root and prop == client_list:
            rescan()
        elif prop == wm_state and window in watched:
            update(window)
        X.XFlush(display)


if __name__ == "__main__":
    main()
