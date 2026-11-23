#!/usr/bin/python3
"""#3315 : un panneau de la barre des tâches fermé ne réveille plus son processus.

Sous Xvfb : chaque panneau (son, réglages rapides, horloge, alimentation) est
ouvert, caché, puis rouvert. Caché, son minuteur de rafraîchissement doit être
arrêté et le fil principal ne doit presque plus se réveiller ; rouvert, le
minuteur doit repartir. Sans Xvfb ou sans GTK, le test le dit et s'arrête."""
import os
import runpy
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if os.environ.get("WINLUX_POPUP_CAS"):
    kind = os.environ["WINLUX_POPUP_CAS"]
    m = runpy.run_path(str(HERE / "winlux-popup.py"), run_name="popup_repos")
    from gi.repository import GLib
    app, res = m["Popup"](kind), {}
    def reveils():
        for line in open("/proc/self/status"):
            if line.startswith("voluntary_ctxt_switches"): return int(line.split()[1])
    def cacher():
        res["ouvert"] = bool(app.refresh_source)
        app.hide(); GLib.timeout_add(3000, debut); return False
    def debut():
        res["t0"] = reveils(); GLib.timeout_add(10000, rouvrir); return False
    def rouvrir():
        res["caché"] = reveils() - res["t0"]; res["minuteur caché"] = bool(app.refresh_source)
        app.do_activate(); GLib.timeout_add(1500, fin); return False
    def fin():
        print(f"{kind} {res['ouvert']} {res['caché']} {res['minuteur caché']} {bool(app.refresh_source)}"); app.quit(); return False
    app.connect("activate", lambda *_: GLib.timeout_add(1500, cacher))
    app.run([])
    raise SystemExit(0)

if not shutil.which("Xvfb"):
    print("(Xvfb absent : panneaux non vérifiés ici)"); raise SystemExit(0)
try:
    subprocess.run([sys.executable, "-c", "import gi; gi.require_version('Gtk','3.0'); from gi.repository import Gtk"], check=True, capture_output=True, timeout=30)
except (subprocess.SubprocessError, OSError):
    print("(GTK absent : panneaux non vérifiés ici)"); raise SystemExit(0)
for numero in range(80, 120):
    if not Path(f"/tmp/.X11-unix/X{numero}").exists(): break
serveur = subprocess.Popen(["Xvfb", f":{numero}", "-nolisten", "tcp"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
problems = []
try:
    for _ in range(100):
        if Path(f"/tmp/.X11-unix/X{numero}").exists(): break
        time.sleep(0.05)
    for kind in ("volume", "quick", "calendar", "power"):
        env = dict(os.environ, DISPLAY=f":{numero}", NO_AT_BRIDGE="1", WINLUX_POPUP_CAS=kind)
        sortie = subprocess.run([sys.executable, "-B", __file__], env=env, capture_output=True, text=True, timeout=60).stdout.split()
        print("panneau, minuteur ouvert, réveils en 10 s caché, minuteur caché, minuteur rouvert :", sortie)
        if len(sortie) != 5:
            problems.append(f"{kind} : pas de mesure"); continue
        _, ouvert, reveils, cache, rouvert = sortie
        if ouvert != "True": problems.append(f"{kind} : pas de rafraîchissement panneau ouvert")
        if cache != "False": problems.append(f"{kind} : minuteur encore actif panneau caché")
        if int(reveils) > 4: problems.append(f"{kind} : {reveils} réveils en 10 s panneau caché")
        if rouvert != "True": problems.append(f"{kind} : minuteur non relancé à la réouverture")
finally:
    serveur.kill(); serveur.wait()
for problem in problems:
    print("ÉCHEC :", problem)
if problems:
    raise SystemExit(1)
print("Panneau fermé : plus de réveil chaque seconde, rafraîchissement relancé à l'ouverture (#3315).")
