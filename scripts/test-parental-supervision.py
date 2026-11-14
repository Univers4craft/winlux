#!/usr/bin/python3
"""Contrôle parental : le service est relancé s'il s'arrête (#2247).

Sans systemd, « winlux-parental supervise » lance le service et le relance
après chaque arrêt, avec un délai croissant s'il s'arrête aussitôt. Le test
remplace le service par de faux programmes et relève les délais demandés,
puis lance la vraie supervision et tue le service qu'elle a lancé.
Sans root : racine factice (WINLUX_PARENTAL_ROOT).
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-parental.py"
work = Path(tempfile.mkdtemp(prefix="winlux-parental-supervision."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"
failures = 0


def check(cond, label):
    global failures
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures += 1


loader = SourceFileLoader("winlux_parental", str(TOOL))
spec = importlib.util.spec_from_loader("winlux_parental", loader)
lib = importlib.util.module_from_spec(spec)
loader.exec_module(lib)

# 1. Un service qui s'arrête aussitôt : relancé, délai croissant borné.
counter = work / "lancements"
fake = [sys.executable, "-c", f"open({str(counter)!r}, 'a').write('x'); raise SystemExit(3)"]
delays, notes = [], []
lib.supervise(fake, notes.append, rounds=8, sleep=delays.append)
check(counter.read_text() == "x" * 8, "service arrêté : relancé à chaque fois (#2247)")
check(delays == [2, 5, 10, 30, 60, 60, 60, 60], f"délai croissant, au plus une minute ({delays})")
check(all("code 3" in note for note in notes), "chaque arrêt noté au journal avec son code")

# 2. Après une marche stable, le délai repart du plus court.
delays.clear()
lib.supervise([sys.executable, "-c", "pass"], lambda text: None, rounds=2, sleep=delays.append, stable=0)
check(delays == [1, 1], "service resté en marche : délai remis au plus court")

# 3. Programme introuvable : noté, jamais d'arrêt de la supervision.
notes.clear()
lib.supervise([str(work / "absent")], notes.append, rounds=2, sleep=lambda s: None)
check(len(notes) == 2 and "impossible de lancer" in notes[0], "service introuvable : noté et réessayé")

# 4. Vraie supervision : le service tué revient, SIGTERM arrête tout.
log = open(work / "journal", "w")
env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
sup = subprocess.Popen([sys.executable, "-B", str(TOOL), "supervise"], stdout=log, stderr=log, env=env)


def daemons():
    out = []
    for entry in os.listdir("/proc"):
        if entry.isdigit():
            try:
                cmd = Path(f"/proc/{entry}/cmdline").read_bytes().split(b"\0")
                stat = Path(f"/proc/{entry}/stat").read_text().rsplit(")", 1)[1].split()
            except OSError:
                continue
            if b"daemon" in cmd and str(TOOL).encode() in cmd and int(stat[1]) == sup.pid and stat[0] != "Z":
                out.append(int(entry))
    return out


def wait_for(predicate, seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.1)
    return None


first = wait_for(daemons, 10)
check(bool(first), "supervise lance le service")
if first:
    os.kill(first[0], signal.SIGKILL)
    second = wait_for(lambda: [pid for pid in daemons() if pid != first[0]], 10)
    check(bool(second), "service tué : relancé par la supervision")
sup.terminate()
try:
    sup.wait(timeout=10)
except subprocess.TimeoutExpired:
    sup.kill()
check(sup.returncode == 0 and wait_for(lambda: not daemons() or None, 5),
      "SIGTERM sur la supervision : service arrêté avec elle")
log.close()
journal = (work / "journal").read_text(errors="replace")
check("supervision : service arrêté (code -9)" in journal, "arrêt du service noté au journal")
for pid in daemons():
    os.kill(pid, signal.SIGKILL)
shutil.rmtree(work, ignore_errors=True)
print("test-parental-supervision :", "réussi" if failures == 0 else f"{failures} échec(s)")
sys.exit(1 if failures else 0)
