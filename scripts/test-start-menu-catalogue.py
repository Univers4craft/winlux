#!/usr/bin/python3
"""Menu Démarrer : le catalogue des applications n'est plus relu à chaque
ouverture (#3121).

Ce qui est vérifié : menu préchargé, cinq ouvertures n'appellent plus
Gio.AppInfo.get_all (une seule lecture, au préchargement) ; un lanceur ajouté
pendant que le menu est caché apparaît à l'ouverture suivante, relu au repos
avant elle ; un lanceur retiré disparaît de même.

Ce qui est supposé : Gio.AppInfoMonitor signale les lanceurs ajoutés ou
retirés dans les dossiers « applications » de XDG_DATA_HOME et XDG_DATA_DIRS
(documentation de GIO), comme ici pour XDG_DATA_HOME.

À lancer dans un Xvfb et un bus de session dédiés :
  xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session -- /usr/bin/python3 -B scripts/test-start-menu-catalogue.py
"""
import json
import os
import runpy
import sys
import tempfile
import time
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "winlux-start-menu.py"
failures = []


def ok(condition, message):
    print(("OK    " if condition else "ÉCHEC ") + message)
    if not condition:
        failures.append(message)


tmp = Path(tempfile.mkdtemp(prefix="winlux-catalogue-")); home = tmp / "home"
os.environ.update(HOME=str(home), WINLUX_LANG="fr", WINLUX_OSK_ROOT=str(tmp), XDG_DATA_HOME=str(home / ".local/share"))
os.environ.pop("XDG_CONFIG_HOME", None)
(tmp / "proc/bus/input").mkdir(parents=True)
(tmp / "proc/bus/input/devices").write_text('N: Name="AT Translated Set 2 keyboard"\nH: Handlers=kbd event0\nB: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe\n\n')
apps_dir = home / ".local/share/applications"; apps_dir.mkdir(parents=True)
for i in range(20):
    (apps_dir / f"e{i:02d}.desktop").write_text(f"[Desktop Entry]\nType=Application\nName=Application {i:02d}\nExec=true\nCategories=Utility;\n")
(home / ".config/winlux").mkdir(parents=True); (home / ".config/winlux/start.json").write_text(json.dumps({"pins": [], "recent": []}))

m = runpy.run_path(str(SCRIPT), run_name="catalogue")
Gio, GLib = m["Gio"], m["GLib"]
calls = [0]
real_get_all = Gio.AppInfo.get_all
def counted():
    calls[0] += 1
    return real_get_all()
Gio.AppInfo.get_all = staticmethod(counted)

app = m["Start"](); app.preloading = True
log = {}


def spin(condition, seconds):
    context = GLib.MainContext.default()
    end = time.monotonic() + seconds
    while time.monotonic() < end and not condition():
        context.iteration(False); time.sleep(0.01)
    return condition()


def names():
    return {a.get_display_name() for a in getattr(app, "apps", {}).values()}


def scenario():
    if not app.window: return True
    try:
        log["preload"] = calls[0]
        for _ in range(5):
            app.activate(); spin(lambda: False, 0.15)
            app.hide(); spin(lambda: False, 0.1)
        log["five_opens"] = calls[0] - log["preload"]
        before = calls[0]
        (apps_dir / "nouveau.desktop").write_text("[Desktop Entry]\nType=Application\nName=Nouvelle Appli\nExec=true\nCategories=Utility;\n")
        log["reloaded_hidden"] = spin(lambda: calls[0] > before and "Nouvelle Appli" in names(), 10)
        log["added"] = "Nouvelle Appli" in names()
        reads = calls[0]
        app.activate(); spin(lambda: False, 0.15)
        log["open_after_add"] = calls[0] - reads
        app.search.set_text("nouvelle"); app.render()
        log["found"] = [a.get_display_name() for a in app.results]
        app.search.set_text(""); app.hide(); spin(lambda: False, 0.1)
        (apps_dir / "nouveau.desktop").unlink()
        log["removed"] = spin(lambda: "Nouvelle Appli" not in names(), 10)
    finally:
        app.quit()
    return False


GLib.timeout_add(300, scenario)
GLib.timeout_add(60000, lambda: app.quit() or False)
app.run([sys.argv[0]])
print("relevé :", json.dumps(log, ensure_ascii=False))
ok(log.get("preload") == 1, "catalogue lu une fois au préchargement")
ok(log.get("five_opens") == 0, "cinq ouvertures sans relire le catalogue (Gio.AppInfo.get_all)")
ok(log.get("reloaded_hidden") and log.get("added"), "lanceur ajouté menu caché : catalogue relu au repos, avant l'ouverture")
ok(log.get("open_after_add") == 0 and "Nouvelle Appli" in log.get("found", []),
   "ouverture suivante : la nouvelle application est trouvée sans relecture à l'ouverture")
ok(log.get("removed"), "lanceur retiré : l'application disparaît du catalogue")
raise SystemExit(1 if failures else 0)
