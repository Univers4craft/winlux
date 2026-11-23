#!/usr/bin/python3
"""Relevé d'état à la demande (#3126).

Avant : winlux-user-session.sh lançait `winlux-control status` toutes les 3 s,
à vie (≈ 65 processus par passage, `xrandr --query` qui resonde les écrans),
même bureau immobile et Paramètres fermés.

1. La boucle de session fait un relevé à l'ouverture, puis plus aucun tant
   que personne ne le demande.
2. Une demande (fichier touché) donne exactement un relevé de plus.
3. Une demande répétée (Paramètres ouverts) garde l'état frais à chaque tour.
4. Une action de winlux-control sans relevé immédiat (WINLUX_CONTROL_NO_STATUS)
   pose la demande, pour que la boucle publie l'état comme avant.
5. Paramètres (Python) touche la demande sans suivre un lien.
"""
import ast
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


session = (ROOT / "scripts/winlux-user-session.sh").read_text()
# Corps de la boucle : du calcul de « served » à son « done ».
start = session.index('served="$control_demand-servie"')
end = session.index("done", start) + len("done")
loop = session[start:end]

with tempfile.TemporaryDirectory() as directory:
    work = Path(directory)
    bin_dir = work / "bin"
    bin_dir.mkdir()
    calls = work / "appels"
    stub = bin_dir / "winlux-control"
    stub.write_text(f'#!/bin/sh\necho "$WINLUX_CONTROL_PERIODIC $*" >> "{calls}"\n')
    stub.chmod(0o755)
    # Tour de boucle raccourci : 3 s → 0,1 s.
    (bin_dir / "sleep").write_text("#!/bin/sh\nexec /bin/sleep 0.1\n")
    (bin_dir / "sleep").chmod(0o755)
    account = work / "control-compte"
    account.mkdir()
    demand = account / "demande"
    script = (f'uid={os.getuid()}\ncontrol_demand="{demand}"\nstatus_nice=\n'
              + loop.replace("/usr/bin/winlux-control", str(stub)) + "\n")
    env = dict(os.environ, PATH=f"{bin_dir}:/usr/bin:/bin")
    proc = subprocess.Popen(["sh", "-c", script], env=env)
    try:
        def count():
            return len(calls.read_text().splitlines()) if calls.exists() else 0
        time.sleep(1.5)  # ≈ 15 tours sans demande
        first = count()
        print(f"relevés après 15 tours sans demande : {first}")
        ok(first == 1, "un seul relevé à l'ouverture, aucun sans demande (#3126)")
        lines = calls.read_text().splitlines() if calls.exists() else []
        ok(bool(lines) and lines[0].startswith("1 status"), "relevé périodique léger (WINLUX_CONTROL_PERIODIC=1)")
        time.sleep(1.1)  # date de fichier à la seconde près (busybox)
        demand.write_text("")
        time.sleep(1.0)
        second = count()
        print(f"relevés après une demande : {second}")
        ok(second == 2, "une demande donne exactement un relevé (#3126)")
        for _ in range(3):
            time.sleep(1.05)
            os.utime(demand)
        time.sleep(1.0)
        third = count()
        print(f"relevés après trois demandes espacées : {third}")
        ok(third == 5, "Paramètres ouverts : un relevé par demande, l'état reste frais (#3126)")
    finally:
        proc.kill()
        proc.wait()

    # 4. Action sans relevé immédiat : la demande est posée.
    root = work / "racine"
    (root / "etc/winlux").mkdir(parents=True)
    (root / "run/winlux").mkdir(parents=True)
    env = dict(os.environ, WINLUX_CONTROL_ROOT=str(root), WINLUX_CONTROL_NO_STATUS="1")
    result = subprocess.run(["sh", str(ROOT / "scripts/winlux-control.sh"), "theme-dark"],
                            env=env, capture_output=True, text=True)
    print(result.stdout + result.stderr)
    ok((root / "etc/winlux/preferences.conf").read_text().strip() == "THEME=dark", "l'action elle-même est faite")
    ok((root / "run/winlux-control-demande").exists(),
       "action sans relevé immédiat : relevé demandé à la boucle (#3126)")
    ok(not (root / "run/winlux/control.txt").exists(), "action sans relevé immédiat : pas de relevé synchrone")

    # 5. Paramètres : demande touchée, jamais à travers un lien.
    source = (ROOT / "scripts/winlux-settings.py").read_text()
    tree = ast.parse(source)
    function = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "demand_status")
    namespace = {"os": os}
    exec(compile(ast.Module(body=[function], type_ignores=[]), "winlux-settings.py", "exec"), namespace)
    target = work / "py-demande"
    os.environ["WINLUX_CONTROL_DEMAND"] = str(target)
    namespace["demand_status"]()
    ok(target.exists(), "Paramètres pose la demande de relevé (#3126)")
    victim = work / "victime"
    victim.write_text("intact")
    old = victim.stat().st_mtime_ns
    target.unlink()
    target.symlink_to(victim)
    time.sleep(0.01)
    namespace["demand_status"]()
    ok(victim.stat().st_mtime_ns == old and victim.read_text() == "intact",
       "Paramètres ne suit pas un lien posé à la place de la demande")
    os.environ["WINLUX_CONTROL_DEMAND"] = str(work / "absent/demande")
    namespace["demand_status"]()
    ok(True, "dossier absent : aucune erreur")

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("relevé à la demande : OK")
