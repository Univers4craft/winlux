#!/usr/bin/python3
"""Fond « Winlux Aurore » animé (#3079), sous Xvfb.

Vérifie le comportement de winlux-fond-anime, vu de dehors :
- désactivé par défaut, et rien ne tourne ;
- sans compositeur, il ne se lance pas (la couche serait noire) ;
- avec un compositeur et le fond Aurore, une seule couche « bureau » sans
  entrée souris, qui se réveille quelques fois par seconde ;
- plus aucun réveil d'image quand une fenêtre est en plein écran ou agrandie,
  quand l'économiseur d'écran est actif ou quand la batterie est faible ;
- il s'arrête de lui-même quand le fond change, quand le compositeur s'en va,
  et quand on le désactive.

Le compositeur et le gestionnaire de fenêtres sont simulés par libX11 (ctypes) :
seule leur trace compte pour l'animation (sélection _NET_WM_CM_S0,
_NET_CLIENT_LIST, _NET_WM_STATE). Le fond affiché vient d'un faux
`xfconf-query -l -v` (WINLUX_FOND_ANIME_XFCONF).
"""
import ctypes
import ctypes.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
BIN = os.environ.get("WINLUX_FOND_ANIME_BIN") or str(ROOT / "target" / "release" / "winlux-fond-anime")
failures = []


def check(ok, label):
    print(("ok    " if ok else "ÉCHEC ") + label, flush=True)
    if not ok:
        failures.append(label)


# ── libX11 minimal ──────────────────────────────────────────────────────────
x11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
x11.XDefaultRootWindow.restype = ctypes.c_ulong
x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x11.XInternAtom.restype = ctypes.c_ulong
x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
x11.XCreateSimpleWindow.restype = ctypes.c_ulong
x11.XCreateSimpleWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint,
                                    ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_ulong]
x11.XSetSelectionOwner.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
x11.XGetSelectionOwner.restype = ctypes.c_ulong
x11.XGetSelectionOwner.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x11.XChangeProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int,
                                ctypes.c_int, ctypes.c_void_p, ctypes.c_int]
x11.XMapWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
x11.XForceScreenSaver.argtypes = [ctypes.c_void_p, ctypes.c_int]
x11.XGetWindowProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long, ctypes.c_long,
                                   ctypes.c_int, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
                                   ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong),
                                   ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_void_p)]
x11.XFree.argtypes = [ctypes.c_void_p]

dpy = x11.XOpenDisplay(None)
if not dpy:
    print("pas d'écran X : lancer sous xvfb-run", file=sys.stderr)
    sys.exit(1)
root = x11.XDefaultRootWindow(dpy)


def atom(name):
    return x11.XInternAtom(dpy, name.encode(), 0)


XA_ATOM, XA_CARDINAL, XA_WINDOW, XA_PIXMAP = 4, 6, 33, 20


def set_atoms(win, prop, values, kind=XA_ATOM):
    data = (ctypes.c_ulong * max(1, len(values)))(*values)
    x11.XChangeProperty(dpy, win, atom(prop), kind, 32, 0, data, len(values))
    x11.XSync(dpy, 0)


def get32(win, prop):
    kind, fmt, n, after, data = ctypes.c_ulong(), ctypes.c_int(), ctypes.c_ulong(), ctypes.c_ulong(), ctypes.c_void_p()
    if x11.XGetWindowProperty(dpy, win, atom(prop), 0, 64, 0, 0, ctypes.byref(kind), ctypes.byref(fmt),
                              ctypes.byref(n), ctypes.byref(after), ctypes.byref(data)) != 0 or not data.value:
        return []
    values = list(ctypes.cast(data, ctypes.POINTER(ctypes.c_ulong))[:n.value]) if fmt.value == 32 else []
    x11.XFree(data)
    return values


ButtonPressMask = 1 << 2
x11.XSelectInput.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_long]
x11.XCheckWindowEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_long, ctypes.c_void_p]


def click_reaches(win, x, y):
    """Clic gauche simulé (XTEST) en (x, y) ; True si `win` le reçoit.
    None quand aucun moyen de simuler un clic n'est installé."""
    x11.XSelectInput(dpy, win, ButtonPressMask)
    x11.XSync(dpy, 0)
    path = ctypes.util.find_library("Xtst")
    if path:
        xtst = ctypes.CDLL(path)
        xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
        xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        xtst.XTestFakeMotionEvent(dpy, -1, x, y, 0)
        xtst.XTestFakeButtonEvent(dpy, 1, 1, 0)
        xtst.XTestFakeButtonEvent(dpy, 1, 0, 0)
    elif subprocess.run(["sh", "-c", "command -v xdotool"], capture_output=True).returncode == 0:
        subprocess.run(["xdotool", "mousemove", str(x), str(y), "click", "1"], timeout=10)
    else:
        return None
    x11.XSync(dpy, 0)
    event = ctypes.create_string_buffer(192)
    end = time.time() + 2
    while time.time() < end:
        if x11.XCheckWindowEvent(dpy, win, ButtonPressMask, event):
            return True
        time.sleep(0.05)
    return False


def new_window():
    win = x11.XCreateSimpleWindow(dpy, root, 0, 0, 200, 100, 0, 0, 0)
    x11.XMapWindow(dpy, win)
    x11.XSync(dpy, 0)
    return win


compositor_win = new_window()
cm_atom = atom("_NET_WM_CM_S0")


def compositor(on):
    x11.XSetSelectionOwner(dpy, cm_atom, compositor_win if on else 0, 0)
    x11.XSync(dpy, 0)


# ── Programme sous test ─────────────────────────────────────────────────────
work = Path(tempfile.mkdtemp(prefix="fond-anime-"))
xfconf = work / "xfconf.txt"
power = work / "power"
(power / "BAT0").mkdir(parents=True)
for name, value in (("type", "Battery"), ("status", "Discharging"), ("capacity", "80")):
    (power / "BAT0" / name).write_text(value + "\n")
env = dict(os.environ, XDG_CONFIG_HOME=str(work / "config"), WINLUX_FOND_ANIME_XFCONF=str(xfconf),
           WINLUX_FOND_ANIME_POWER=str(power),
           WINLUX_FOND_ANIME_ETOILES=str(ROOT / "assets" / "wallpapers" / "winlux-prisme-aurore.etoiles"))


def wallpaper(name):
    xfconf.write_text(f"/backdrop/screen0/monitorscreen/workspace0/last-image   /usr/share/winlux/wallpapers/{name}\n")


def cli(*args):
    out = subprocess.run([BIN, *args], env=env, capture_output=True, text=True, timeout=20)
    print(f"  $ winlux-fond-anime {' '.join(args)} → {out.returncode} {out.stdout.strip()!r} {out.stderr.strip()!r}", flush=True)
    return dict(line.split("=", 1) for line in out.stdout.splitlines() if "=" in line)


def mine(entry):
    """Lancé sur notre écran X (une autre session peut faire tourner le sien)."""
    try:
        return f"DISPLAY={os.environ.get('DISPLAY', '')}".encode() in (entry / "environ").read_bytes().split(b"\0")
    except OSError:
        return False


def animation_pid():
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            cmd = (entry / "cmdline").read_bytes().split(b"\0")
            state = (entry / "stat").read_text().rsplit(")", 1)[1].split()[0]
        except OSError:
            continue
        if cmd and cmd[0] == BIN.encode() and b"run" in cmd and state != "Z" and mine(entry):
            return int(entry.name)
    return None


def wakeups(pid, seconds):
    """Réveils volontaires par seconde : chaque image dessinée en est un."""
    def count():
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            if line.startswith("voluntary_ctxt_switches"):
                return int(line.split()[1])
        return 0
    first = count()
    time.sleep(seconds)
    rate = (count() - first) / seconds
    print(f"  réveils : {rate:.1f} par seconde sur {seconds} s", flush=True)
    return rate


def wait_gone(seconds=5):
    end = time.time() + seconds
    while time.time() < end:
        if animation_pid() is None:
            return True
        time.sleep(0.1)
    return False


def wait_running(seconds=5):
    end = time.time() + seconds
    while time.time() < end:
        pid = animation_pid()
        if pid:
            return pid
        time.sleep(0.1)
    return None


try:
    wallpaper("winlux-prisme-aurore.png")
    state = cli("status")
    check(state.get("enabled") == "off" and state.get("running") == "no", "désactivé par défaut, rien ne tourne (#3079)")

    cli("enable")
    state = cli("status")
    check(state.get("reason") == "sans-compositeur" and animation_pid() is None,
          "sans compositeur, l'animation ne se lance pas (#3079)")

    compositor(True)
    cli("ensure")
    pid = wait_running()
    check(pid is not None, "avec compositeur et fond Aurore, l'animation démarre (#3079)")
    time.sleep(1)
    cli("ensure")
    others = []
    for p in Path("/proc").iterdir():
        try:
            if p.name.isdigit() and p.name != str(pid) and mine(p) \
                    and (p / "cmdline").read_bytes().startswith(BIN.encode() + b"\0run") \
                    and (p / "stat").read_text().rsplit(")", 1)[1].split()[0] != "Z":
                others.append(p.name)
        except OSError:
            pass
    check(not others, "une seule animation à la fois (#3079)")

    # Couche : fenêtre de bureau, sur tous les bureaux virtuels, sans entrée.
    layer = x11.XGetSelectionOwner(dpy, atom("_WINLUX_FOND_ANIME_S0"))
    types = get32(layer, "_NET_WM_WINDOW_TYPE")
    check(atom("_NET_WM_WINDOW_TYPE_DESKTOP") in types, "la couche est une fenêtre de type bureau (#3079)")
    check([v & 0xFFFFFFFF for v in get32(layer, "_NET_WM_DESKTOP")] == [0xFFFFFFFF], "la couche est sur tous les bureaux virtuels (#3079)")
    # Un clic à l'endroit de la couche arrive à la fenêtre du dessous.
    passed = click_reaches(compositor_win, 50, 50)
    if passed is None:
        print("  (clic non vérifié : ni libXtst ni xdotool)", flush=True)
    else:
        check(passed, "la couche laisse passer les clics vers la fenêtre du dessous (#3079)")

    active = wakeups(pid, 4)
    check(active >= 2, "fond visible : l'animation se réveille pour dessiner (#3079)")

    # Fenêtre plein écran sur le bureau affiché : plus aucune image.
    win = new_window()
    set_atoms(win, "_NET_WM_STATE", [atom("_NET_WM_STATE_FULLSCREEN")])
    set_atoms(win, "_NET_WM_DESKTOP", [0], XA_CARDINAL)
    set_atoms(root, "_NET_CLIENT_LIST", [win], XA_WINDOW)
    time.sleep(0.5)
    check(wakeups(pid, 4) < 0.5, "fenêtre plein écran : aucune image dessinée (#3079)")

    set_atoms(win, "_NET_WM_STATE", [atom("_NET_WM_STATE_MAXIMIZED_VERT"), atom("_NET_WM_STATE_MAXIMIZED_HORZ")])
    time.sleep(0.5)
    check(wakeups(pid, 4) < 0.5, "fenêtre agrandie : aucune image dessinée (#3079)")

    set_atoms(win, "_NET_WM_STATE", [atom("_NET_WM_STATE_MAXIMIZED_VERT"), atom("_NET_WM_STATE_MAXIMIZED_HORZ"),
                                     atom("_NET_WM_STATE_HIDDEN")])
    time.sleep(0.5)
    check(wakeups(pid, 3) >= 2, "fenêtre agrandie puis réduite : l'animation reprend (#3079)")

    # Économiseur d'écran actif : plus aucune image, puis reprise.
    x11.XForceScreenSaver(dpy, 1)  # ScreenSaverActive
    x11.XSync(dpy, 0)
    time.sleep(0.5)
    check(wakeups(pid, 4) < 0.5, "économiseur d'écran actif : aucune image dessinée (#3079)")
    x11.XForceScreenSaver(dpy, 0)  # ScreenSaverReset
    x11.XSync(dpy, 0)
    time.sleep(0.5)
    check(wakeups(pid, 3) >= 2, "économiseur d'écran coupé : l'animation reprend (#3079)")

    # Changement de fond : xfdesktop repose sa pixmap, l'animation s'en va.
    wallpaper("winlux-prisme-aurore-clair.png")
    set_atoms(root, "_XROOTPMAP_ID", [1], XA_PIXMAP)
    check(wait_gone(), "autre fond (Aurore claire) : l'animation s'arrête d'elle-même (#3079)")
    check(cli("status").get("reason") == "autre-fond", "état : autre fond (#3079)")
    wallpaper("winlux-prisme-aurore-3840x2160.png")
    cli("ensure")
    pid = wait_running()
    check(pid is not None, "retour au fond Aurore (4K) : l'animation reprend (#3079)")

    # Compositeur arrêté : la couche serait noire, l'animation s'en va.
    compositor(False)
    check(wait_gone(), "compositeur arrêté : l'animation s'arrête d'elle-même (#3079)")
    compositor(True)

    # Batterie faible au démarrage : aucune image.
    (power / "BAT0" / "capacity").write_text("15\n")
    cli("ensure")
    pid = wait_running()
    time.sleep(1)
    check(pid is not None and wakeups(pid, 4) < 0.5, "batterie à 15 % sur batterie : aucune image dessinée (#3079)")
    (power / "BAT0" / "capacity").write_text("80\n")

    cli("disable")
    check(wait_gone(), "désactivé dans Paramètres : l'animation s'arrête (#3079)")
    state = cli("status")
    check(state.get("enabled") == "off" and state.get("running") == "no", "état après désactivation (#3079)")
    check((work / "config" / "winlux" / "fond-anime.conf").read_text().strip() == "ANIMATION=off",
          "le choix est gardé dans le réglage du compte (#3079)")
finally:
    pid = animation_pid()
    if pid:
        os.kill(pid, 15)

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print("fond animé : OK")
