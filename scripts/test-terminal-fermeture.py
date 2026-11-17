#!/usr/bin/python3 -I
"""#2745 : fermer le Terminal Winlux termine son shell, même occupé.

xterm répond à la fermeture de sa fenêtre (WM_DELETE_WINDOW) par un seul SIGHUP
au groupe de processus de son enfant. On le reproduit ici sans X : le relais
winlux-terminal-shell tourne dans un pseudo-terminal, comme sous xterm, et
reçoit ce SIGHUP. Le relais doit se terminer, et son shell avec lui :
  - quand le shell perd le premier SIGHUP (renvoi) ;
  - quand le shell n'en tient aucun compte (arrêt forcé après le délai) ;
  - avec un vrai « bash -i » qui reçoit une saisie continue, cas où bash perd
    le SIGHUP (40 sur 40 sans relais sur le banc qui a servi au diagnostic).
Il rend aussi le code de sortie du shell, et laisse SIGPIPE par défaut.
"""
import os
import pty
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

RELAY = str(Path(__file__).resolve().parent.parent / "integration" / "winlux-terminal-shell")
failures = []


def check(condition, label):
    print(("OK    " if condition else "ÉCHEC ") + label, flush=True)
    if not condition:
        failures.append(label)


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return Path(f"/proc/{pid}/stat").read_text().split(") ")[-1][0] != "Z"


def descendants(pid):
    found = []
    for entry in Path("/proc").iterdir():
        if entry.name.isdigit():
            try:
                fields = (entry / "stat").read_text().split(") ")[-1].split()
            except OSError:
                continue
            if int(fields[1]) == pid:
                found.append(int(entry.name))
    return found


def close_like_xterm(command, flood=0.0, limit=3.0):
    """Lance le relais dans un pty, envoie un seul SIGHUP au groupe comme xterm.
    Rend (fermé à temps, durée, shells restés en vie)."""
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["PS1"] = "$ "
        os.execv(RELAY, [RELAY] + command)
    stop = threading.Event()

    def drain():
        try:
            while os.read(fd, 65536):
                pass
        except OSError:
            pass

    def feed():
        try:
            while not stop.is_set():
                os.write(fd, b"a" * 64)
        except OSError:
            pass

    threading.Thread(target=drain, daemon=True).start()
    time.sleep(0.5)
    children = descendants(pid)
    if flood:
        threading.Thread(target=feed, daemon=True).start()
        time.sleep(flood)
    # La saisie s'arrête quand on ferme la fenêtre ; ce qui est déjà dans le
    # pty reste à traiter par le shell.
    stop.set()
    started = time.monotonic()
    os.kill(-pid, signal.SIGHUP)
    closed = False
    while time.monotonic() - started < limit:
        if os.waitpid(pid, os.WNOHANG)[0] == pid:
            closed = True
            break
        time.sleep(0.02)
    elapsed = time.monotonic() - started
    if not closed:
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
    time.sleep(0.1)
    left = [child for child in children if alive(child)]
    for child in left:
        os.kill(child, signal.SIGKILL)
    os.close(fd)
    return closed, elapsed, left


status = subprocess.run([RELAY, "/bin/sh", "-c", "exit 3"], timeout=10).returncode
check(status == 3, f"le code de sortie du shell est rendu ({status})")
piped = subprocess.run([RELAY, "/bin/sh", "-c", "yes | head -n 1"], capture_output=True, text=True, timeout=10)
check(piped.stdout == "y\n" and piped.stderr == "", f"SIGPIPE par défaut sous le relais ({piped.stderr.strip()!r})")

# Shell qui perd le premier SIGHUP, comme bash occupé par une saisie.
loses_first = [sys.executable, "-I", "-c",
               "import signal, time\n"
               "seen = []\n"
               "def hup(*_):\n"
               "    seen.append(1)\n"
               "    if len(seen) > 1: raise SystemExit(129)\n"
               "signal.signal(signal.SIGHUP, hup)\n"
               "while True: time.sleep(1)\n"]
closed, elapsed, left = close_like_xterm(loses_first, limit=2.0)
check(closed and not left, f"premier SIGHUP perdu : fenêtre fermée en {elapsed:.2f} s, shells restants {left}")

closed, elapsed, left = close_like_xterm(["/bin/sh", "-c", "trap '' HUP; while :; do sleep 1; done"], limit=7.0)
check(closed and not left and elapsed >= 4.5, f"shell sourd au SIGHUP : arrêté au bout de {elapsed:.2f} s, shells restants {left}")

if Path("/bin/bash").exists():
    worst = 0.0
    stuck = []
    for attempt in range(5):
        closed, elapsed, left = close_like_xterm(["/bin/bash", "-i"], flood=0.1 + 0.07 * attempt, limit=6.0)
        print(f"      essai {attempt} : {elapsed:.2f} s", flush=True)
        worst = max(worst, elapsed)
        if not closed or left:
            stuck.append(attempt)
    check(not stuck, f"bash -i en pleine saisie : 5 fermetures, pire {worst:.2f} s, ratées {stuck}")

if failures:
    sys.exit(f"{len(failures)} échec(s) : {failures}")
print("WINLUX_TERMINAL_FERMETURE=0")
