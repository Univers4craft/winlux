#!/usr/bin/python3
"""Fenêtres agrandies à coins carrés (#582) et ombre unique des fenêtres GTK
à décorations côté client (#583).

Hors affichage : les règles _WINLUX_MAXIMIZED et _WINLUX_CSD sont dans le
picom.conf livré (plus de « _GTK_FRAME_EXTENTS@ », sans effet sur picom 12.5), le
veilleur est installé et lancé par les deux sessions, et la migration remplace
un ancien picom.conf Winlux sans toucher à un picom.conf personnel.

Avec un affichage (xvfb-run) et openbox : une fenêtre agrandie reçoit
_WINLUX_MAXIMIZED = 1 sur son cadre (veilleur en --once puis en continu), et
la perd quand elle est restaurée ; une fenêtre qui pose _GTK_FRAME_EXTENTS
reçoit _WINLUX_CSD = 1 sur son cadre et la perd quand la propriété disparaît.
"""
import ast
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
SHIPPED = ROOT / "integration/native/config/picom/picom.conf"
WATCH = HERE / "winlux-maximized-watch.py"
problems = []


def check(condition, message):
    if not condition:
        problems.append(message)


def offline_checks():
    text = SHIPPED.read_text(encoding="utf-8")
    check(text.startswith("# Compositeur de la session Winlux"), "picom.conf livré : en-tête Winlux absent (migration impossible)")
    check(re.search(r'\{ match = "_WINLUX_MAXIMIZED = 1"; corner-radius = 0;', text) is not None,
          "picom.conf livré : règle _WINLUX_MAXIMIZED absente")
    check(re.search(r'\{ match = "_WINLUX_CSD = 1"; shadow = false;', text) is not None,
          "picom.conf livré : règle _WINLUX_CSD absente (double ombre GTK, #583)")
    check("_GTK_FRAME_EXTENTS@" not in text,
          "picom.conf livré : règle « _GTK_FRAME_EXTENTS@ » sans effet sur picom 12.5 (#583)")
    stage = (HERE / "stage-desktop-components.sh").read_text(encoding="utf-8")
    check(" winlux-maximized-watch " in stage, "winlux-maximized-watch non installé dans l'image")
    for session in ("winlux-user-session.sh", "winlux-native-session.sh"):
        check("/usr/bin/winlux-maximized-watch" in (HERE / session).read_text(encoding="utf-8"),
              f"{session} : veilleur non lancé à côté de picom")

    # Migration : seule la fonction migrate_picom de winlux-native-config.
    namespace = {"shutil": shutil}
    source = (HERE / "winlux-native-config.py").read_text(encoding="utf-8")
    for node in ast.parse(source).body:
        if isinstance(node, ast.FunctionDef) and node.name == "migrate_picom":
            exec(compile(ast.Module([node], []), "winlux-native-config", "exec"), namespace)
    migrate = namespace.get("migrate_picom")
    check(migrate is not None, "winlux-native-config : migrate_picom absente")
    if migrate is None:
        return
    with tempfile.TemporaryDirectory() as folder:
        folder = Path(folder)
        old = folder / "ancien.conf"
        old.write_text("\n".join(line for line in SHIPPED.read_text(encoding="utf-8").splitlines()
                                 if "_WINLUX_MAXIMIZED" not in line) + "\n", encoding="utf-8")
        migrate(SHIPPED, old)
        check(old.read_bytes() == SHIPPED.read_bytes(), "migration : ancien picom.conf Winlux non remplacé")
        # picom.conf de la 0.2.20 : règle _WINLUX_MAXIMIZED, mais « _GTK_FRAME_EXTENTS@ ».
        before_583 = folder / "avant-583.conf"
        before_583.write_text("\n".join(line for line in SHIPPED.read_text(encoding="utf-8").splitlines()
                                        if "_WINLUX_CSD" not in line) + "\n"
                              + '  { match = "_GTK_FRAME_EXTENTS@"; shadow = false; },\n', encoding="utf-8")
        migrate(SHIPPED, before_583)
        check(before_583.read_bytes() == SHIPPED.read_bytes(), "migration : picom.conf sans _WINLUX_CSD non remplacé")
        personal = folder / "perso.conf"
        mine = "# Mon picom\ncorner-radius = 12;\n"
        personal.write_text(mine, encoding="utf-8")
        migrate(SHIPPED, personal)
        check(personal.read_text(encoding="utf-8") == mine, "migration : picom.conf personnel modifié")
        edited = folder / "modifie.conf"
        kept = SHIPPED.read_text(encoding="utf-8") + "shadow-radius = 4;\n"
        edited.write_text(kept, encoding="utf-8")
        migrate(SHIPPED, edited)
        check(edited.read_text(encoding="utf-8") == kept, "migration : picom.conf déjà à jour réécrit")
        migrate(SHIPPED, folder / "absent.conf")
        check(not (folder / "absent.conf").exists(), "migration : fichier absent créé")


def run(*command):
    return subprocess.run(command, capture_output=True, text=True, timeout=10).stdout


def frame_of(window):
    root = re.search(r"Window id: (0x[0-9a-f]+)", run("xwininfo", "-root")).group(1)
    current = window
    for _ in range(16):
        found = re.search(r"Parent window id: (0x[0-9a-f]+)", run("xwininfo", "-children", "-id", current))
        if not found:
            return None
        if int(found.group(1), 16) == int(root, 16):
            return current
        current = found.group(1)
    return None


def marker(frame, name="_WINLUX_MAXIMIZED"):
    return f"{name}(CARDINAL) = 1" in run("xprop", "-id", frame, name)


def csd(window, on):
    if on:
        run("xprop", "-id", window, "-f", "_GTK_FRAME_EXTENTS", "32c", "-set", "_GTK_FRAME_EXTENTS", "12,12,12,12")
    else:
        run("xprop", "-id", window, "-remove", "_GTK_FRAME_EXTENTS")


def wait_for(predicate, seconds=5):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if predicate():
            return True
        time.sleep(0.1)
    return predicate()


def find_window(title, seconds=15):
    def search():
        for window in re.findall(r"0x[0-9a-f]+", run("xprop", "-root", "_NET_CLIENT_LIST").split("=", 1)[-1]):
            if f'"{title}"' in run("xprop", "-id", window, "WM_NAME"):
                return window
        return None
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        window = search()
        if window:
            return window
        time.sleep(0.2)
    return None


def test_window(title):
    """Fenêtre X toute simple (Xlib par ctypes, sans GTK) qui vit jusqu'à être tuée."""
    import ctypes
    x = ctypes.CDLL("libX11.so.6")
    x.XOpenDisplay.restype = ctypes.c_void_p
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XDefaultRootWindow.restype = ctypes.c_ulong
    x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x.XCreateSimpleWindow.restype = ctypes.c_ulong
    x.XCreateSimpleWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_uint,
                                      ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_ulong]
    x.XStoreName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_char_p]
    x.XMapWindow.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    x.XNextEvent.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    display = x.XOpenDisplay(None)
    window = x.XCreateSimpleWindow(display, x.XDefaultRootWindow(display), 50, 50, 400, 300, 0, 0, 0xffffff)
    x.XStoreName(display, window, title.encode())
    x.XMapWindow(display, window)
    event = (ctypes.c_long * 24)()
    while True:
        x.XNextEvent(display, event)


def maximize(title, on):
    run("wmctrl", "-r", title, "-b", f"{'add' if on else 'remove'},maximized_vert,maximized_horz")


def display_checks():
    title = "winlux-coins-carres"
    with tempfile.TemporaryDirectory() as folder:
        openbox = subprocess.Popen(["openbox", "--config-file", os.devnull], stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL, env=dict(os.environ, HOME=folder))
        processes = [openbox]
        try:
            check(wait_for(lambda: subprocess.run(["wmctrl", "-m"], capture_output=True).returncode == 0, 10),
                  "openbox n'a pas démarré")
            app = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()), "window", title])
            processes.append(app)
            window = find_window(title)
            check(window is not None, "fenêtre d'essai introuvable")
            if not window:
                return
            frame = frame_of(window)
            check(frame is not None and frame != window, "cadre openbox introuvable")
            if not frame or frame == window:
                return

            # --once : synchronise puis quitte.
            subprocess.run([sys.executable, "-B", str(WATCH), "--once"], timeout=10)
            check(not marker(frame), "--once : fenêtre normale marquée agrandie")
            maximize(title, True)
            time.sleep(0.5)
            subprocess.run([sys.executable, "-B", str(WATCH), "--once"], timeout=10)
            check(marker(frame), "--once : fenêtre agrandie sans _WINLUX_MAXIMIZED sur son cadre")
            maximize(title, False)
            time.sleep(0.5)
            subprocess.run([sys.executable, "-B", str(WATCH), "--once"], timeout=10)
            check(not marker(frame), "--once : marque restée après restauration")
            check(not marker(frame, "_WINLUX_CSD"), "--once : fenêtre sans _GTK_FRAME_EXTENTS marquée CSD")
            csd(window, True)
            subprocess.run([sys.executable, "-B", str(WATCH), "--once"], timeout=10)
            check(marker(frame, "_WINLUX_CSD"), "--once : _GTK_FRAME_EXTENTS non recopiée en _WINLUX_CSD sur le cadre")
            csd(window, False)
            subprocess.run([sys.executable, "-B", str(WATCH), "--once"], timeout=10)
            check(not marker(frame, "_WINLUX_CSD"), "--once : _WINLUX_CSD restée sans _GTK_FRAME_EXTENTS")

            # En continu : suit l'agrandissement et la restauration.
            watch = subprocess.Popen([sys.executable, "-B", str(WATCH)])
            processes.append(watch)
            time.sleep(0.5)
            maximize(title, True)
            check(wait_for(lambda: marker(frame)), "veilleur : marque non posée à l'agrandissement")
            maximize(title, False)
            check(wait_for(lambda: not marker(frame)), "veilleur : marque non retirée à la restauration")
            csd(window, True)
            check(wait_for(lambda: marker(frame, "_WINLUX_CSD")), "veilleur : _WINLUX_CSD non posée")
            csd(window, False)
            check(wait_for(lambda: not marker(frame, "_WINLUX_CSD")), "veilleur : _WINLUX_CSD non retirée")
            check(watch.poll() is None, "veilleur arrêté en cours de route")
        finally:
            for process in reversed(processes):
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()


def main():
    offline_checks()
    missing = [tool for tool in ("openbox", "wmctrl", "xprop", "xwininfo") if not shutil.which(tool)]
    if missing or not os.environ.get("DISPLAY"):
        print("Fenêtres réelles non vérifiées : " + (", ".join(missing) or "DISPLAY") + " absent(s)")
    else:
        display_checks()
    for problem in problems:
        print("ÉCHEC : " + problem, file=sys.stderr)
    if not problems:
        print("OK: coins carrés des fenêtres agrandies et ombre unique des fenêtres GTK CSD (règles picom, veilleur, migration)")
    return 1 if problems else 0


if __name__ == "__main__":
    if sys.argv[1:2] == ["window"]:
        test_window(sys.argv[2])
    else:
        sys.exit(main())
