#!/usr/bin/python3
"""Trousseau de secrets ouvert à la connexion, sans fenêtre (#1488).

Avec gnome-keyring et secret-tool installés, un vrai démon tourne sur un bus
de session isolé (HOME et XDG_RUNTIME_DIR temporaires) : winlux-keyring crée
le trousseau « login », un secret s'y range sans gcr-prompter, le trousseau
se rouvre avec le bon mot de passe après redémarrage du démon et refuse un
mauvais. L'écran de verrouillage transmet le mot de passe par un tube.
"""
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
HELPER = SCRIPTS / "winlux-keyring.py"
failures = []


def ok(cond, label):
    print(("ok   " if cond else "ÉCHEC ") + label)
    if not cond:
        failures.append(label)


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# Écran de verrouillage : mot de passe par l'entrée standard, jamais en argument.
lock = load("winlux_lock", SCRIPTS / "winlux-lock.py")
with tempfile.TemporaryDirectory() as tmp:
    fake = Path(tmp, "winlux-keyring")
    seen = Path(tmp, "seen")
    fake.write_text(f"#!/bin/sh\nprintf '%s|' \"$*\" >{seen}.tmp\ncat >>{seen}.tmp\nmv {seen}.tmp {seen}\n")
    fake.chmod(0o755)
    os.environ["WINLUX_KEYRING"] = str(fake)
    lock.open_keyring("mot de passe é")
    for _ in range(50):
        if seen.exists():
            break
        time.sleep(0.1)
    ok(seen.exists() and seen.read_text() == "unlock|mot de passe é",
       "verrouillage : mot de passe transmis au trousseau par un tube")
    del os.environ["WINLUX_KEYRING"]

missing = [tool for tool in ("gnome-keyring-daemon", "secret-tool", "dbus-run-session", "gdbus") if not shutil.which(tool)]
if missing:
    print("ignoré : démon réel (" + ", ".join(missing) + " absent)")
else:
    script = r'''
set -u
K="$PYTHON -I $HELPER unlock"
# Seul le démon de ce bus isolé est arrêté, jamais celui de l'utilisateur.
restart() {
    pid=$(gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.GetConnectionUnixProcessID org.freedesktop.secrets 2>/dev/null | sed -n 's/.*uint32 \([0-9]*\).*/\1/p')
    if test -n "$pid"; then kill "$pid"; while kill -0 "$pid" 2>/dev/null; do sleep 0.1; done; fi
}
printf '%s' "$PASSWORD" | $K >/dev/null || echo "create-failed"
printf s3cret | timeout 15 secret-tool store --label=essai service winlux-test key k || echo "store-failed"
test -f "$HOME/.local/share/keyrings/login.keyring" || echo "no-login-keyring"
restart
printf '%s' "$PASSWORD" | $K >/dev/null || echo "reopen-failed"
test "$(timeout 15 secret-tool lookup service winlux-test key k)" = s3cret || echo "lookup-failed"
if test -n "$PASSWORD"; then
    restart
    if printf faux | $K >/dev/null 2>&1; then echo "wrong-accepted"; fi
fi
restart
'''
    for password, label in (("mot de passe", "compte avec mot de passe"), ("", "compte sans mot de passe")):
        with tempfile.TemporaryDirectory() as home, tempfile.TemporaryDirectory() as runtime:
            os.chmod(runtime, 0o700)
            env = {k: v for k, v in os.environ.items() if k not in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS")}
            env.update(HOME=home, XDG_RUNTIME_DIR=runtime, PASSWORD=password, PYTHON=sys.executable,
                       HELPER=str(HELPER))
            out = subprocess.run(["dbus-run-session", "--", "sh", "-c", script], env=env,
                                 capture_output=True, text=True, timeout=120)
            problems = [line for line in out.stdout.split() if line.endswith(("-failed", "-accepted", "-keyring"))]
            ok(not problems, f"trousseau « login » créé, rouvert et utilisé sans fenêtre : {label} {problems or ''}")
            if problems:
                print(out.stderr[-2000:])

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Trousseau à la connexion : OK")
