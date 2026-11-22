#!/usr/bin/env python3
"""Droits des fichiers enregistrés par un programme Windows lancé par
winlux-open (#2862).

Un faux runtime tient lieu de `winlux` : `--inspect` répond « gui », et le
« programme » crée un fichier comme le ferait Fichier > Enregistrer sous
(open(O_CREAT) en 0666, filtré par l'umask hérité). Sous l'umask 022 du
compte, le fichier doit sortir en 0644, comme celui d'un outil natif ; le
dossier d'état du lanceur (journal de lancement) reste en 0700.
"""
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
OPEN = ROOT / "integration" / "winlux-open"

FAKE = """#!/usr/bin/env python3
import os, sys
if sys.argv[1:2] == ["--inspect"]:
    print("gui")
    sys.exit(0)
fd = os.open(os.environ["ESSAI_SORTIE"], os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o666)
os.write(fd, b"essai\\n")
os.close(fd)
"""

failures = 0


def check(cond, label):
    global failures
    print(("ok   " if cond else "ÉCHEC ") + label)
    if not cond:
        failures += 1


with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    bin_dir = tmp / "bin"
    bin_dir.mkdir()
    fake = bin_dir / "winlux"
    fake.write_text(FAKE, encoding="utf-8")
    fake.chmod(0o755)
    exe = tmp / "Programme" / "appli.exe"
    exe.parent.mkdir()
    exe.write_bytes(b"MZ")
    docs = tmp / "Documents"
    docs.mkdir()
    out = docs / "essai.txt"
    state_home = tmp / "etat"
    env = {
        "PATH": f"{bin_dir}:/usr/bin:/bin",
        "HOME": str(tmp),
        "XDG_STATE_HOME": str(state_home),
        "TMPDIR": str(tmp),
        "WINLUX_RUNTIME": str(fake),
        "ESSAI_SORTIE": str(out),
    }
    old = os.umask(0o022)
    try:
        r = subprocess.run(["sh", str(OPEN), str(exe)], env=env, stdin=subprocess.DEVNULL,
                           capture_output=True, text=True, timeout=60)
    finally:
        os.umask(old)
    check(out.exists(), f"le programme a enregistré son fichier (code {r.returncode})")
    if out.exists():
        mode = stat.S_IMODE(out.stat().st_mode)
        check(mode == 0o644, f"fichier enregistré en {mode:04o} sous l'umask 022 du compte, attendu 0644")
    state = state_home / "winlux"
    if state.exists():
        mode = stat.S_IMODE(state.stat().st_mode)
        check(mode == 0o700, f"dossier d'état du lanceur en {mode:04o}, attendu 0700")
    else:
        check(False, "dossier d'état du lanceur créé")
    if failures:
        print(r.stdout[-2000:], r.stderr[-2000:])

sys.exit(1 if failures else 0)
