#!/usr/bin/python3
"""Veille de l'écran au repos (#3291).

1. update_busy() répond comme `winlux-update --busy` (moteur APT) sans lancer
   de programme : drapeau de root dont le processus vit, ou verrou POSIX
   d'APT/dpkg vu dans /proc/locks.
2. SaverDpms : hors de la minute qui suit l'apparition d'XScreenSaver, plus
   de parcours de tout /proc tant que le démon connu vit ; s'il disparaît,
   le parcours revient et le DPMS est remis à la relance (#749).
"""
import fcntl
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
tool = runpy.run_path(str(ROOT / "scripts/winlux-screensaver-inhibit.py"), run_name="veille_repos")
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


busy = tool["update_busy"]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    (root / "run/winlux").mkdir(parents=True)
    (root / "var/lib/dpkg").mkdir(parents=True)
    (root / "proc").symlink_to("/proc")
    ok(busy(str(root)) is False, "rien en cours : pas occupé")

    # Drapeau : seulement celui de root, pas un lien, processus vivant.
    flag = root / "run/winlux/update-inhibit"
    flag.write_text(f"{os.getpid()}\n")
    ok(busy(str(root)) is (os.getuid() == 0), "drapeau de root, processus vivant : occupé (sinon refusé, #643)")
    flag.write_text("999999999\n")
    ok(busy(str(root)) is False, "drapeau d'un processus mort : pas occupé")
    flag.write_text("0\n")
    ok(busy(str(root)) is False, "drapeau à 0 : pas occupé")
    flag.unlink()
    target = root / "cible"
    target.write_text(f"{os.getpid()}\n")
    flag.symlink_to(target)
    ok(busy(str(root)) is False, "drapeau remplacé par un lien : ignoré")
    flag.unlink()

    # Verrou POSIX d'APT tenu par un autre processus.
    lock = root / "var/lib/dpkg/lock-frontend"
    lock.write_text("")
    holder = subprocess.Popen([sys.executable, "-c",
                               "import fcntl,sys,time\n"
                               f"f=open({str(lock)!r},'w'); fcntl.lockf(f, fcntl.LOCK_EX)\n"
                               "print('pris', flush=True); time.sleep(30)"],
                              stdout=subprocess.PIPE, text=True)
    try:
        holder.stdout.readline()
        ok(busy(str(root)) is True, "verrou POSIX sur lock-frontend : occupé")
    finally:
        holder.kill()
        holder.wait()
    time.sleep(0.1)
    ok(busy(str(root)) is False, "verrou relâché : pas occupé")

    # Un verrou FLOCK sur le même fichier n'est pas un verrou d'APT.
    with open(lock) as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        ok(busy(str(root)) is False, "verrou flock : pas pris pour APT (comme grep \" POSIX \")")

source = (ROOT / "scripts/winlux-screensaver-inhibit.py").read_text(encoding="utf-8")
body = source.split("def update_busy", 1)[1].split("\ndef ", 1)[0]
ok("subprocess" not in body and "winlux-update" not in body.split('"""', 2)[2],
   "update_busy ne lance aucun programme (#3291)")


class FakeX:
    def __init__(self):
        self.enabled, self.calls = True, []

    def __call__(self, *args):
        self.calls.append(args)
        if args == ("q",):
            return "DPMS is Enabled" if self.enabled else "DPMS is Disabled"
        if args == ("+dpms",):
            self.enabled = True
        return ""


x, clock, scans = FakeX(), [0.0], []
state = {"daemons": {4242}, "init": False, "alive": True}


def processes():
    scans.append(clock[0])
    return state["daemons"], state["init"]


saver = tool["SaverDpms"](xset=x, processes=processes, wanted=lambda: True, held=lambda: False,
                          clock=lambda: clock[0], alive=lambda pid: state["alive"] and pid in state["daemons"])
for second in range(0, 61, 3):
    clock[0] = second
    saver.tick()
during = len(scans)
for second in range(63, 63 + 600, 3):
    clock[0] = second
    saver.tick()
print(f"parcours de /proc : {during} pendant la première minute, {len(scans) - during} en 10 min ensuite")
ok(len(scans) - during == 0, "XScreenSaver connu et vivant : plus de parcours de /proc (#3291)")
state["daemons"], state["alive"] = {5151}, True
x.enabled = False
clock[0] = 700
saver.tick()
ok(len(scans) > during, "démon connu disparu : parcours repris")
ok(x.enabled, "XScreenSaver relancé : DPMS remis (#749)")

with tempfile.TemporaryDirectory() as directory:
    proc = Path(directory)
    (proc / "10").mkdir()
    (proc / "10/cmdline").write_bytes(b"/usr/bin/xscreensaver\0-no-splash\0")
    (proc / "11").mkdir()
    (proc / "11/cmdline").write_bytes(b"python3\0xscreensaver\0")
    alive = tool["saver_alive"]
    ok(alive(10, str(proc)) and not alive(11, str(proc)) and not alive(12, str(proc)),
       "saver_alive : seul le vrai démon xscreensaver compte")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("veille au repos : OK")
