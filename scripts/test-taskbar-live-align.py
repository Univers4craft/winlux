#!/usr/bin/python3
"""Barre centrée choisie en direct : la barre se range sans être relancée à la main.

À lancer dans un affichage X dédié (xvfb-run, 1920x1080). Démarre une vraie
xfce4-panel avec la configuration livrée (barre à gauche, contrepoids 13 au
bout), le vrai `winlux-tray`, puis applique `winlux-control align-center`
pendant que la barre tourne, comme Paramètres > Personnalisation > Barre des
tâches, et enfin `align-left`.

Avant le correctif (#651), la barre déjà lancée ne déplaçait pas le contrepoids
13 du bout de la barre vers la gauche de Démarrer : la zone de notification
restait décalée d'environ 240 px (vide à droite), et le groupe Démarrer
centré au quart de l'écran, jusqu'à un `xfce4-panel -r` manuel.
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

directory = Path(__file__).resolve().parent
project = directory.parent
shipped = project / "integration/native/config"
MARK = ("\n.xfce4-panel button, .xfce4-panel button:checked, .xfce4-panel button:hover { background: #ff00ff; "
        "border-color: #ff00ff; border-radius: 0; box-shadow: none; }\n")


def prepare(home):
    config = home / ".config"
    shutil.copytree(shipped, config)
    (config / "gtk-3.0/settings.ini").write_text("[Settings]\ngtk-theme-name=Adwaita\ngtk-enable-animations=false\n",
                                                 encoding="utf-8")
    with open(config / "gtk-3.0/gtk.css", "a", encoding="utf-8") as css:
        css.write(MARK)
    start = home / "winlux-start.desktop"
    start.write_text((project / "integration/native/applications/winlux-start.desktop").read_text(encoding="utf-8")
                     .replace("/usr/share/winlux/native/start.svg", str(project / "integration/native/start.svg")),
                     encoding="utf-8")
    for path in list((config / "xfce4").rglob("*.rc")) + list((config / "xfce4").rglob("*.xml")):
        text = path.read_text(encoding="utf-8")
        text = text.replace("/usr/bin/winlux-clock", f"/bin/sh {directory}/winlux-clock.sh")
        text = text.replace("/usr/share/applications/winlux-start.desktop", str(start))
        text = text.replace("/usr/bin/winlux-tray", f"{sys.executable} -B {directory}/winlux-tray.py")
        path.write_text(text, encoding="utf-8")


def layout(Gdk, root, width, height):
    """Bord gauche de Démarrer (premier bouton magenta) et dernier pixel clair (horloge)."""
    pixbuf = Gdk.pixbuf_get_from_window(root, 0, height - 40, width, 40)
    if pixbuf is None:
        return None, None
    data, stride, channels = pixbuf.get_pixels(), pixbuf.get_rowstride(), pixbuf.get_n_channels()
    start = None
    for x in range(width):
        red, green, blue = data[10 * stride + x * channels:10 * stride + x * channels + 3]
        if abs(red - 255) + green + abs(blue - 255) < 60:
            start = x
            break
    last = None
    for x in range(width - 1, -1, -1):
        column = [data[y * stride + x * channels:y * stride + x * channels + 3] for y in range(4, 36, 2)]
        if any(min(pixel) > 150 for pixel in column):
            last = x
            break
    return start, last


def settle(Gdk, root, width, height, seconds=20):
    """Attend une disposition stable pendant 5 s (20 s au plus)."""
    began, previous, since = time.monotonic(), None, time.monotonic()
    while time.monotonic() - began < seconds:
        current = layout(Gdk, root, width, height)
        if current != previous:
            previous, since = current, time.monotonic()
        elif time.monotonic() - since > 5 and None not in current:
            break
        time.sleep(0.1)
    return previous


def main():
    missing = [tool for tool in ("xfce4-panel", "openbox", "dbus-run-session", "xwininfo", "xprop", "xfconf-query")
               if not shutil.which(tool)]
    if missing:
        print("Barre réelle non vérifiée : " + ", ".join(missing) + " absent(s)")
        return 0
    import gi
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk
    problems = []
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temp:
        home = Path(temp) / "compte"
        prepare(home)
        fifo = Path(temp) / "commandes"
        os.mkfifo(fifo)
        env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"),
                   XDG_CACHE_HOME=str(home / ".cache"), XDG_DATA_HOME=str(home / ".local/share"),
                   WINLUX_TRAY_DIR=str(home / "tray"), WINLUX_LANG="fr", WINLUX_CONTROL_ROOT=str(Path(temp) / "racine"),
                   WINLUX_CONTROL_NO_STATUS="1", WINLUX_CONTROL_TEST_LOG=str(Path(temp) / "journal"))
        # winlux-control doit joindre le même xfconfd que la barre : il est lancé
        # dans la même session D-Bus, à la demande, par la file « commandes ».
        script = (f"openbox & sleep 1; xfce4-panel --disable-wm-check & "
                  f"while read action < '{fifo}'; do sh '{directory}/winlux-control.sh' \"$action\" >/dev/null 2>&1; done")
        process = subprocess.Popen(["dbus-run-session", "--", "sh", "-c", script], env=env, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL, start_new_session=True)
        try:
            root = Gdk.get_default_root_window()
            width, height = root.get_width(), root.get_height()
            start, last = settle(Gdk, root, width, height)
            print(f"barre livrée : Démarrer x={start}, bord droit x={last}")
            for action in ("align-center", "align-left"):
                with open(fifo, "w") as pipe:
                    pipe.write(action + "\n")
                start, last = settle(Gdk, root, width, height)
                print(f"{action} en direct : Démarrer x={start}, bord droit x={last}")
                if last is None or last < width - 30:
                    problems.append(f"{action} en direct : zone de notification décalée, bord droit à x={last} sur {width}")
                if action == "align-center" and not (start is not None and width // 2 - 160 < start < width // 2):
                    problems.append(f"{action} en direct : Démarrer à x={start}, pas au milieu de l'écran")
                if action == "align-left" and not (start is not None and start < 20):
                    problems.append(f"{action} en direct : Démarrer à x={start}, pas à gauche")
        finally:
            try:
                os.killpg(process.pid, 15)
            except ProcessLookupError:
                pass
            process.wait(timeout=10)
    for problem in problems:
        print("ÉCHEC : " + problem, file=sys.stderr)
    if not problems:
        print("Alignement de la barre appliqué en direct, sans relance manuelle.")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
