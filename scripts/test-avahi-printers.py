#!/usr/bin/python3
"""Imprimantes du réseau : avahi-daemon lancé avant CUPS (#623).

Relevé du 7 novembre 2026 sur le PC de damien (v0.2.19 installé) : sans
systemd, rien ne lançait avahi-daemon, donc les backends dnssd/driverless
de CUPS ne voyaient aucune imprimante Wi-Fi (IPP Everywhere, AirPrint).
On exécute winlux-system-services avec des démons simulés et on vérifie
l'ordre de lancement.
"""
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SERVICES = ROOT / "scripts/winlux-system-services.sh"
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


packages = (ROOT / "scripts/prepare-desktop-packages.py").read_text()
ok('"avahi-daemon"' in packages, "avahi-daemon installé explicitement dans l'image")

source = SERVICES.read_text()
start = source.index("# Découverte réseau (#623)")
end = source.index("has_bluetooth=false")
block = source[start:end]
ok(block.index("avahi-daemon --no-rlimits") < block.index("/usr/sbin/cupsd -f"),
   "avahi-daemon lancé avant cupsd")
ok("grep -q '^avahi:' /etc/passwd" in block, "compte avahi vérifié avant le lancement")

with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    log = work / "order.log"
    fake_root = work / "root"
    for name in ("usr/sbin", "etc", "run/winlux"):
        (fake_root / name).mkdir(parents=True)

    def run(passwd):
        (fake_root / "etc/passwd").write_text(passwd)
        log.write_text("")
        script = block
        for daemon in ("avahi-daemon", "cupsd", "cups-browsed"):
            fake = fake_root / "usr/sbin" / daemon
            fake.write_text(f'#!/bin/sh\necho "{daemon} $*" >> "{log}"\n')
            fake.chmod(0o755)
        for path in ("/usr/sbin/", "/etc/passwd", "/run/", "/var/"):
            script = script.replace(path, f"{fake_root}{path}")
        result = subprocess.run(["sh", "-eu", "-c", script + "\nwait\n"], capture_output=True, text=True, timeout=30,
                                env={**os.environ, "PATH": "/usr/bin:/bin"})
        return result, log.read_text().splitlines()

    result, order = run("root:x:0:0::/root:/bin/sh\navahi:x:110:118::/run/avahi-daemon:/usr/sbin/nologin\n")
    ok(result.returncode == 0, f"bloc des services exécuté ({result.stderr.strip()[:200]})")
    names = [line.split()[0] for line in order]
    ok(names[:1] == ["avahi-daemon"] and "cupsd" in names and "cups-browsed" in names,
       f"avahi-daemon, cupsd et cups-browsed lancés ({names})")
    ok(any(re.fullmatch(r"cupsd -f", line) for line in order), "cupsd au premier plan, comme avant")
    pid = fake_root / "run/winlux/avahi.pid"
    ok(pid.exists() and pid.read_text().strip().isdigit(), "pid d'avahi-daemon noté")

    result, order = run("root:x:0:0::/root:/bin/sh\n")
    names = [line.split()[0] for line in order]
    ok(result.returncode == 0 and "avahi-daemon" not in names and "cupsd" in names,
       f"sans compte avahi : CUPS lancé quand même ({names})")
    ok("compte avahi absent" in (fake_root / "run/winlux/avahi.log").read_text(), "raison notée dans avahi.log")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("OK: avahi-daemon lancé avant CUPS")
