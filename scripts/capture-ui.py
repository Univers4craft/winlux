#!/usr/bin/python3
"""Captures d'écran de l'interface Winlux pour repérer les régressions visuelles.

À lancer dans un affichage X dédié (xvfb-run -s '-screen 0 1920x1080x24'),
jamais sur le bureau de l'utilisateur : chaque application est ouverte seule,
photographiée quand l'image ne bouge plus, puis fermée.

    capture-ui.py DOSSIER [--only nom1,nom2] [--runtime target/release/winlux]
                  [--testdir target/x86_64-pc-windows-msvc/release]

Les applications maison PE (Explorateur, Store…) ne sont capturées que si le
runtime et gui_demo.exe sont présents. Le dossier reçoit un PNG par écran et
captures.json (liste, erreurs). Code de sortie 1 si un écran attendu n'a pas
pu s'afficher (application plantée au démarrage par exemple).
"""
import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import signal
import subprocess
import sys
import tempfile
import time

import gi
gi.require_version("Gdk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import Gdk, GdkPixbuf  # noqa: E402

project = Path(__file__).resolve().parent.parent
scripts = project / "scripts"
python = sys.executable or "/usr/bin/python3"


def targets(runtime, testdir):
    """(nom, commande, variables d'environnement) de chaque écran capturé."""
    found = []
    settings = runpy.run_path(str(scripts / "winlux-settings.py"), run_name="capture_ui")
    for page in range(len(settings["PAGES"])):
        found.append((f"parametres-{page:02d}", [python, "-B", str(scripts / "winlux-settings.py")],
                      {"WINLUX_SETTINGS_PAGE": str(page)}))
    found.append(("menu-demarrer", [python, "-B", str(scripts / "winlux-start-menu.py")], {}))
    for kind in ("volume", "power"):
        found.append((f"popup-{kind}", [python, "-B", str(scripts / "winlux-popup.py"), kind], {}))
    for page in ("quick", "notifications", "tasks", "appearance", "apps", "files", "recovery"):
        found.append((f"outils-{page}", [python, "-B", str(scripts / "winlux-system-tools.py"), page], {}))
    found.append(("bienvenue", [python, "-B", str(scripts / "winlux-welcome.py")], {}))
    demo = Path(testdir) / "gui_demo.exe"
    if Path(runtime).is_file() and demo.is_file():
        pe = {"WINLUX_AUDIO": "null", "WINLUX_NO_SPLASH": "1", "WINLUX_NO_LOGIN": "1", "WINLUX_NO_OOBE": "1"}
        found.append(("bureau-pe", [str(runtime), str(demo)], dict(pe, WINLUX_DESKTOP_DEMO="1")))
        for app in ("explorer", "store", "fonts", "taskschd", "firewall"):
            found.append((f"appli-{app}", [str(runtime), str(demo)],
                          dict(pe, WINLUX_NATIVE_SHELL="1", WINLUX_SHELL_APP=app)))
    return found


def grab():
    root = Gdk.get_default_root_window()
    pixbuf = Gdk.pixbuf_get_from_window(root, 0, 0, root.get_width(), root.get_height())
    if pixbuf is None:
        raise RuntimeError("Capture de l'écran impossible")
    return pixbuf


def pixels(pixbuf):
    return pixbuf.get_pixels()


def isolated_home(directory):
    """Profil vierge : thème Winlux, pas de clignotement ni d'animation GTK."""
    config = directory / "config"
    gtk = config / "gtk-3.0"
    shutil.copytree(project / "integration/native/config/gtk-3.0", gtk)
    with open(gtk / "settings.ini", "a", encoding="utf-8") as settings:
        settings.write("gtk-cursor-blink=false\ngtk-enable-animations=false\n")
    for name in ("state", "data", "cache", "home"):
        (directory / name).mkdir()
    return {"XDG_CONFIG_HOME": str(config), "XDG_STATE_HOME": str(directory / "state"),
            "XDG_DATA_HOME": str(directory / "data"), "XDG_CACHE_HOME": str(directory / "cache"),
            "HOME": str(directory / "home"), "LANG": "fr_FR.UTF-8", "WINLUX_LANG": "fr",
            "NO_AT_BRIDGE": "1", "GTK_A11Y": "none", "TZ": "UTC"}


def capture(name, command, extra, base_env, blank, output, timeout):
    environment = dict(os.environ, **base_env, **extra)
    log = output / f"{name}.log"
    with open(log, "w", encoding="utf-8") as stream:
        process = subprocess.Popen(command, env=environment, stdout=stream, stderr=subprocess.STDOUT,
                                   stdin=subprocess.DEVNULL, start_new_session=True)
    try:
        deadline = time.monotonic() + timeout
        previous, shown = None, None
        # L'écran doit avoir changé, puis rester identique sur deux images
        # successives (fin des fondus, du chargement des icônes…).
        while time.monotonic() < deadline:
            time.sleep(0.5)
            if process.poll() is not None:
                return f"l'application s'est arrêtée (code {process.returncode}), voir {log.name}"
            current = grab()
            data = pixels(current)
            if data != blank:
                if data == previous:
                    shown = current
                    break
                shown = current
            previous = data
        if shown is None:
            return f"aucune fenêtre affichée en {timeout:.0f} s, voir {log.name}"
        shown.savev(str(output / f"{name}.png"), "png", [], [])
        return None
    finally:
        try:
            os.killpg(process.pid, signal.SIGTERM)
            process.wait(timeout=5)
        except (ProcessLookupError, subprocess.TimeoutExpired):
            try: os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError: pass
            process.wait()
        # Attendre que l'écran redevienne vide avant l'application suivante.
        for _ in range(20):
            if pixels(grab()) == blank: break
            time.sleep(0.1)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("output")
    parser.add_argument("--only", default="")
    parser.add_argument("--runtime", default=str(project / "target/release/winlux"))
    parser.add_argument("--testdir", default=str(project / "target/x86_64-pc-windows-msvc/release"))
    parser.add_argument("--timeout", type=float, default=float(os.environ.get("WINLUX_CAPTURE_TIMEOUT", "25")))
    args = parser.parse_args()
    if not os.environ.get("DISPLAY"):
        raise SystemExit("Aucun affichage : lancer avec xvfb-run")
    if subprocess.run(["wmctrl", "-m"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
        raise SystemExit("Un gestionnaire de fenêtres tourne déjà : utiliser un Xvfb dédié")
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    wanted = {name for name in args.only.split(",") if name}
    with tempfile.TemporaryDirectory(prefix="winlux-captures.") as temporary:
        base_env = isolated_home(Path(temporary))
        manager = subprocess.Popen(["openbox", "--sm-disable", "--config-file",
                                    str(project / "integration/native/config/openbox/rc.xml")],
                                   env=dict(os.environ, **base_env),
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            for _ in range(50):
                if subprocess.run(["wmctrl", "-m"], stdout=subprocess.DEVNULL,
                                  stderr=subprocess.DEVNULL).returncode == 0: break
                time.sleep(0.1)
            time.sleep(0.5)
            blank = pixels(grab())
            report = {"screen": [Gdk.get_default_root_window().get_width(),
                                 Gdk.get_default_root_window().get_height()],
                      "captures": [], "errors": {}}
            for name, command, extra in targets(args.runtime, args.testdir):
                if wanted and name not in wanted: continue
                started = time.monotonic()
                error = capture(name, command, extra, base_env, blank, output, args.timeout)
                spent = time.monotonic() - started
                if error:
                    report["errors"][name] = error
                    print(f"ÉCHEC {name} : {error}", flush=True)
                else:
                    report["captures"].append(name)
                    (output / f"{name}.log").unlink(missing_ok=True)
                    print(f"OK {name} ({spent:.1f} s)", flush=True)
        finally:
            manager.terminate()
            manager.wait(timeout=5)
    (output / "captures.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{len(report['captures'])} capture(s), {len(report['errors'])} échec(s) dans {output}")
    return 1 if report["errors"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
