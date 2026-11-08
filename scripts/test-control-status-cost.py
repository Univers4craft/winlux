#!/usr/bin/python3
"""Relevé d'état périodique léger (winlux-control status toutes les 3 s).

Mesure VM du 7 novembre 2026 : 186 ms par appel, 6,56 % d'un cœur en
continu, surtout en processus lancés (sys > user) : cat, head, sed, awk
pour chaque valeur, et des sondes rares relancées toutes les 3 s.

1. status() ne lance plus cat/head/sed/awk pour lire une valeur.
2. value() et first() gardent exactement l'ancien comportement.
3. Relevé périodique : peu de programmes lancés (compté avec strace si présent).
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/winlux-control.sh"
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


source = SCRIPT.read_text()
body = source[source.index("\nstatus() {"):source.index("\nupdates_current() {")]
ok(not [l for l in body.splitlines() if "| head -n 1" in l and "asound/pcm" not in l],
   "status : plus de « | head -n 1 » (sauf repli sans wpctl)")
ok(not re.search(r"\$\(cat ", body), "status : plus de $(cat …) pour lire une valeur")
ok("| sed 's/^$/" not in body, "status : valeurs par défaut sans sed")
ok(not re.search(r"\| awk|awk '", body.split("amixer")[0]), "status : plus d'awk pour /proc")
for probe in ("cached sysupdate", "cached update-auto", "cached cpu-report", "cached boot-time"):
    ok(probe in source, f"sonde rare en cache : {probe}")
ok(source.count("find \"$mine/status-cache\"") >= 1 and "-mmin -1 2>/dev/null)\" " not in source,
   "un seul find par relevé pour tout le cache")

# value()/first() : mêmes réponses que l'ancien sed | head.
helpers = source[source.index("value() {"):source.index("pref() {")]
with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    conf = work / "a.conf"
    conf.write_text("A=1\nEMPTY=\nB=x=y\nA=2\n  C=3\nD=sans fin de ligne")
    cases = {
        "value A": "1", "value EMPTY": "", "value EMPTY fr": "fr", "value MISSING fr": "",
        "value B": "x=y", "value C": "", "value D": "sans fin de ligne",
    }
    for call, expected in cases.items():
        name, key, *rest = call.split()
        script = helpers + f'\nvalue "{conf}" {key} {" ".join(rest)}\n'
        got = subprocess.run(["sh", "-c", script], capture_output=True, text=True).stdout.rstrip("\n")
        old = subprocess.run(["sh", "-c", f'sed -n "s/^{key}=//p" "{conf}" | head -n 1'],
                             capture_output=True, text=True).stdout.rstrip("\n")
        if rest and old == "" and subprocess.run(["grep", "-q", f"^{key}=", str(conf)]).returncode == 0:
            old = rest[0]
        ok(got == expected == old, f"{call} → {got!r} (ancien {old!r})")
    single = work / "one"
    single.write_text("Asia/Tokyo\n")
    for args, expected in ((f'"{single}"', "Asia/Tokyo"), (f'"{work}/absent" 0', "0"), (f'"{work}/absent"', "")):
        got = subprocess.run(["sh", "-c", helpers + f"\nfirst {args}\n"], capture_output=True, text=True).stdout.rstrip("\n")
        ok(got == expected, f"first {args.split('/')[-1]} → {got!r}")

# Relevé périodique complet dans une racine simulée.
with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    root = work / "root"
    for sub in ("bin", "root/etc/winlux", "root/usr/share/winlux", "root/proc/bus/input", "root/run/winlux/hw"):
        (work / sub).mkdir(parents=True)
    (root / "usr/share/winlux/version").write_text("v-test\n")
    (root / "proc/bus/input/devices").write_text('N: Name="A"\nH: Handlers=event5 js0\nN: Name="B"\n')
    (root / "proc/mounts").write_text("/dev/vda / ext4 rw 0 0\n")
    (root / "etc/winlux/input.conf").write_text("XKB_LAYOUT=\n")
    (root / "run/winlux/hw/cpu-threads").write_text("8\n")
    mocks = {
        "ip": "case \"$*\" in '-4 route show default') echo 'default via 10.0.2.2 dev eth0';; "
              "'-4 -o addr show dev eth0') echo '2: eth0 inet 10.0.2.15/24 brd x';; esac",
        "wpctl": "case \"$1\" in get-volume) echo 'Volume: 0.40';; esac",
        "winlux-update": "case \"$*\" in --busy) exit 1;; '--auto status') echo enabled;; esac",
        "winlux": "case \"$1\" in --cpu-report) echo cap=1;; --boot-time) echo boot=2;; esac",
        "winlux-sysupdate": "echo sys_updates=0",
        "id": "echo 0",
    }
    for name, body_ in mocks.items():
        (work / "bin" / name).write_text("#!/bin/sh\n" + body_ + "\n")
        (work / "bin" / name).chmod(0o755)
    env = {**os.environ, "PATH": f"{work}/bin:/usr/bin:/bin", "WINLUX_CONTROL_ROOT": str(root)}
    subprocess.run(["sh", str(SCRIPT), "status"], env=env, check=True)
    control = (root / "run/winlux/control.txt").read_text()
    for line in ("version=v-test", "layout=fr", "persistent=1", "network=eth0 10.0.2.15/24",
                 "audio=Volume: 0.40", "input_devices=2", "gamepads=1", "cpu_threads=8", "update_auto=enabled"):
        ok(f"\n{line}\n" in f"\n{control}", f"relevé : {line}")
    ok("cap=1" in (root / "run/winlux/cpu.txt").read_text(), "relevé : capacités processeur publiées")
    if shutil.which("strace"):
        trace = work / "trace"
        subprocess.run(["strace", "-f", "-qq", "-e", "trace=execve", "-o", str(trace), "sh", str(SCRIPT), "status"],
                       env={**env, "WINLUX_CONTROL_PERIODIC": "1"}, check=True)
        count = trace.read_text().count("execve(")
        ok(count <= 25, f"relevé périodique : {count} programmes lancés (≤ 25, 106 avant)")
    else:
        print("—    strace absent : décompte des programmes non mesuré ici")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("OK: relevé d'état périodique léger")
