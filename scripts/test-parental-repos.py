#!/usr/bin/python3
"""Contrôle parental au repos (Gain #3316).

Avant :
- le service se réveillait chaque seconde, même sans aucun compte surveillé ;
- la supervision, qui ne fait qu'attendre le service, gardait tout le
  programme compilé en mémoire (≈ 24 Mo, autant que le service).

Vérifié, sans root, dans une racine factice :
1. Sans compte surveillé, le service se réveille moins d'une fois toutes les
   2 s (une relecture des réglages toutes les 10 s).
2. Un compte mis sous surveillance est pris en compte comme avant, à la
   relecture suivante (au plus ≈ 10 s). Le service reprend alors son tour
   d'une seconde.
3. La supervision garde son pid (le service reste son enfant), occupe moins
   de 16 Mo, et relance le service tué.
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
work = Path(tempfile.mkdtemp(prefix="winlux-parental-repos."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label, flush=True)
    if not condition:
        failures.append(label)


(work / "etc/winlux").mkdir(parents=True)
(work / "run/winlux/sessions").mkdir(parents=True)
(work / "etc/passwd").write_text("root:x:0:0:root:/root:/bin/bash\nalice:x:1001:1001:Alice:/home/alice:/bin/bash\n"
                                 "bob:x:1002:1002:Bob:/home/bob:/bin/bash\n")
(work / "etc/group").write_text("sudo:x:27:alice\n")
(work / "etc/winlux/users.conf").write_text("alice:root:shadow\nbob:user:shadow\n")
loader = SourceFileLoader("winlux_parental_repos", str(TOOL))
spec = importlib.util.spec_from_loader("winlux_parental_repos", loader)
lib = importlib.util.module_from_spec(spec)
loader.exec_module(lib)


def status(pid, key):
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith(key + ":"):
            return int(line.split()[1])
    return 0


def wakeups(pid):
    return status(pid, "voluntary_ctxt_switches") + status(pid, "nonvoluntary_ctxt_switches")


def children(pid):
    out = []
    for entry in os.listdir("/proc"):
        if entry.isdigit():
            try:
                stat = Path(f"/proc/{entry}/stat").read_text().rsplit(")", 1)[1].split()
            except OSError:
                continue
            if int(stat[1]) == pid and stat[0] != "Z":
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


log = open(work / "journal", "w")
sup = subprocess.Popen([sys.executable, "-B", str(TOOL), "supervise"], stdout=log, stderr=log,
                       env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1"))
try:
    daemon = wait_for(lambda: children(sup.pid), 10)
    ok(bool(daemon), "la supervision lance le service, toujours son enfant")
    daemon = daemon[0] if daemon else None
    time.sleep(1.5)
    rss = status(sup.pid, "VmRSS")
    print(f"supervision : {rss} kio ; service : {status(daemon, 'VmRSS') if daemon else '?'} kio")
    ok(0 < rss < 16000, "la supervision n'a plus tout le programme en mémoire (< 16 Mo) (Gain #3316)")
    ok(sup.poll() is None, "la supervision tourne")

    # 1. Aucun compte surveillé.
    before = wakeups(daemon)
    time.sleep(8)
    idle = (wakeups(daemon) - before) / 8
    print(f"sans compte surveillé : {idle:.2f} réveil/s")
    ok(idle < 0.5, "sans compte surveillé : moins d'un réveil toutes les 2 s (Gain #3316)")

    # 2. Un compte mis sous surveillance : pris à la relecture suivante.
    lib.private_dir(lib.CONF_DIR)
    lib.write_private(lib.CONF_DIR / "bob.json", lib.default_config())
    ok(lib.supervised_accounts().keys() == {"bob"}, "bob est surveillé dans la racine factice")
    time.sleep(12)
    before = wakeups(daemon)
    time.sleep(4)
    busy = (wakeups(daemon) - before) / 4
    print(f"bob surveillé : {busy:.2f} réveil/s")
    ok(busy >= 0.75, "compte surveillé : pris en compte en au plus 10 s, tour d'une seconde repris")

    # 3. Service tué : relancé par la supervision allégée.
    os.kill(daemon, signal.SIGKILL)
    again = wait_for(lambda: [pid for pid in children(sup.pid) if pid != daemon], 10)
    ok(bool(again), "service tué : relancé par la supervision")
finally:
    sup.terminate()
    try:
        sup.wait(timeout=10)
    except subprocess.TimeoutExpired:
        sup.kill()
    for pid in children(sup.pid):
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass
    log.close()
ok(sup.returncode == 0, "SIGTERM : la supervision s'arrête proprement")
journal = (work / "journal").read_text(errors="replace")
ok("supervision : service arrêté (code -9)" in journal, "arrêt du service noté au journal")
shutil.rmtree(work, ignore_errors=True)
if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("contrôle parental au repos : OK")
