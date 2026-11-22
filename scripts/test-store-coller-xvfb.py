#!/usr/bin/python3
"""Store : Ctrl+V colle le presse-papiers dans la recherche (#2894).

À lancer dans un affichage X dédié (xvfb-run), après la compilation du
runtime et de gui_demo.exe. Un texte est copié côté Linux (sélection X11
CLIPBOARD, comme une copie faite dans le navigateur) ; le pont du
presse-papiers (`winlux --clipboard-bridge`) le rend aux programmes Windows.
Le Store (shell PE) s'ouvre ; le test clique vraiment (XTest) dans la
recherche puis tape Ctrl+V, et lit ce que le Store a dessiné grâce à
WINLUX_CONTRAST_AUDIT (chaque texte dessiné est noté, une fois).

Attendu : le texte copié est dessiné dans la recherche après Ctrl+V, et
pas avant.

    test-store-coller-xvfb.py [--runtime target/release/winlux]
                              [--testdir target/x86_64-pc-windows-msvc/release]
"""
import argparse
import ctypes
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

project = Path(__file__).resolve().parent.parent
COPIED = "Notepad++ 8.9"
# Recherche du Store, fenêtre à sa place par défaut sur un écran 1920×1080
# sans gestionnaire de fenêtres.
SEARCH = (634, 99)
OWNER = r"""
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gdk, Gtk
clip = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
clip.set_text(sys.argv[1], -1)
print("copié", flush=True)
Gtk.main()
"""

failures = []


def check(condition, label):
    print(("ok   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


def drawn(report):
    try:
        lines = report.read_text(encoding="cp1252", errors="replace").splitlines()
    except OSError:
        return []
    return [(line.split("\t") + ["", "", ""])[3] for line in lines]


def stop(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=5)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", default=str(project / "target/release/winlux"))
    parser.add_argument("--testdir", default=str(project / "target/x86_64-pc-windows-msvc/release"))
    args = parser.parse_args()
    demo = Path(args.testdir) / "gui_demo.exe"
    if not Path(args.runtime).is_file() or not demo.is_file():
        print(f"runtime ou gui_demo.exe absent ({args.runtime}, {demo})")
        return 1
    with tempfile.TemporaryDirectory() as temp:
        temp = Path(temp)
        home, run = temp / "home", temp / "run"
        for d in (home / ".local/share/winlux", run):
            d.mkdir(parents=True)
        (home / ".local/share/winlux/personnalisation.conf").write_text("oobe=done\n")
        report = temp / "texte.txt"
        lines = ["sources=apt", "refreshing=0", f"updated={int(time.time())}", "updates=0", "system_updates=0"]
        lines += [f"app=apt:{n}|utilitaires|{n}|Logiciel {n}|Ed|a|||100||Ubuntu|1|" for n in ("lynx", "vlc")]
        (run / "store.txt").write_text("\n".join(lines) + "\n")
        environment = dict(os.environ, HOME=str(home), WINLUX_DRIVE_C=str(home),
                           XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".local/share"),
                           XDG_RUNTIME_DIR=str(run), WINLUX_STORE_RUN=str(run), WINLUX_NATIVE_SHELL="1",
                           WINLUX_SHELL_APP="store", WINLUX_LANG="fr", WINLUX_AUDIO="null", WINLUX_NO_SPLASH="1",
                           WINLUX_NO_LOGIN="1", WINLUX_NO_OOBE="1", WINLUX_CONTRAST_AUDIT=str(report),
                           WINLUX_CONTRAST_MINIMUM="2100", NO_AT_BRIDGE="1")
        quiet = dict(stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        bridge = subprocess.Popen([args.runtime, "--clipboard-bridge"], env=environment, **quiet)
        time.sleep(1)
        owner = subprocess.Popen([sys.executable, "-c", OWNER, COPIED], env=environment, stdout=subprocess.PIPE,
                                 stderr=subprocess.DEVNULL, text=True, start_new_session=True)
        process = None
        x11, xtst = ctypes.CDLL("libX11.so.6"), ctypes.CDLL("libXtst.so.6")
        x11.XOpenDisplay.restype = ctypes.c_void_p
        x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        x11.XStringToKeysym.restype = ctypes.c_ulong
        x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
        x11.XKeysymToKeycode.restype = ctypes.c_ubyte
        x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                              ctypes.c_ulong]
        xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        display = x11.XOpenDisplay(None)

        def click(point):
            xtst.XTestFakeMotionEvent(display, -1, point[0], point[1], 0)
            x11.XSync(display, 0)
            time.sleep(0.3)
            xtst.XTestFakeButtonEvent(display, 1, 1, 0)
            xtst.XTestFakeButtonEvent(display, 1, 0, 0)
            x11.XSync(display, 0)

        def chord(*names):
            codes = [x11.XKeysymToKeycode(display, x11.XStringToKeysym(n.encode())) for n in names]
            for c in codes:
                xtst.XTestFakeKeyEvent(display, c, 1, 0)
                x11.XSync(display, 0)
                time.sleep(0.05)
            for c in reversed(codes):
                xtst.XTestFakeKeyEvent(display, c, 0, 0)
            x11.XSync(display, 0)

        def wait(condition, seconds):
            end = time.time() + seconds
            while time.time() < end:
                if condition():
                    return True
                time.sleep(0.2)
            return condition()

        try:
            check(owner.stdout.readline().strip() == "copié", f"« {COPIED} » copié côté Linux")
            time.sleep(2)
            process = subprocess.Popen([args.runtime, str(demo)], env=environment, **quiet)
            check(wait(lambda: "Logiciel lynx" in drawn(report) or "lynx" in drawn(report), 20),
                  "le Store affiche le catalogue")
            time.sleep(1)
            click(SEARCH)
            time.sleep(1)
            check(COPIED not in drawn(report), "avant Ctrl+V, le texte copié n'est pas dans la recherche")
            chord("Control_L", "v")
            check(wait(lambda: COPIED in drawn(report), 5), f"Ctrl+V colle « {COPIED} » dans la recherche")
        finally:
            for p in (process, owner, bridge):
                if p is not None:
                    stop(p)
        if failures:
            print("textes dessinés :", sorted(set(drawn(report)))[:80])
    if failures:
        print(f"\n{len(failures)} contrôle(s) en échec")
        return 1
    print("OK : Ctrl+V colle le presse-papiers dans la recherche du Store")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
