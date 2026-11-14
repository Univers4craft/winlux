#!/usr/bin/python3
"""Store : un second clic sur « Installer » pendant une opération le dit à l'écran (#2186).

À lancer dans un affichage X dédié (xvfb-run), après la compilation du
runtime et de gui_demo.exe. Le Store (shell PE) s'ouvre sur un catalogue
de six logiciels ; un faux winlux-store, placé en tête du PATH, note chaque
appel et laisse une installation « en cours » pendant une minute. Le test
clique vraiment (XTest) sur « Installer » d'une première carte, puis d'une
seconde, et lit ce que le Store a dessiné grâce à WINLUX_CONTRAST_AUDIT
(seuil 2100 : chaque texte dessiné est noté, une fois).

Attendu : le premier clic lance l'installation ; le second n'en lance pas
d'autre et le bandeau du bas affiche « Une installation est déjà en cours :
attendez qu'elle se termine. ». Avant le second clic, ce texte n'est pas
dessiné.

    test-store-bandeau-xvfb.py [--runtime target/release/winlux]
                               [--testdir target/x86_64-pc-windows-msvc/release]
"""
import argparse
import ctypes
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

project = Path(__file__).resolve().parent.parent
BUSY = "Une installation est déjà en cours : attendez qu'elle se termine."
APPS = ("lynx", "vlc", "gimp", "abiword", "inkscape", "krita")
# Boutons « Installer » des deux premières cartes « Applications essentielles »
# (fenêtre du Store à sa place par défaut sur un écran 1920×1080, sans gestionnaire de fenêtres).
# Le bandeau de progression apparu après le premier clic remonte les cartes de 23 points.
FIRST, SECOND = (359, 631), (571, 608)

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


def calls(log):
    try:
        return log.read_text().splitlines()
    except OSError:
        return []


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
        home, run, bin_dir = temp / "home", temp / "run", temp / "bin"
        for d in (home / ".local/share/winlux", run, bin_dir):
            d.mkdir(parents=True)
        (home / ".local/share/winlux/personnalisation.conf").write_text("oobe=done\n")
        log, report = temp / "appels.log", temp / "texte.txt"
        progress = run / "store-progress.txt"
        fake = bin_dir / "winlux-store"
        fake.write_text(f"""#!/bin/sh
echo "$*" >> '{log}'
case "$1" in
install|remove)
  printf 'op=%s\\ntarget=%s\\nstate=running\\npercent=40\\nmessage=Installation en cours\\n' "$1" "$2" > '{progress}'
  sleep 60;;
esac
exit 0
""")
        fake.chmod(0o755)
        lines = ["sources=apt", "refreshing=0", f"updated={int(time.time())}", "updates=0", "system_updates=0"]
        lines += [f"app=apt:{n}|utilitaires|{n}|Logiciel {n}|Ed|a|||100||Ubuntu|1|" for n in APPS]
        (run / "store.txt").write_text("\n".join(lines) + "\n")
        environment = dict(os.environ, HOME=str(home), WINLUX_DRIVE_C=str(home),
                           XDG_CONFIG_HOME=str(home / ".config"), XDG_DATA_HOME=str(home / ".local/share"),
                           PATH=f"{bin_dir}:{os.environ.get('PATH', '/usr/bin:/bin')}",
                           WINLUX_STORE_RUN=str(run), WINLUX_NATIVE_SHELL="1", WINLUX_SHELL_APP="store",
                           WINLUX_LANG="fr", WINLUX_AUDIO="null", WINLUX_NO_SPLASH="1", WINLUX_NO_LOGIN="1",
                           WINLUX_NO_OOBE="1", WINLUX_CONTRAST_AUDIT=str(report), WINLUX_CONTRAST_MINIMUM="2100")
        process = subprocess.Popen([args.runtime, str(demo)], env=environment, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL, start_new_session=True)
        x11, xtst = ctypes.CDLL("libX11.so.6"), ctypes.CDLL("libXtst.so.6")
        x11.XOpenDisplay.restype = ctypes.c_void_p
        x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                              ctypes.c_ulong]
        xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        display = x11.XOpenDisplay(None)

        def click(point):
            xtst.XTestFakeMotionEvent(display, -1, point[0], point[1], 0)
            x11.XSync(display, 0)
            time.sleep(0.3)
            xtst.XTestFakeButtonEvent(display, 1, 1, 0)
            xtst.XTestFakeButtonEvent(display, 1, 0, 0)
            x11.XSync(display, 0)

        def wait(condition, seconds):
            end = time.time() + seconds
            while time.time() < end:
                if condition():
                    return True
                time.sleep(0.2)
            return condition()

        try:
            check(wait(lambda: "Logiciel lynx" in drawn(report) or "lynx" in drawn(report), 20),
                  "le Store affiche le catalogue")
            time.sleep(1)
            click(FIRST)
            check(wait(lambda: any(c.startswith("install") and "lynx" in c for c in calls(log)), 10),
                  "le premier clic lance l'installation de lynx")
            time.sleep(2)
            check(BUSY not in drawn(report), "avant le second clic, le bandeau n'affiche pas l'attente")
            click(SECOND)
            check(wait(lambda: BUSY in drawn(report), 5),
                  f"le second clic affiche dans le bandeau « {BUSY} »")
            time.sleep(1)
            check(not any("vlc" in c for c in calls(log)), "le second clic ne lance aucune autre installation")
        finally:
            try:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=5)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            subprocess.run(["pkill", "-f", str(fake)], check=False)
        if failures:
            print("appels :", calls(log))
            print("textes dessinés :", sorted(set(drawn(report)))[:80])
    if failures:
        print(f"\n{len(failures)} contrôle(s) en échec")
        return 1
    print("OK : un second clic pendant une installation est expliqué dans le bandeau du Store")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
