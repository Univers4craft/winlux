#!/usr/bin/python3
"""Un programme Windows lancé puis terminé ne laisse aucun fichier vide
/tmp/winlux-journal-<pid>.log (#3322).

Le journal par processus de runtime/src/quality.rs n'est plus créé qu'au
premier vidage qui a une ligne à écrire. On lance hello.exe plusieurs fois et
on compare les fichiers winlux-journal-* de /tmp avant et après.
"""
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
WINLUX = Path(os.environ.get("WINLUX_BIN", ROOT / "target/release/winlux"))
HELLO = Path(os.environ.get("WINLUX_TEST_EXE", ROOT / "target/x86_64-pc-windows-msvc/release/hello.exe"))
TMP = Path("/tmp")


def journaux():
    return {p.name: p.stat().st_size for p in TMP.glob("winlux-journal-*.log")}


if not (WINLUX.is_file() and HELLO.is_file()):
    print(f"test-journal-par-processus : ignoré, {WINLUX} ou {HELLO} absent")
    sys.exit(0)

env = {k: v for k, v in os.environ.items() if k != "WINLUX_JOURNAL"}
avant = journaux()
for _ in range(3):
    proc = subprocess.run([str(WINLUX), str(HELLO)], env=env, capture_output=True, timeout=120)
    if proc.returncode != 0:
        print(f"ÉCHEC {HELLO.name} a rendu {proc.returncode} : {proc.stderr[-400:]!r}")
        sys.exit(1)
nouveaux = {nom: taille for nom, taille in journaux().items() if nom not in avant}
vides = sorted(nom for nom, taille in nouveaux.items() if taille == 0)
if vides:
    print(f"ÉCHEC trois lancements de {HELLO.name} ont laissé {len(vides)} journal(aux) vide(s) : {vides}")
    sys.exit(1)
print(f"OK   aucun journal par processus vide après trois lancements (#3322) ; nouveaux : {sorted(nouveaux)}")
