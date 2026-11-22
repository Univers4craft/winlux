#!/usr/bin/env python3
"""Sonde de vitesse de l'Explorateur Winlux (shell PE, gui_demo.exe), vue de X11.

Chronomètre DEHORS chaque geste de l'Explorateur : l'heure du geste (XTest)
puis la première et la dernière zone repeinte de sa fenêtre, signalées par
l'extension DAMAGE du serveur X. Aucune lecture d'écran (XGetImage) pendant
une mesure : la sonde n'occupe pas le serveur X, elle écoute ses événements.
Une seule capture est prise, à la fin du geste des vignettes, hors chrono.

Pour chaque geste : médiane, minimum, maximum sur N essais de
  - « 1re » : geste -> première zone repeinte (réaction perçue) ;
  - « fin » : geste -> dernière zone repeinte avant QUIET ms sans peinture ;
  - CPU de l'Explorateur (/proc/<pid>/task/*/schedstat, ns), CPU de Xorg et
    de picom quand /proc les laisse lire, appels système de lecture/écriture
    (/proc/<pid>/io), changements de contexte volontaires.

Tout se passe dans un dossier de travail jetable (--travail, par défaut
/tmp/sonde-explorateur-<uid>) et, par défaut, avec un HOME jetable : les
réglages, les fichiers récents et les vignettes du compte ne sont pas touchés.
--home-reel ne fait que les gestes en lecture seule (ouvrir, flèches,
Échap, fermer) avec le vrai dossier personnel.

Sécurité : avant chaque touche, la sonde vérifie que le focus clavier est dans
la fenêtre de l'Explorateur qu'elle a lancée ; sinon elle s'arrête sans rien
taper. Elle n'envoie jamais Suppr. Elle tue l'Explorateur par son pid.

Usage : sonde-explorateur.py --exe GUI_DEMO.exe [--runtime WINLUX] [--essais 5]
        [--gros 5000] [--json SORTIE.json] [--home-reel]
"""
import argparse
import ctypes
import ctypes.util
import json
import os
import select
import shutil
import signal
import statistics
import struct
import subprocess
import sys
import time
import zlib

CreateNotify, MapNotify, PropertyNotify, KeyPress = 16, 19, 28, 2
SubstructureNotifyMask, StructureNotifyMask, PropertyChangeMask = 1 << 19, 1 << 17, 1 << 22

XK = {"Down": 0xFF54, "Up": 0xFF52, "Return": 0xFF0D, "BackSpace": 0xFF08, "Escape": 0xFF1B,
      "F2": 0xFFBF, "F5": 0xFFC2, "Control_L": 0xFFE3, "End": 0xFF57, "Home": 0xFF50,
      "Next": 0xFF56}


class X:
    def __init__(self):
        self.x = ctypes.CDLL(ctypes.util.find_library("X11"))
        self.t = ctypes.CDLL(ctypes.util.find_library("Xtst"))
        self.d = ctypes.CDLL(ctypes.util.find_library("Xdamage") or "libXdamage.so.1")
        x = self.x
        x.XOpenDisplay.restype = ctypes.c_void_p
        x.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x.XDefaultRootWindow.restype = ctypes.c_ulong
        x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        x.XSelectInput.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_long]
        x.XNextEvent.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        x.XPending.argtypes = [ctypes.c_void_p]
        x.XFlush.argtypes = [ctypes.c_void_p]
        x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        x.XConnectionNumber.argtypes = [ctypes.c_void_p]
        x.XInternAtom.restype = ctypes.c_ulong
        x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
        x.XGetWindowProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long,
                                         ctypes.c_long, ctypes.c_int, ctypes.c_ulong,
                                         ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int),
                                         ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong),
                                         ctypes.POINTER(ctypes.c_void_p)]
        x.XFree.argtypes = [ctypes.c_void_p]
        x.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        x.XKeysymToKeycode.restype = ctypes.c_ubyte
        x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        x.XGetInputFocus.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int)]
        x.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
                                 ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
                                 ctypes.POINTER(ctypes.c_uint)]
        x.XTranslateCoordinates.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int,
                                            ctypes.c_int, ctypes.POINTER(ctypes.c_int),
                                            ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong)]
        x.XGetGeometry.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong)] + \
            [ctypes.POINTER(ctypes.c_int)] * 2 + [ctypes.POINTER(ctypes.c_uint)] * 4
        x.XGetImage.restype = ctypes.c_void_p
        x.XGetImage.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint,
                                ctypes.c_uint, ctypes.c_ulong, ctypes.c_int]
        x.XGetPixel.restype = ctypes.c_ulong
        x.XGetPixel.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
        x.XDestroyImage = None
        self.t.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        self.t.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        self.t.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                                ctypes.c_ulong]
        self.d.XDamageQueryExtension.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_int),
                                                 ctypes.POINTER(ctypes.c_int)]
        self.d.XDamageCreate.restype = ctypes.c_ulong
        self.d.XDamageCreate.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int]
        self.dpy = x.XOpenDisplay(None)
        if not self.dpy:
            sys.exit("sonde : pas d'affichage X (DISPLAY)")
        self.root = x.XDefaultRootWindow(self.dpy)
        ev, er = ctypes.c_int(), ctypes.c_int()
        if not self.d.XDamageQueryExtension(self.dpy, ctypes.byref(ev), ctypes.byref(er)):
            sys.exit("sonde : extension DAMAGE absente")
        self.damage_event = ev.value  # XDamageNotify = base + 0
        # Une fenêtre créée puis détruite aussitôt (cadre d'openbox…) rend
        # BadWindow sur XSelectInput : le gestionnaire Xlib par défaut tuait
        # la sonde (banc 2, 22 nov.). Ces erreurs sont ignorées.
        self._handler = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)(lambda d, e: 0)
        x.XSetErrorHandler(self._handler)
        self.fd = x.XConnectionNumber(self.dpy)
        self.buf = ctypes.create_string_buffer(192)
        x.XSelectInput(self.dpy, self.root, SubstructureNotifyMask)
        x.XFlush(self.dpy)
        self.atoms = {}

    def atom(self, name):
        if name not in self.atoms:
            self.atoms[name] = self.x.XInternAtom(self.dpy, name.encode(), 0)
        return self.atoms[name]

    def prop(self, win, name):
        t, f, n, after = ctypes.c_ulong(), ctypes.c_int(), ctypes.c_ulong(), ctypes.c_ulong()
        data = ctypes.c_void_p()
        if self.x.XGetWindowProperty(self.dpy, win, self.atom(name), 0, 1024, 0, 0, ctypes.byref(t),
                                     ctypes.byref(f), ctypes.byref(n), ctypes.byref(after),
                                     ctypes.byref(data)) != 0 or not data.value:
            return None
        try:
            if f.value == 32:
                return list((ctypes.c_ulong * n.value).from_address(data.value))
            return ctypes.string_at(data.value, n.value * f.value // 8)
        finally:
            self.x.XFree(data)

    def events(self, timeout):
        """Événements disponibles avant `timeout` s : liste (heure, type, a, b)."""
        out = []
        if not self.x.XPending(self.dpy):
            r, _, _ = select.select([self.fd], [], [], max(0.0, timeout))
            if not r and not self.x.XPending(self.dpy):
                return out
        while self.x.XPending(self.dpy):
            self.x.XNextEvent(self.dpy, self.buf)
            now = time.monotonic()
            typ = struct.unpack_from("i", self.buf, 0)[0]
            a, b = struct.unpack_from("QQ", self.buf, 32)
            out.append((now, typ, a, b))
        return out

    def key(self, name, ctrl=False):
        code = self.x.XKeysymToKeycode(self.dpy, XK.get(name) or ord(name))
        ctl = self.x.XKeysymToKeycode(self.dpy, XK["Control_L"])
        if ctrl:
            self.t.XTestFakeKeyEvent(self.dpy, ctl, 1, 0)
        self.t.XTestFakeKeyEvent(self.dpy, code, 1, 0)
        self.t.XTestFakeKeyEvent(self.dpy, code, 0, 0)
        if ctrl:
            self.t.XTestFakeKeyEvent(self.dpy, ctl, 0, 0)
        self.x.XFlush(self.dpy)

    def move(self, x, y):
        self.t.XTestFakeMotionEvent(self.dpy, -1, x, y, 0)
        self.x.XFlush(self.dpy)

    def button(self, n):
        self.t.XTestFakeButtonEvent(self.dpy, n, 1, 0)
        self.t.XTestFakeButtonEvent(self.dpy, n, 0, 0)
        self.x.XFlush(self.dpy)

    def geometry(self, win):
        rx, ry, child = ctypes.c_int(), ctypes.c_int(), ctypes.c_ulong()
        self.x.XTranslateCoordinates(self.dpy, win, self.root, 0, 0, ctypes.byref(rx), ctypes.byref(ry),
                                     ctypes.byref(child))
        r = ctypes.c_ulong()
        gx, gy = ctypes.c_int(), ctypes.c_int()
        w, h, bw, depth = ctypes.c_uint(), ctypes.c_uint(), ctypes.c_uint(), ctypes.c_uint()
        self.x.XGetGeometry(self.dpy, win, ctypes.byref(r), ctypes.byref(gx), ctypes.byref(gy), ctypes.byref(w),
                            ctypes.byref(h), ctypes.byref(bw), ctypes.byref(depth))
        return rx.value, ry.value, w.value, h.value

    def focus_inside(self, win):
        f, rev = ctypes.c_ulong(), ctypes.c_int()
        self.x.XGetInputFocus(self.dpy, ctypes.byref(f), ctypes.byref(rev))
        w = f.value
        for _ in range(16):
            if w == win:
                return True
            if w in (0, 1, self.root):
                return False
            root, parent = ctypes.c_ulong(), ctypes.c_ulong()
            kids, n = ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
            if not self.x.XQueryTree(self.dpy, w, ctypes.byref(root), ctypes.byref(parent), ctypes.byref(kids),
                                     ctypes.byref(n)):
                return False
            if kids:
                self.x.XFree(kids)
            w = parent.value
        return False

    def capture_png(self, win, path):
        _, _, w, h = self.geometry(win)
        img = self.x.XGetImage(self.dpy, win, 0, 0, w, h, 0xFFFFFFFF, 2)
        if not img:
            return None
        rows = []
        dark = 0
        for y in range(h):
            row = bytearray(b"\0")
            for xx in range(w):
                p = self.x.XGetPixel(img, xx, y)
                r, g, b = (p >> 16) & 255, (p >> 8) & 255, p & 255
                row += bytes((r, g, b))
            rows.append(bytes(row))
        raw = b"".join(rows)

        def chunk(t, d):
            c = struct.pack(">I", len(d)) + t + d
            return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
        png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + \
            chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
        with open(path, "wb") as f:
            f.write(png)
        return dark


def schedstat(pid):
    """Temps CPU (ns) de tous les fils d'un processus, ou None."""
    total = 0
    try:
        for t in os.listdir(f"/proc/{pid}/task"):
            with open(f"/proc/{pid}/task/{t}/schedstat") as f:
                total += int(f.read().split()[0])
    except OSError:
        return None
    return total


def io(pid):
    out = {}
    try:
        with open(f"/proc/{pid}/io") as f:
            for line in f:
                k, v = line.split(":")
                out[k] = int(v)
        out["ctxsw"] = 0
        for t in os.listdir(f"/proc/{pid}/task"):
            with open(f"/proc/{pid}/task/{t}/status") as f:
                for line in f:
                    if line.startswith("voluntary_ctxt_switches") or line.startswith("nonvoluntary_ctxt_switches"):
                        out["ctxsw"] += int(line.split()[1])
    except OSError:
        pass
    return out


def cpu_mhz():
    """Fréquence moyenne et maximale des cœurs juste avant un geste (MHz)."""
    try:
        with open("/proc/cpuinfo") as f:
            v = [float(l.split(":")[1]) for l in f if l.startswith("cpu MHz")]
        return [round(sum(v) / len(v)), round(max(v))] if v else None
    except OSError:
        return None


def pid_of(comm):
    for p in os.listdir("/proc"):
        if p.isdigit():
            try:
                with open(f"/proc/{p}/comm") as f:
                    if f.read().strip() == comm:
                        return int(p)
            except OSError:
                pass
    return None


def png_file(path, w, h, seed):
    """Image PNG de test (dégradé), écrite sans bibliothèque."""
    rows = []
    for y in range(h):
        row = bytearray(b"\0")
        v = (y * 255 // h + seed * 37) & 255
        for xx in range(w):
            row += bytes(((xx * 255 // w) & 255, v, (seed * 53) & 255))
        rows.append(bytes(row))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + \
        chunk(b"IDAT", zlib.compress(b"".join(rows), 1)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(data)


def prepare(work, gros):
    if os.path.exists(work):
        shutil.rmtree(work)
    os.makedirs(work)
    petit = os.path.join(work, "petit")
    os.makedirs(os.path.join(petit, "sous", "plus-bas"))
    for i in range(12):
        with open(os.path.join(petit, f"note-{i:02}.txt"), "w") as f:
            f.write("Winlux " * (i + 1))
    for i in range(5):
        with open(os.path.join(petit, "sous", f"doc-{i}.txt"), "w") as f:
            f.write("x")
    big = os.path.join(work, "gros")
    os.makedirs(big)
    for i in range(gros):
        open(os.path.join(big, f"fichier-{i:05}.txt"), "w").close()
    imgs = os.path.join(work, "images")
    os.makedirs(imgs)
    for i in range(40):
        png_file(os.path.join(imgs, f"photo-{i:02}.png"), 640, 480, i)
    home = os.path.join(work, "home")
    os.makedirs(home)
    return petit, big, imgs, home


def windows_path(p):
    return "Z:" + p.replace("/", "\\")


class Explorer:
    def __init__(self, x, args, home, folder):
        self.x = x
        env = dict(os.environ)
        env.update(WINLUX_NATIVE_SHELL="1", WINLUX_SHELL_APP="explorer", WINLUX_RUNTIME=args.runtime)
        env.pop("WINLUX_DESKTOP_DEMO", None)
        env.pop("WINLUX_EXPLORATEUR_SANS", None)
        if getattr(args, "sans", None):
            env["WINLUX_EXPLORATEUR_SANS"] = args.sans
        if folder:
            env["WINLUX_SHELL_PATH"] = windows_path(folder)
        if home:
            env["HOME"] = home
            env["XDG_CACHE_HOME"] = os.path.join(home, ".cache")
            env["XDG_CONFIG_HOME"] = os.path.join(home, ".config")
            env["XDG_DATA_HOME"] = os.path.join(home, ".local", "share")
            env["XDG_STATE_HOME"] = os.path.join(home, ".local", "state")
        self.t0 = time.monotonic()
        self.proc = subprocess.Popen([args.runtime, args.exe], env=env, stdout=subprocess.DEVNULL,
                                     stderr=subprocess.DEVNULL, start_new_session=True)
        self.win = None
        self.created = None
        self.mapped = None
        self.first = None
        self.last = None
        self.candidates = {}
        self.mine_set = set()
        self.others = set()

    def wait_open(self, quiet, limit=20.0):
        """Fenêtre créée, visible, première et dernière peinture du lancement."""
        x = self.x
        deadline = self.t0 + limit
        while True:
            now = time.monotonic()
            if now > deadline:
                break
            if self.last and now - self.last > quiet:
                break
            for t, typ, a, b in x.events(0.02):
                if typ == CreateNotify and a == x.root:
                    win = b
                    x.x.XSelectInput(x.dpy, win, StructureNotifyMask | PropertyChangeMask)
                    x.d.XDamageCreate(x.dpy, win, 0)
                    x.x.XFlush(x.dpy)
                    self.candidates[win] = t
                elif typ == MapNotify and b in self.candidates:
                    if self.win is None and self.mine(b):
                        self.win, self.created, self.mapped = b, self.candidates[b], t
                        self.mine_set.add(b)
                elif typ == x.damage_event and a in self.candidates:
                    if self.win is None and self.mine(a):
                        self.win, self.created = a, self.candidates[a]
                    if a == self.win:
                        self.mine_set.add(a)
                    if a == self.win:
                        self.first = self.first or t
                        self.last = t
        return self.win is not None

    def mine(self, win):
        pid = self.x.prop(win, "_NET_WM_PID")
        return bool(pid) and pid[0] == self.proc.pid

    def ms(self, t):
        return None if t is None else round((t - self.t0) * 1000, 1)

    def result(self):
        io_ = io(self.proc.pid)
        return {"premiere": self.ms(self.first), "fin": self.ms(self.last), "visible": self.ms(self.mapped),
                "cpu_explorateur": None if schedstat(self.proc.pid) is None
                else round(schedstat(self.proc.pid) / 1e6, 1),
                "cpu_autres": [None], "lectures": io_.get("syscr"), "ecritures": io_.get("syscw")}

    def close(self):
        if self.proc.poll() is None:
            os.kill(self.proc.pid, signal.SIGTERM)
            try:
                self.proc.wait(3)
            except subprocess.TimeoutExpired:
                os.kill(self.proc.pid, signal.SIGKILL)
                self.proc.wait()
        # Purge les événements de la fenêtre fermée.
        self.x.events(0.05)


def gesture(x, ex, action, quiet, limit=5.0, others=()):
    """Exécute `action` et renvoie (1re ms, fin ms, cpu explorateur ms, cpu X ms, io)."""
    x.events(0)  # vide la file avant le geste
    mhz = cpu_mhz()
    c0 = schedstat(ex.proc.pid)
    o0 = [schedstat(p) if p else None for p in others]
    i0 = io(ex.proc.pid)
    t0 = time.monotonic()
    action()
    first = last = None
    n = 0
    while True:
        now = time.monotonic()
        if now - t0 > limit or (last and now - last > quiet):
            break
        for t, typ, a, b in x.events(0.01):
            if typ == CreateNotify and a == x.root:
                x.x.XSelectInput(x.dpy, b, StructureNotifyMask)
                x.d.XDamageCreate(x.dpy, b, 0)
                x.x.XFlush(x.dpy)
                ex.candidates[b] = t
            elif typ == x.damage_event and a in ex.candidates and a not in ex.others:
                if a == ex.win or ex.mine(a):
                    ex.mine_set.add(a)
                else:
                    ex.others.add(a)
            if typ == x.damage_event and a in ex.mine_set:
                first = first or t
                last = t
                n += 1
    c1 = schedstat(ex.proc.pid)
    o1 = [schedstat(p) if p else None for p in others]
    i1 = io(ex.proc.pid)
    d = lambda a, b: None if a is None or b is None else round((b - a) / 1e6, 2)
    return {
        "premiere": None if first is None else round((first - t0) * 1000, 2),
        "fin": None if last is None else round((last - t0) * 1000, 2),
        "zones": n,
        "cpu_explorateur": d(c0, c1),
        "cpu_autres": [d(a, b) for a, b in zip(o0, o1)],
        "lectures": i1.get("syscr", 0) - i0.get("syscr", 0),
        "ecritures": i1.get("syscw", 0) - i0.get("syscw", 0),
        "ctxsw": i1.get("ctxsw", 0) - i0.get("ctxsw", 0),
        "mhz": mhz,
    }


def summary(rows, key):
    vals = [r[key] for r in rows if r.get(key) is not None]
    if not vals:
        return "—"
    return f"{statistics.median(vals):.1f} (min {min(vals):.1f}, max {max(vals):.1f})"


def court(x, args, others, results, line, safe, petit, big, home):
    """Passe ciblée : gestes courts répétés, chacun suivi de son retour à l'état
    de départ, avec une pause de 0,6 s (pas de double-clic involontaire)."""
    n, q = args.essais, 0.12
    ex = Explorer(x, args, home, petit)
    if not ex.wait_open(0.4):
        sys.exit("sonde : fenêtre de l'Explorateur introuvable")
    time.sleep(1.0)
    gx, gy, gw, gh = x.geometry(ex.win)

    def rep(name, before, action, after, quiet=q, limit=5.0):
        rows = []
        for _ in range(n):
            if before:
                before()
            time.sleep(0.6)
            rows.append(gesture(x, ex, safe(ex, action), quiet, limit=limit, others=others))
            if after:
                time.sleep(0.2)
                gesture(x, ex, safe(ex, after), q)
        line(name, rows)

    rep("flèche Bas (petit)", lambda: gesture(x, ex, safe(ex, lambda: x.key("Home")), q),
        lambda: x.key("Down"), None)
    rep("F2, champ de renommage", lambda: gesture(x, ex, safe(ex, lambda: x.key("End")), q),
        lambda: x.key("F2"), lambda: x.key("Escape"))

    def to_list():
        x.move(gx + gw // 2 + 60, gy + 220)
        time.sleep(0.1)
    rep("clic droit, menu", to_list, lambda: x.button(3), lambda: x.key("Escape"))

    def into_sub():
        gesture(x, ex, safe(ex, lambda: x.key("Home")), q)
        gesture(x, ex, safe(ex, lambda: x.key("Return")), q)
        x.move(gx + 100, gy + 60)
        time.sleep(0.1)
        x.move(gx + 103, gy + 64)
    rep("dossier parent (bouton Haut)", into_sub, lambda: x.button(1), None)
    ex.close()

    ex = Explorer(x, args, home, big)
    ex.wait_open(0.4)
    time.sleep(1.0)
    gx, gy, gw, gh = x.geometry(ex.win)
    x.move(gx + gw // 2 + 60, gy + 300)
    time.sleep(0.3)
    rep("molette, un cran (défilement terminé)", lambda: gesture(x, ex, safe(ex, lambda: x.key("Home")), 0.7),
        lambda: x.button(5), None, quiet=0.7, limit=6.0)
    time.sleep(1.0)
    c0, o0, i0, t0 = schedstat(ex.proc.pid), [schedstat(p) if p else None for p in others], io(ex.proc.pid), \
        time.monotonic()
    time.sleep(10.0)
    c1, o1, i1, t1 = schedstat(ex.proc.pid), [schedstat(p) if p else None for p in others], io(ex.proc.pid), \
        time.monotonic()
    dur = t1 - t0
    rest = {"cpu_explorateur_pct": None if c0 is None else round((c1 - c0) / 1e7 / dur, 3),
            "reveils_par_s": round((i1.get("ctxsw", 0) - i0.get("ctxsw", 0)) / dur, 1)}
    results["repos"] = rest
    print(f"au repos (10 s)                    CPU expl. {rest['cpu_explorateur_pct']} %, "
          f"réveils {rest['reveils_par_s']}/s", flush=True)
    ex.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", required=True)
    ap.add_argument("--runtime", default=shutil.which("winlux") or "/usr/bin/winlux")
    ap.add_argument("--essais", type=int, default=5)
    ap.add_argument("--gros", type=int, default=5000)
    ap.add_argument("--travail", default=f"/tmp/sonde-explorateur-{os.getuid()}")
    ap.add_argument("--json")
    ap.add_argument("--capture")
    ap.add_argument("--home-reel", action="store_true")
    ap.add_argument("--court", action="store_true",
                    help="gestes courts seulement, --essais fois chacun (passe ciblée)")
    ap.add_argument("--sans", help="prototype : modifications retirées (lettres w, t, p, m)")
    ap.add_argument("--forcer-focus", action="store_true",
                    help="Xvfb sans gestionnaire de fenêtres : donne le focus à l'Explorateur")
    args = ap.parse_args()
    args.exe = os.path.abspath(args.exe)
    args.runtime = os.path.abspath(args.runtime)
    x = X()
    others = [pid_of("Xorg") or pid_of("Xvfb"), pid_of("picom")]
    results = {"essais": args.essais, "autres": ["Xorg/Xvfb", "picom"]}

    def line(name, rows, key="fin"):
        results[name] = rows
        cpu = summary(rows, "cpu_explorateur")
        xs = [r["cpu_autres"][0] for r in rows if r.get("cpu_autres") and r["cpu_autres"][0] is not None]
        xcpu = f"{statistics.median(xs):.1f}" if xs else "—"
        print(f"{name:34} 1re {summary(rows, 'premiere'):28} fin {summary(rows, key):28} "
              f"CPU expl. {cpu:24} CPU X {xcpu:>6} lect. {summary(rows, 'lectures')}",
              flush=True)

    def safe(ex, fn):
        def run():
            if args.forcer_focus:
                x.x.XSetInputFocus(x.dpy, ex.win, 1, 0)
                x.x.XSync(x.dpy, 0)
            if not x.focus_inside(ex.win):
                raise SystemExit("sonde : le focus clavier n'est plus dans l'Explorateur, arrêt sans taper")
            fn()
        return run

    if args.home_reel:
        rows = []
        for i in range(args.essais):
            ex = Explorer(x, args, None, None)
            ok = ex.wait_open(0.3)
            rows.append(ex.result())
            ex.close()
            time.sleep(0.5)
            if not ok:
                sys.exit("sonde : fenêtre de l'Explorateur introuvable")
        line("ouverture Accueil (vrai HOME)", rows)
        if args.json:
            with open(args.json, "w") as f:
                json.dump(results, f, indent=1)
        return

    if args.court:
        petit, big, imgs, home = prepare(args.travail, 400)
        court(x, args, others, results, line, safe, petit, big, home)
        if args.json:
            with open(args.json, "w") as f:
                json.dump(results, f, indent=1)
        shutil.rmtree(args.travail, ignore_errors=True)
        return

    petit, big, imgs, home = prepare(args.travail, args.gros)
    # Ouvertures : la première est la plus froide que la sonde puisse obtenir
    # sans root (le cache de pages n'est pas vidé).
    rows = []
    for i in range(args.essais + 1):
        ex = Explorer(x, args, home, petit)
        if not ex.wait_open(0.3):
            sys.exit("sonde : fenêtre de l'Explorateur introuvable")
        rows.append(ex.result())
        ex.close()
        time.sleep(0.5)
    results["ouverture_premiere"] = rows[0]
    print(f"ouverture, 1re de la passe          visible {rows[0]['visible']} ms, 1re peinture "
          f"{rows[0]['premiere']} ms, fin {rows[0]['fin']} ms", flush=True)
    line("ouverture petit dossier (chaud)", rows[1:])
    line("  dont fenêtre visible", [dict(r, premiere=r["visible"], fin=r["visible"]) for r in rows[1:]])

    rows = []
    for i in range(args.essais):
        ex = Explorer(x, args, home, big)
        ex.wait_open(0.4)
        rows.append(ex.result())
        ex.close()
        time.sleep(0.5)
    line(f"ouverture dossier {args.gros} fichiers", rows)

    # Gestes dans une seule fenêtre.
    ex = Explorer(x, args, home, petit)
    ex.wait_open(0.4)
    time.sleep(1.0)
    gx, gy, gw, gh = x.geometry(ex.win)
    q = 0.12
    reps = max(args.essais, 5)

    line("flèche Bas (petit)", [gesture(x, ex, safe(ex, lambda: x.key("Down")), q, others=others)
                                for _ in range(reps * 2)])
    # Entrée sur « sous » (premier élément, dossier), Retour arrière revient.
    def into_sub():
        x.key("Home")
    gesture(x, ex, safe(ex, into_sub), q)
    rows_in, rows_back, rows_up = [], [], []
    for _ in range(reps):
        rows_in.append(gesture(x, ex, safe(ex, lambda: x.key("Return")), q, others=others))
        rows_back.append(gesture(x, ex, safe(ex, lambda: x.key("BackSpace")), q, others=others))
        gesture(x, ex, safe(ex, lambda: x.key("Home")), q)
    line("ouvrir un sous-dossier (Entrée)", rows_in)
    line("revenir, dossier déjà vu (Ret. arr.)", rows_back)
    # Dossier parent : bouton « Haut » de la barre d'outils (x 103, y 64).
    for _ in range(reps):
        gesture(x, ex, safe(ex, lambda: x.key("Return")), q)
        x.move(gx + 103, gy + 64)
        rows_up.append(gesture(x, ex, safe(ex, lambda: x.button(1)), q, others=others))
        gesture(x, ex, safe(ex, lambda: x.key("Home")), q)
    line("dossier parent (bouton Haut)", rows_up)
    # Clic droit au milieu de la liste, puis Échap.
    rows_menu, rows_esc = [], []
    for _ in range(reps):
        x.move(gx + gw // 2 + 60, gy + 220)
        time.sleep(0.15)
        x.events(0.05)
        rows_menu.append(gesture(x, ex, safe(ex, lambda: x.button(3)), q, others=others))
        rows_esc.append(gesture(x, ex, safe(ex, lambda: x.key("Escape")), q, others=others))
    line("clic droit, menu", rows_menu)
    line("Échap, menu fermé", rows_esc)
    # Renommer : F2, une lettre, Entrée (sur un .txt du petit dossier).
    rows_f2, rows_ren = [], []
    for i in range(reps):
        gesture(x, ex, safe(ex, lambda: x.key("End")), q)
        rows_f2.append(gesture(x, ex, safe(ex, lambda: x.key("F2")), q, others=others))
        gesture(x, ex, safe(ex, lambda: x.key("End")), q)
        gesture(x, ex, safe(ex, lambda: x.key("z")), q)
        rows_ren.append(gesture(x, ex, safe(ex, lambda: x.key("Return")), 0.3, others=others))
    line("F2, champ de renommage", rows_f2)
    line("Entrée, fichier renommé", rows_ren)
    # Copier-coller dans le même dossier (« - Copie »).
    rows_cv = []
    for i in range(reps):
        gesture(x, ex, safe(ex, lambda: x.key("End")), q)
        gesture(x, ex, safe(ex, lambda: x.key("c", ctrl=True)), q)
        rows_cv.append(gesture(x, ex, safe(ex, lambda: x.key("v", ctrl=True)), 0.4, others=others))
    line("Ctrl+V, copie affichée", rows_cv)
    results["fichiers_petit_apres"] = len(os.listdir(petit))
    ex.close()

    # Gros dossier : flèche, défilement, recherche.
    ex = Explorer(x, args, home, big)
    ex.wait_open(0.4)
    time.sleep(1.0)
    gx, gy, gw, gh = x.geometry(ex.win)
    line(f"flèche Bas ({args.gros} fichiers)", [gesture(x, ex, safe(ex, lambda: x.key("Down")), q, others=others)
                                               for _ in range(reps * 2)])
    x.move(gx + gw // 2 + 60, gy + 300)
    time.sleep(0.2)
    line("molette, un cran", [gesture(x, ex, lambda: x.button(5), q, others=others) for _ in range(reps * 2)])
    # Défilement terminé : silence de 0,7 s exigé (le tic lent est de 500 ms).
    time.sleep(1.0)
    line("molette, défilement terminé", [gesture(x, ex, lambda: x.button(5), 0.7, limit=6.0, others=others)
                                         for _ in range(reps)])
    # Coût au repos, fenêtre ouverte, rien ne bouge : 10 s.
    time.sleep(1.0)
    c0, o0, i0, t0 = schedstat(ex.proc.pid), [schedstat(p) if p else None for p in others], io(ex.proc.pid), \
        time.monotonic()
    time.sleep(10.0)
    c1, o1, i1, t1 = schedstat(ex.proc.pid), [schedstat(p) if p else None for p in others], io(ex.proc.pid), \
        time.monotonic()
    dur = t1 - t0
    rest = {"cpu_explorateur_pct": None if c0 is None else round((c1 - c0) / 1e7 / dur, 3),
            "cpu_x_pct": None if not o0[0] else round((o1[0] - o0[0]) / 1e7 / dur, 3),
            "reveils_par_s": round((i1.get("ctxsw", 0) - i0.get("ctxsw", 0)) / dur, 1),
            "lectures_par_s": round((i1.get("syscr", 0) - i0.get("syscr", 0)) / dur, 1)}
    results["repos"] = rest
    print(f"au repos (10 s)                    CPU expl. {rest['cpu_explorateur_pct']} % d'un cœur, "
          f"CPU X {rest['cpu_x_pct']} %, réveils {rest['reveils_par_s']}/s, lectures {rest['lectures_par_s']}/s",
          flush=True)
    line("Fin (dernier élément)", [gesture(x, ex, safe(ex, lambda: x.key("End")), q, others=others),
                                   gesture(x, ex, safe(ex, lambda: x.key("Home")), q, others=others)] * 1)
    gesture(x, ex, safe(ex, lambda: x.key("f", ctrl=True)), q)
    rows_s = []
    for ch in "fichier-04":
        rows_s.append(gesture(x, ex, safe(ex, lambda c=ch: x.key(c)), q,
                              others=others))
    line("recherche, une lettre tapée", rows_s)
    ex.close()

    # Vignettes : dossier d'images, vue « Grandes icônes » (bouton de la barre d'état).
    shutil.rmtree(os.path.join(home, ".cache"), ignore_errors=True)
    for label in ("vignettes, cache vide", "vignettes, cache plein"):
        ex = Explorer(x, args, home, imgs)
        ex.wait_open(0.4)
        time.sleep(0.5)
        gx, gy, gw, gh = x.geometry(ex.win)
        x.move(gx + gw - 21, gy + gh - 14)
        r = gesture(x, ex, lambda: x.button(1), 1.0, limit=30.0, others=others)
        line(label, [r])
        if args.capture and label.endswith("plein"):
            x.capture_png(ex.win, args.capture)
        # Revenir en Détails pour la passe suivante (bouton voisin).
        x.move(gx + gw - 51, gy + gh - 14)
        gesture(x, ex, lambda: x.button(1), q)
        ex.close()

    if args.json:
        with open(args.json, "w") as f:
            json.dump(results, f, indent=1)
    shutil.rmtree(args.travail, ignore_errors=True)


if __name__ == "__main__":
    main()
