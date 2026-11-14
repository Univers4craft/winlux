#!/usr/bin/python3
"""« winlux-power now reboot » survit à la fermeture de son terminal (#2275).

Banc 1, 12 nov. : lancé depuis un xterm, `winlux-power now reboot` fermait
toutes les fenêtres, dont ce terminal ; le terminal envoyait SIGHUP à ses
tâches et l'écran comme son gardien mouraient avant de signaler l'init.

Ici, sans affichage : la commande tourne dans un vrai pseudo-terminal dont
elle est la tâche au premier plan ; dès que l'écran annonce qu'il est prêt, on
ferme le côté maître (le noyau envoie SIGHUP, comme à la fermeture d'un
xterm). L'init factice doit quand même recevoir SIGTERM (redémarrage).
"""
import fcntl
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import termios
import time

here = Path(__file__).resolve().parent
power = here / "winlux-power.py"


def fake_init(marker):
    # Même contrat que l'init de Winlux : USR2 = arrêt, TERM = redémarrage.
    process = subprocess.Popen(["sh", "-c", f'trap "echo poweroff > {marker}; exit" USR2; '
                                f'trap "echo reboot > {marker}; exit" TERM; while :; do sleep 0.1; done'])
    time.sleep(0.3)
    return process


def controlling_tty():
    # Après le setsid de start_new_session : le pty devient le terminal de contrôle.
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


with tempfile.TemporaryDirectory() as temp:
    temp = Path(temp)
    marker = temp / "init.txt"
    init = fake_init(marker)
    env = dict(os.environ, WINLUX_CONTROL_ROOT=str(temp), WINLUX_POWER_SOCKET=str(temp / "power.sock"),
               WINLUX_LANG="fr", WINLUX_POWER_INIT_PID=str(init.pid), WINLUX_POWER_TEST_HANGUP_WAIT="1")
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    master, slave = os.openpty()
    process = subprocess.Popen([sys.executable, "-B", str(power), "now", "reboot"], env=env,
                               stdin=slave, stdout=slave, stderr=slave, start_new_session=True,
                               preexec_fn=controlling_tty)
    os.close(slave)
    seen = b""
    deadline = time.monotonic() + 20
    while b"pr\xc3\xaat" not in seen and time.monotonic() < deadline:
        ready, _, _ = select.select([master], [], [], 0.5)
        if ready:
            try:
                seen += os.read(master, 4096)
            except OSError:
                break
    assert b"pr\xc3\xaat" in seen, seen
    os.close(master)  # le terminal se ferme : SIGHUP au groupe au premier plan
    try:
        process.wait(30)
    except subprocess.TimeoutExpired:
        process.kill()
        raise
    try:
        init.wait(10)
    except subprocess.TimeoutExpired:
        init.kill()
        raise AssertionError(f"init jamais signalé après la fermeture du terminal (code {process.returncode})")
    assert marker.read_text().strip() == "reboot", marker.read_text()

print("test-power-terminal-hangup : OK (terminal fermé, redémarrage quand même)")
