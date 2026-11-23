#!/usr/bin/python3
"""Init au repos : la garde du bureau ne lance plus quatre processus par seconde.

Avant (#3312) : chaque seconde, l'init (pid 1) relisait /run/winlux/session-action et
update-action par deux `$(/bin/busybox cat …)`, testait la session par
`/bin/busybox kill -0`, puis dormait par `/bin/busybox sleep 1` : quatre
processus par seconde, un bureau immobile compris (relevé du 22 nov. : /init
0,49 % d'un cœur, 6 réveils/s).

La boucle extraite de l'init (scripts/build-live-iso.sh) tourne ici sous le
même shell que l'init (busybox sh), avec /bin/busybox remplacé par un témoin
qui note chaque programme lancé, pour les deux boucles (compte du bureau et
bureau root) :
1. Bureau immobile : un seul programme par tour (le sleep, qui fait aussi
   récolter les orphelins par l'init).
2. Fichiers absents, vides ou sans saut de ligne final : aucune erreur, la
   boucle continue, la valeur est lue comme avant.
3. « Se déconnecter » (logoff-session) et « Redémarrer la session »
   (restart-session) arrêtent la session au tour suivant, comme avant ; une
   mise à jour en cours (winlux-update --busy) retarde toujours le redémarrage.
4. La session terminée, la boucle s'arrête au tour suivant.
"""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


init = (ROOT / "scripts/build-live-iso.sh").read_text()


def loop_of(pid_var):
    """Boucle de garde d'un bureau : de son « while » au « done » qui précède son wait."""
    header = next(line for line in init.splitlines()
                  if line.strip().startswith("while ") and f'kill -0 "${pid_var}"' in line
                  and "xorg" not in line)
    start = init.index(header)
    end = init.index(f'wait "${pid_var}"', start)
    end = init.rindex("done", start, end) + len("done")
    return init[start:end]


shell = shutil.which("busybox")
command = [shell, "sh"] if shell else ["sh"]
print(f"shell de l'init : {' '.join(command)}" + ("" if shell else " (busybox absent : sh du système)"))

with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    run = work / "run/winlux"
    run.mkdir(parents=True)
    calls = work / "appels"
    real = shell or "/bin/sh"
    # Témoin de /bin/busybox : note l'applet, puis la vraie commande (sleep raccourci).
    fake = work / "busybox"
    fake.write_text(
        "#!/bin/sh\n"
        f'echo "$1" >> "{calls}"\n'
        'if test "$1" = sleep; then exec sleep 0.1; fi\n'
        f'if test -x "{real}" && test "{real}" != /bin/sh; then exec "{real}" "$@"; fi\n'
        # Sans busybox : l'applet par le sh du système (kill y est interne).
        'applet=$1; shift; exec sh -c "$applet \\"\\$@\\"" sh "$@"\n')
    fake.chmod(0o755)
    busy = work / "occupe"
    update_stub = work / "winlux-update"
    update_stub.write_text(f'#!/bin/sh\ntest "$1" = --busy && test -e "{busy}"\n')
    update_stub.chmod(0o755)

    def prepare(pid_var):
        body = (loop_of(pid_var).replace("/bin/busybox", str(fake))
                .replace("/run/winlux/", f"{run}/").replace("/usr/bin/winlux-update", str(update_stub)))
        return (f"{pid_var}=$1\nlock_at_start=0\nlogged_off=0\n{body}\n"
                'echo "fin logged_off=$logged_off lock_at_start=$lock_at_start"\n')

    def tours():
        lines = calls.read_text().splitlines() if calls.exists() else []
        return lines

    for pid_var, label in (("session_pid", "bureau du compte"), ("desktop_pid", "bureau root")):
        for name in ("session-action", "update-action"):
            (run / name).unlink(missing_ok=True)
        calls.unlink(missing_ok=True)
        busy.unlink(missing_ok=True)
        script = prepare(pid_var)
        # La session : un groupe de processus à elle, comme sous setsid.
        session = subprocess.Popen(["sleep", "60"], start_new_session=True)
        proc = subprocess.Popen(command + ["-c", script, "init", str(session.pid)],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            time.sleep(1.2)  # ≈ 10 tours, fichiers absents
            quiet = tours()
            sleeps = quiet.count("sleep")
            print(f"{label} : {len(quiet)} programmes en {sleeps} tours : {sorted(set(quiet))}")
            ok(sleeps >= 5 and len(quiet) == sleeps,
               f"{label} immobile : un seul programme par tour, le sleep (#3312)")
            # Fichiers vides, sans saut de ligne, multi-lignes : rien ne doit partir.
            (run / "session-action").write_text("")
            (run / "update-action").write_text("reboot")
            time.sleep(0.4)
            ok(proc.poll() is None and session.poll() is None,
               f"{label} : fichier vide ou autre action, la session continue")
            if pid_var == "session_pid":
                (run / "session-action").write_text("logoff-session")  # sans saut de ligne
                proc.wait(timeout=5)
                out = proc.stdout.read()
                ok(session.wait(timeout=5) is not None, f"{label} : « Se déconnecter » arrête la session (#1564)")
                ok("logged_off=1 lock_at_start=1" in out, f"{label} : déconnexion notée, écran de connexion ensuite")
                ok(not (run / "session-action").exists(), f"{label} : demande consommée")
            else:
                busy.touch()
                (run / "session-action").write_text("restart-session\n")
                (run / "update-action").write_text("restart-session\n")
                time.sleep(0.5)
                ok(proc.poll() is None and session.poll() is None,
                   f"{label} : mise à jour en cours, le redémarrage attend")
                busy.unlink()
                proc.wait(timeout=5)
                ok(session.poll() is not None or session.wait(timeout=5) is not None,
                   f"{label} : « Redémarrer la session » arrête le bureau")
                ok(not (run / "session-action").exists() and not (run / "update-action").exists(),
                   f"{label} : demandes consommées")
            err = proc.stderr.read()
            ok(err.strip() == "", f"{label} : aucune erreur du shell ({err.strip()[:80]!r})")
        finally:
            for p in (proc, session):
                if p.poll() is None:
                    p.kill()
                    p.wait()

    # 4. Session terminée : la boucle s'arrête au tour suivant.
    calls.unlink(missing_ok=True)
    for name in ("session-action", "update-action"):
        (run / name).unlink(missing_ok=True)
    session = subprocess.Popen(["sleep", "0.3"])
    proc = subprocess.Popen(command + ["-c", prepare("session_pid"), "init", str(session.pid)],
                            stdout=subprocess.PIPE, text=True)
    session.wait()
    try:
        proc.wait(timeout=3)
        ok(True, "session terminée : la garde s'arrête")
    except subprocess.TimeoutExpired:
        proc.send_signal(signal.SIGKILL)
        proc.wait()
        ok(False, "session terminée : la garde s'arrête")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")

print("init au repos : OK")
