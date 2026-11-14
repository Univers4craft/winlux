#!/usr/bin/python3
"""reboot, poweroff, halt et shutdown passent par winlux-power (#2273).

Sur Ubuntu, systemd-sysv fait de ces commandes des liens vers systemctl ; notre
shim systemctl ne regardait pas son nom d'appel et affichait son usage. On
appelle ici le shim sous chacun de ces noms, avec un faux winlux-power qui
note ses arguments : seul un arrêt immédiat doit l'appeler.
"""
import os
from pathlib import Path
import subprocess
import tempfile

shim = Path(__file__).resolve().with_name("winlux-systemctl.sh")

with tempfile.TemporaryDirectory() as temp:
    temp = Path(temp)
    tools = temp / "bin"
    tools.mkdir()
    calls = temp / "calls.txt"
    fake = tools / "winlux-power"
    fake.write_text(f'#!/bin/sh\nprintf "%s\\n" "$*" >>"{calls}"\n')
    fake.chmod(0o755)
    for name in ("reboot", "poweroff", "halt", "shutdown", "systemctl"):
        (tools / name).symlink_to(shim)
    env = dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}")

    def run(*argv):
        calls.unlink(missing_ok=True)
        result = subprocess.run([str(tools / argv[0]), *argv[1:]], env=env, capture_output=True,
                                text=True, timeout=20)
        called = calls.read_text().splitlines() if calls.exists() else []
        return result, called

    expected = {
        ("reboot",): ["execute reboot"],
        ("reboot", "-f"): ["execute reboot"],
        ("poweroff",): ["execute poweroff"],
        ("poweroff", "--reboot"): ["execute reboot"],
        ("halt",): ["execute poweroff"],
        ("halt", "-p"): ["execute poweroff"],
        ("shutdown", "-r", "now"): ["execute reboot"],
        ("shutdown", "-h", "now"): ["execute poweroff"],
        ("shutdown", "now", "maintenance"): ["execute poweroff"],
        ("systemctl", "reboot"): ["execute reboot"],
        ("systemctl", "poweroff"): ["execute poweroff"],
        ("systemctl", "halt"): ["execute poweroff"],
    }
    for argv, want in expected.items():
        result, called = run(*argv)
        assert "Usage: systemctl" not in result.stdout + result.stderr, (argv, result)
        assert result.returncode == 0 and called == want, (argv, result, called)

    # Rien ne doit s'éteindre : journal wtmp seul, arrêt différé, annulation, option inconnue.
    for argv in (("reboot", "-w"), ("shutdown",), ("shutdown", "-r", "+5"), ("shutdown", "-h", "22:00"),
                 ("shutdown", "-c"), ("reboot", "--zz")):
        result, called = run(*argv)
        assert called == [], (argv, result, called)
        assert argv == ("reboot", "-w") or result.returncode != 0, (argv, result)

    # Refus de winlux-power (mise à jour en cours) : le code de retour remonte.
    fake.write_text('#!/bin/sh\necho "Une mise à jour est en cours." >&2\nexit 1\n')
    result, _ = run("reboot")
    assert result.returncode == 1 and "mise à jour" in result.stderr, result

print("test-systemctl-power : OK (reboot, poweroff, halt, shutdown, systemctl reboot)")
