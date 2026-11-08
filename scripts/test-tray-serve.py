#!/usr/bin/env python3
"""Service résident de la barre des tâches (winlux-tray serve) et son lecteur.

Vérifie que :
  1. le service écrit, pour chaque greffon genmon, exactement la ligne que
     donnerait un calcul direct (statut, recherche, notifications, contrepoids) ;
  2. un second service pour le même compte s'arrête aussitôt (verrou) ;
  3. le lecteur /usr/bin/winlux-tray (scripts/winlux-tray-read.sh) recopie
     la sortie du service sans démarrer Python quand le service vit ;
  4. service absent, mort ou sortie manquante : le lecteur retombe sur un
     calcul direct, comme avant ;
  5. les sorties d'une session précédente sont effacées au démarrage du
     service (contrepoids faux au démarrage, #602), et rien n'est servi
     pendant le lancement de la barre ;
  6. le lecteur est bien plus rapide qu'un calcul en Python (mesure indicative).
"""

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


with tempfile.TemporaryDirectory() as directory:
    base = Path(directory)
    cache, tray_dir, supplies = base / "cache", base / "tray", base / "power"
    (supplies / "BAT0").mkdir(parents=True)
    (supplies / "BAT0/capacity").write_text("57\n")
    (supplies / "BAT0/status").write_text("Discharging\n")
    os.environ.update({"XDG_CACHE_HOME": str(cache), "WINLUX_TRAY_DIR": str(tray_dir),
                       "WINLUX_POWER_SUPPLY": str(supplies), "XDG_CONFIG_HOME": str(base / "config"),
                       "XDG_STATE_HOME": str(base / "state"), "WINLUX_LANG": "fr", "WINLUX_PANEL_THEME": "dark"})
    os.environ.pop("DISPLAY", None)

    tray = runpy.run_path(str(SCRIPTS / "winlux-tray.py"), run_name="winlux_tray")
    out = tray["output_directory"]()
    out.mkdir(parents=True)
    (out / "out-balance").write_text("<img>/ancienne/session.svg</img><tool></tool>\n")
    serve = tray["serve"]
    serve.__globals__["_panel_pid"] = lambda: os.getpid()
    # Barre en train de se lancer : le service attend, les greffons calculent.
    serve.__globals__["SETTLE"] = 10 ** 9
    ticks = iter(range(100))
    ok(serve(clock=lambda: float(next(ticks)), sleep=lambda _: None, rounds=2) == 0
       and not list(out.glob("out-*")), "service : rien de servi pendant le lancement de la barre")
    (out / "out-balance").write_text("<img>/ancienne/session.svg</img><tool></tool>\n")
    serve.__globals__["SETTLE"] = 0
    ok(serve(clock=lambda: float(next(ticks)), sleep=lambda _: None, rounds=1) == 0, "service : un tour sans erreur")
    for kind in ("status", "search", "notifications", "balance"):
        written = (out / f"out-{kind}").read_text().strip() if (out / f"out-{kind}").exists() else ""
        ok(written and written == tray["render"](kind), f"service : sortie « {kind} » identique au calcul direct")
    ok("ancienne" not in (out / "out-balance").read_text(), "service : sortie de la session précédente effacée (#602)")
    ok(not (out / "serve.pid").exists(), "service : fichier pid retiré à l'arrêt")

    # Verrou : un second service pour le même compte rend la main aussitôt.
    import fcntl
    with open(out / "serve.lock", "w") as held:
        fcntl.flock(held, fcntl.LOCK_EX)
        start = time.monotonic()
        ok(serve(clock=time.monotonic, sleep=lambda _: None, rounds=50) == 0 and time.monotonic() - start < 1,
           "service : un seul par compte (verrou)")

    # Lecteur : service vivant (un processus nommé winlux-tray) → recopie.
    fake = base / "bin/winlux-tray"
    fake.parent.mkdir()
    shutil.copy(shutil.which("sleep"), fake)
    alive = subprocess.Popen([str(fake), "30"])
    direct = base / "direct"
    direct.write_text("#!/bin/sh\necho \"direct $*\"\n")
    direct.chmod(0o755)
    env = {**os.environ, "WINLUX_TRAY_PROGRAM": str(direct), "WINLUX_TRAY_NO_SERVE": "1", "HOME": str(base)}
    reader = [str(SCRIPTS / "winlux-tray-read.sh")]
    try:
        (out / "serve.pid").write_text(f"{alive.pid}\n")
        status = "<img>/x/status.svg</img><tool>Wi-Fi : wlan0\nVolume : 40%\nBatterie : 57 %</tool><click>q</click>\n"
        (out / "out-status").write_text(status)
        result = subprocess.run(reader + ["status"], env=env, capture_output=True, text=True)
        ok(result.stdout == status, f"lecteur : sortie du service recopiée en entier, infobulle sur plusieurs lignes ({result.stdout!r})")
        (out / "out-search").unlink(missing_ok=True)
        result = subprocess.run(reader + ["search"], env=env, capture_output=True, text=True)
        ok(result.stdout == "direct search\n", f"lecteur : sortie absente → calcul direct ({result.stdout!r})")
        result = subprocess.run(reader + ["battery"], env=env, capture_output=True, text=True)
        ok(result.stdout == "direct battery\n", "lecteur : greffon non servi → calcul direct")

        # Mesure : 50 lectures contre 5 calculs directs en Python.
        start = time.monotonic()
        for _ in range(50):
            subprocess.run(reader + ["status"], env=env, capture_output=True)
        read_ms = (time.monotonic() - start) * 1000 / 50
        start = time.monotonic()
        for _ in range(5):
            subprocess.run([sys.executable, "-B", str(SCRIPTS / "winlux-tray.py"), "notifications"], capture_output=True)
        python_ms = (time.monotonic() - start) * 1000 / 5
        print(f"      lecture {read_ms:.1f} ms, calcul Python {python_ms:.1f} ms")
        ok(read_ms * 3 < python_ms, "lecteur : au moins trois fois plus rapide qu'un calcul Python")
    finally:
        alive.kill()
        alive.wait()
    # Service mort (pid d'un processus disparu) : calcul direct.
    result = subprocess.run(reader + ["status"], env=env, capture_output=True, text=True)
    ok(result.stdout == "direct status\n", "lecteur : service mort → calcul direct")

if failures:
    print(f"{len(failures)} échec(s)")
    sys.exit(1)
print("OK: service résident de la barre des tâches et son lecteur")
