#!/usr/bin/env python3
"""Barre des tâches recalculée sur changement, pas toutes les 2 s (#3129).

Avant : winlux-tray serve relançait xwininfo -root -tree, un xprop par
greffon, xfconf-query, xdpyinfo, ip, nmcli et pactl toutes les 2 à 3 s,
barre immobile (8,2 % d'un cœur sur le vrai PC).

1. Suivi X : une barre immobile ne produit aucun changement ; un greffon
   redimensionné, ajouté ou retiré en produit un ; une autre fenêtre déplacée
   n'en produit pas ; une barre relancée est retrouvée ; l'écran redimensionné
   en produit un.
2. Service : un greffon suivi n'est recalculé qu'après un changement, ou au
   plus tard après FALLBACK secondes ; jamais repris pendant le lancement de
   la barre ; un greffon sans suivi est recalculé à chaque tour, comme avant.
3. Programme résident (pactl subscribe, nmcli monitor) : une ligne utile fait
   avancer son compteur, une ligne d'un autre objet non.
4. Route par défaut lue dans /proc/net/route : même interface que ip.
"""
import ctypes
import ctypes.util
import os
import runpy
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
failures = []


def ok(condition, message):
    print(("OK    " if condition else "ÉCHEC ") + message)
    if not condition:
        failures.append(message)


base = Path(tempfile.mkdtemp())
os.environ.update({"XDG_CACHE_HOME": str(base / "cache"), "WINLUX_TRAY_DIR": str(base / "tray"),
                   "XDG_CONFIG_HOME": str(base / "config"), "XDG_STATE_HOME": str(base / "state"),
                   "WINLUX_LANG": "fr", "WINLUX_PANEL_THEME": "dark", "WINLUX_POWER_SUPPLY": str(base / "power")})
(base / "power").mkdir()
display = os.environ.pop("DISPLAY", None)
tray = runpy.run_path(str(SCRIPTS / "winlux-tray.py"), run_name="winlux_tray")

# ── 2. Service : recalcul seulement sur changement ──────────────────────────
serve = tray["serve"]
g = serve.__globals__
calls = []
g["render"] = lambda kind, parent=None: (calls.append(kind), f"<txt>{kind}</txt>")[1]
g["_panel_pid"] = lambda: os.getpid()
g["SETTLE"] = 0
age = {"value": 1000.0}
g["_seconds_since_start"] = lambda pid: age["value"]


class FakeX:
    ok, version = True, 0

    def poll(self):
        pass


class FakeLines:
    version = 0
    ok = True

    def poll(self):
        pass

    def close(self):
        pass


fake_x, fake_audio, fake_net = FakeX(), FakeLines(), FakeLines()
real_changes = g["Changes"]


def changes_factory(enabled=True):
    made = real_changes(enabled=False)
    made.enabled, made.x, made.audio, made.network = True, fake_x, fake_audio, fake_net
    return made


g["Changes"] = changes_factory
clock = {"t": 0.0}


def run(rounds):
    calls.clear()

    def tick():
        clock["t"] += 1.0
        return clock["t"]
    serve(clock=tick, sleep=lambda _: None, rounds=rounds)
    return {kind: calls.count(kind) for kind in ("balance", "status", "search", "notifications")}


counts = run(40)
print("40 tours, barre immobile :", counts)
ok(counts["balance"] == 1 and counts["status"] == 1 and counts["search"] == 1,
   "barre immobile : contrepoids, état et recherche calculés une seule fois en 40 s (#3129)")
ok(counts["notifications"] == 20, "notifications (un fichier lu) : toujours toutes les 2 s")
counts = run(130)
print("130 tours, barre immobile :", counts)
ok(counts["balance"] == 3 and counts["status"] == 3,
   f"recalcul de sûreté toutes les {g['FALLBACK']} s même sans changement vu")


def changed_after(action, kind):
    serve_calls = []

    def tick():
        clock["t"] += 1.0
        if len(serve_calls) == 5:
            action()
        serve_calls.append(1)
        return clock["t"]
    calls.clear()
    serve(clock=tick, sleep=lambda _: None, rounds=12)
    return calls.count(kind)


ok(changed_after(lambda: None, "balance") == 1, "référence : un seul calcul en 12 tours sans changement")
ok(changed_after(lambda: setattr(fake_x, "version", fake_x.version + 1), "balance") == 2,
   "greffon redimensionné (événement X) : contrepoids recalculé au tour suivant")
ok(changed_after(lambda: setattr(fake_audio, "version", fake_audio.version + 1), "status") == 2,
   "volume changé (pactl subscribe) : bouton réseau/son/batterie recalculé")
ok(changed_after(lambda: setattr(fake_net, "version", fake_net.version + 1), "status") == 2,
   "réseau changé (nmcli monitor) : bouton recalculé")
(base / "power/BAT0").mkdir()
(base / "power/BAT0/capacity").write_text("80\n")
ok(changed_after(lambda: (base / "power/BAT0/capacity").write_text("79\n"), "status") == 2,
   "batterie qui baisse : bouton recalculé")
(base / "config/winlux").mkdir(parents=True)
ok(changed_after(lambda: (base / "config/winlux/taskbar.conf").write_text("STYLE=floating\n"), "balance") == 2,
   "style de barre changé (Personnalisation) : contrepoids recalculé")
fake_audio.ok = False
ok(changed_after(lambda: None, "status") == 4, "pactl absent ou arrêté : bouton recalculé toutes les 3 s, comme avant")
fake_audio.ok = True
age["value"] = 5.0
ok(changed_after(lambda: None, "balance") == 6, "barre en lancement : contrepoids recalculé à chaque tour (#602)")
age["value"] = 1000.0
g["Changes"] = real_changes

# ── 3. Programme résident ──────────────────────────────────────────────────
Lines = tray["_Lines"]
script = base / "evenements.sh"
script.write_text("#!/bin/sh\necho \"Event 'change' on sink-input #12\"\nsleep 0.3\n"
                  "echo \"Event 'change' on sink #3\"\nsleep 30\n")
script.chmod(0o755)
lines = Lines([str(script)], lambda line: __import__("re").search(r" on (sink|server|card) ", line + " ") is not None)
time.sleep(0.15)
lines.poll()
ok(lines.ok and lines.version == 0, "flux d'un programme (sink-input) : pas de recalcul")
time.sleep(0.4)
lines.poll()
ok(lines.version == 1, "volume de la sortie (sink) : un recalcul")
lines.close()
ok(not lines.ok, "programme résident arrêté avec le service")
ok(not Lines(["/nonexistent/pactl", "subscribe"]).ok, "pactl absent : pas de suivi, pas d'erreur")

# ── 4. Route par défaut sans lancer ip ─────────────────────────────────────
if shutil.which("ip"):
    routes = subprocess.run(["ip", "-4", "route", "show", "default"], capture_output=True, text=True).stdout.split()
    expected = routes[routes.index("dev") + 1] if "dev" in routes else ""
    ok(tray["_default_interface"]() == expected, f"route par défaut lue dans /proc : {expected!r} comme ip")

# ── 1. Suivi X réel (Xvfb) ─────────────────────────────────────────────────
xvfb = shutil.which("Xvfb")
lib = ctypes.util.find_library("X11")
if not xvfb or not lib:
    print("SAUTÉ suivi X : Xvfb ou libX11 absent")
else:
    number = 87
    server = subprocess.Popen([xvfb, f":{number}", "-screen", "0", "1280x800x24", "-nolisten", "tcp"],
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        os.environ["DISPLAY"] = f":{number}"
        for _ in range(50):
            if Path(f"/tmp/.X11-unix/X{number}").exists():
                break
            time.sleep(0.1)
        x = ctypes.CDLL(lib)
        x.XOpenDisplay.restype = ctypes.c_void_p
        x.XOpenDisplay.argtypes = [ctypes.c_char_p]
        dpy = x.XOpenDisplay(None)
        x.XDefaultRootWindow.restype = ctypes.c_ulong
        x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        root = x.XDefaultRootWindow(dpy)
        x.XCreateSimpleWindow.restype = ctypes.c_ulong
        x.XCreateSimpleWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
                                          ctypes.c_uint, ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_ulong]
        for name in ("XMapWindow", "XDestroyWindow"):
            getattr(x, name).argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        x.XMoveResizeWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint]
        x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]

        class Hint(ctypes.Structure):
            _fields_ = [("name", ctypes.c_char_p), ("klass", ctypes.c_char_p)]
        x.XSetClassHint.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(Hint)]

        def window(parent, w, h, cls=None):
            made = x.XCreateSimpleWindow(dpy, parent, 0, 0, w, h, 0, 0, 0)
            if cls:
                x.XSetClassHint(dpy, made, ctypes.byref(Hint(cls, cls.capitalize())))
            x.XMapWindow(dpy, made)
            x.XSync(dpy, 0)
            return made

        def settle(watch):
            time.sleep(0.15)
            watch.poll()
            return watch.version

        panel = window(root, 1280, 48, b"xfce4-panel")
        socket = window(panel, 200, 48)
        plug = window(socket, 30, 48)
        other = window(root, 400, 300, b"mousepad")
        watch = tray["_XWatch"]()
        ok(watch.ok and {panel, socket, plug} <= watch.watched, "suivi X : la barre et ses greffons sont suivis")
        start = settle(watch)
        for _ in range(5):
            idle = settle(watch)
        ok(idle == start, "barre immobile : aucun changement en 0,75 s")
        x.XMoveResizeWindow(dpy, other, 50, 50, 500, 320)
        x.XSync(dpy, 0)
        ok(settle(watch) == idle, "autre fenêtre déplacée : pas de recalcul de la barre")
        x.XMoveResizeWindow(dpy, plug, 0, 0, 44, 48)
        x.XSync(dpy, 0)
        after = settle(watch)
        ok(after > idle, "greffon élargi (texte de l'horloge) : changement vu")
        icon = window(socket, 24, 24)
        added = settle(watch)
        ok(added > after and icon in watch.watched, "icône ajoutée dans la barre : changement vu, et suivie")
        x.XMoveResizeWindow(dpy, icon, 0, 0, 30, 30)
        x.XSync(dpy, 0)
        ok(settle(watch) > added, "icône ajoutée puis redimensionnée : changement vu")
        x.XDestroyWindow(dpy, panel)
        x.XSync(dpy, 0)
        gone = settle(watch)
        new_panel = window(root, 1280, 48, b"xfce4-panel")
        new_plug = window(new_panel, 30, 48)
        relaunched = settle(watch)
        ok(relaunched > gone and new_plug in watch.watched, "barre relancée : retrouvée et suivie")
        x.XMoveResizeWindow(dpy, new_plug, 0, 0, 60, 48)
        x.XSync(dpy, 0)
        ok(settle(watch) > relaunched, "greffon de la barre relancée : changement vu")
    finally:
        server.terminate()
        server.wait()
        os.environ.pop("DISPLAY", None)

shutil.rmtree(base, ignore_errors=True)
if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("barre recalculée sur changement : OK")
