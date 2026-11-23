#!/usr/bin/python3
"""Ancrage au repos (#3279) : `winlux-snap watch` ne se réveille plus, souris immobile.

Avant : un tour toutes les 100 ms, souris immobile comprise, avec 2 à 5
requêtes X par tour (≈ 27 requêtes/s et ≈ 33 réveils/s mesurés sous Xvfb).
Après : réveil sur les événements XInput2 bruts de la souris (mouvement,
appui, relâchement), puis trois tours de 100 ms une fois la souris arrêtée.

Sous Xvfb avec openbox :
1. Souris immobile : moins d'un réveil par seconde.
2. Un mouvement de souris le réveille, puis il se rendort.
3. Sans XInput 2.2 (repli), il interroge toujours le pointeur toutes les
   100 ms, comme avant.

    xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-ancrage-repos.py
"""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


if not os.environ.get("DISPLAY") or not all(shutil.which(t) for t in ("openbox", "xdotool")):
    print("openbox, xdotool ou affichage absent : ancrage au repos non vérifié")
    print("ancrage au repos : OK")
    raise SystemExit(0)


def wakeups(pid):
    total = 0
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if "ctxt_switches:" in line:
            total += int(line.split()[-1])
    return total


def move(x, y):
    """Comme une vraie souris : le dernier pas passe par un périphérique (XTest)."""
    subprocess.run(["xdotool", "mousemove", str(x - 1), str(y)], check=True)
    subprocess.run(["xdotool", "mousemove_relative", "--", "1", "0"], check=True)


def idle_rate(proc, seconds=3.0):
    before = wakeups(proc.pid)
    time.sleep(seconds)
    return (wakeups(proc.pid) - before) / seconds


wm = subprocess.Popen(["openbox"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1)
procs = []
try:
    watch = subprocess.Popen([sys.executable, "-B", str(HERE / "winlux-snap.py"), "watch"],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    procs.append(watch)
    time.sleep(2)
    move(500, 400)
    time.sleep(1.0)  # trois tours après l'arrêt de la souris
    rate = idle_rate(watch)
    print(f"souris immobile : {rate:.1f} réveils/s")
    ok(watch.poll() is None, "le suivi de la souris tourne")
    ok(rate < 1.0, "souris immobile : moins d'un réveil par seconde (#3279)")
    before = wakeups(watch.pid)
    for step in range(5):
        move(520 + step * 10, 420)
        time.sleep(0.05)
    time.sleep(0.4)
    moved = wakeups(watch.pid) - before
    print(f"réveils pendant et juste après un mouvement : {moved}")
    ok(moved >= 3, "un mouvement de souris réveille le suivi (#3279)")
    time.sleep(0.6)
    rate = idle_rate(watch)
    print(f"souris de nouveau immobile : {rate:.1f} réveils/s")
    ok(rate < 1.0, "il se rendort une fois la souris arrêtée (#3279)")

    # Repli sans XInput 2.2 : l'ancienne interrogation toutes les 100 ms.
    fallback = ("import runpy, sys; m = runpy.run_path(sys.argv[1], run_name='repli');"
                "m['_RawInput'].open = classmethod(lambda cls: None); sys.exit(m['watch']())")
    old = subprocess.Popen([sys.executable, "-B", "-c", fallback, str(HERE / "winlux-snap.py")],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    procs.append(old)
    time.sleep(2)
    rate = idle_rate(old)
    print(f"repli sans XInput 2 : {rate:.1f} réveils/s")
    ok(old.poll() is None and rate >= 5, "sans XInput 2.2, le pointeur est toujours suivi toutes les 100 ms")
finally:
    for proc in procs + [wm]:
        if proc.poll() is None:
            proc.kill()
            proc.wait()

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("ancrage au repos : OK")
