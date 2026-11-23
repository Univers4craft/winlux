#!/usr/bin/python3
"""Session au repos : le bureau est surveillé sans « sleep 1 » par seconde (#3297).

La boucle de winlux-user-session.sh qui garde xfdesktop attend désormais par
« wait » avec un piège sur SIGCHLD. Bureau immobile : un seul sleep toutes
les 10 s (réveil de secours). Fin du gestionnaire de fenêtres ou du bureau :
la boucle rend la main aussitôt.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


session = (ROOT / "scripts/winlux-user-session.sh").read_text(encoding="utf-8")
start = session.index("        trap : CHLD\n")
end = session.index('        kill "$ticker" 2>/dev/null || true\n', start) + len('        kill "$ticker" 2>/dev/null || true\n')
loop = session[start:end]
ok("sleep 1; done" not in session.split("xfdesktop --disable-wm-check", 1)[1].split("sleep 1\ndone", 1)[0],
   "plus de « sleep 1 » à chaque seconde pour garder le bureau (#3297)")

with tempfile.TemporaryDirectory() as directory:
    tmp = Path(directory)
    log = tmp / "lancés"
    stub = tmp / "bin"
    stub.mkdir()
    (stub / "sleep").write_text(f'#!/bin/sh\necho "sleep $*" >>"{log}"\nexec /bin/sleep "$@"\n')
    (stub / "sleep").chmod(0o755)
    env = dict(os.environ, PATH=f"{stub}:/usr/bin:/bin")

    def run(case):
        script = ("/bin/sleep 30 & wm_pid=$!\n/bin/sleep 30 & desktop_pid=$!\n"
                  f"( {case} ) &\n" + loop + 'echo "fin $(date +%s.%N)"\nkill $wm_pid $desktop_pid 2>/dev/null\n')
        t0 = time.time()
        result = subprocess.run(["sh", "-c", script], env=env, capture_output=True, text=True, timeout=120)
        return result, time.time() - t0

    # Bureau immobile 3 s, puis fin du gestionnaire de fenêtres.
    log.write_text("")
    result, elapsed = run('/bin/sleep 3; kill $wm_pid')
    launched = log.read_text().split("\n")[:-1]
    print(f"bureau immobile 3 s : {launched}, rendu en {elapsed:.2f} s ; {result.stderr.strip()}")
    ok(launched == ["sleep 10"], "bureau immobile : un seul sleep (réveil de 10 s), pas un par seconde (#3297)")
    ok("fin" in result.stdout and elapsed < 3.8, "fin du gestionnaire de fenêtres : boucle quittée aussitôt")

    # Fin du bureau (xfdesktop planté) : rendu aussitôt, pour le relancer.
    log.write_text("")
    result, elapsed = run('/bin/sleep 1; kill $desktop_pid')
    print(f"bureau arrêté après 1 s : rendu en {elapsed:.2f} s")
    ok("fin" in result.stdout and elapsed < 1.8, "bureau arrêté : boucle quittée aussitôt pour le relancer")

    # Gestionnaire parti juste entre le test et le wait (fenêtre de course
    # forcée ici) : rattrapé au plus tard par le réveil de 10 s.
    result, elapsed = run('kill $wm_pid')
    print(f"gestionnaire parti au moment du test : rendu en {elapsed:.2f} s")
    ok("fin" in result.stdout and elapsed < 11, "course entre test et wait : rattrapée en 10 s au plus")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("attente de session : OK")
