#!/usr/bin/python3
"""Contrôle parental : un service relancé reprend l'état du précédent (#2246).

Le service gèle (SIGSTOP) un compte dont l'écran de blocage a été fermé de
force deux fois. S'il est relancé (arrêt, plantage), le nouveau service
doit dégeler ce compte et fermer l'écran laissé par l'ancien ; les règles
habituelles rouvrent ensuite l'écran si le temps n'est pas permis.
Sans root : racine factice (WINLUX_PARENTAL_ROOT), signaux relevés.
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-parental.py"
work = Path(tempfile.mkdtemp(prefix="winlux-parental-reprise."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"
failures = 0


def check(cond, label):
    global failures
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures += 1


loader = SourceFileLoader("winlux_parental", str(TOOL))
spec = importlib.util.spec_from_loader("winlux_parental", loader)
lib = importlib.util.module_from_spec(spec)
loader.exec_module(lib)
signals = []
lib.signal_all = lambda uid, sig: signals.append((uid, sig))

# 1. Le premier service gèle le compte 1002 : l'état est écrit avant le gel.
first = lib.Daemon()
first.log = lambda text: None
first.freeze(1002)
saved = json.loads(lib.FROZEN_FILE.read_text(encoding="utf-8"))
check(saved == [1002] and signals == [(1002, signal.SIGSTOP)], "compte gelé noté dans l'état du service")
check(oct(lib.FROZEN_FILE.stat().st_mode & 0o777) == "0o600" and oct(lib.STATE_DIR.stat().st_mode & 0o777) == "0o700",
      "état du service privé (dossier 0700, fichier 0600)")

# Écran de blocage laissé par ce premier service (orphelin : son parent a disparu).
leftover = subprocess.run(["sh", "-c", 'setsid "$@" >/dev/null 2>&1 & echo $!', "sh", sys.executable, "-c",
                           "import time; time.sleep(120)", str(TOOL), "screen", "bob", ":7"],
                          capture_output=True, text=True).stdout.strip()
leftover = int(leftover)
for _ in range(50):
    if "screen" in Path(f"/proc/{leftover}/cmdline").read_bytes().decode(errors="replace"):
        break
    time.sleep(0.1)
check(leftover in lib.stale_screens(), "écran laissé par le service précédent reconnu")

# 2. Service relancé : dégel du compte, écran orphelin fermé, état remis à zéro.
notes = []
second = lib.Daemon()
second.log = notes.append
second.recover()
check(leftover in second.orphans and Path(f"/proc/{leftover}").exists(),
      "reprise : l'écran laissé reste en place le temps que le nouveau service rouvre le sien")
second.close_orphans()
check(Path(f"/proc/{leftover}").exists(), "écran laissé encore en place avant la fin du délai")
second.orphans_at = 0
second.close_orphans()
for _ in range(50):
    if not Path(f"/proc/{leftover}").exists() or \
            Path(f"/proc/{leftover}/stat").read_text().rsplit(")", 1)[1].split()[0] == "Z":
        break
    time.sleep(0.1)
gone = not Path(f"/proc/{leftover}").exists() or \
    Path(f"/proc/{leftover}/stat").read_text().rsplit(")", 1)[1].split()[0] == "Z"
check(signals[-1] == (1002, signal.SIGCONT) and not second.frozen,
      "service relancé : le compte gelé par le précédent est dégelé (#2246)")
check(gone, "service relancé : l'écran de blocage orphelin est fermé")
check(json.loads(lib.FROZEN_FILE.read_text(encoding="utf-8")) == [], "état remis à zéro après la reprise")
check(any("dégelé" in note for note in notes), "reprise notée au journal du service")

# 3. Un état illisible ou un compte système n'est jamais touché.
lib.FROZEN_FILE.write_text("[0, 1, \"x\"]", encoding="utf-8")
before = len(signals)
lib.Daemon().recover()
check(len(signals) == before, "compte root ou système jamais dégelé depuis l'état")

# 4. Un écran lancé par ce service lui-même n'est pas pris pour un orphelin.
own = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)", str(TOOL), "screen", "bob", ":8"])
time.sleep(0.3)
check(own.pid not in lib.stale_screens(), "écran du service en cours jamais fermé")
own.kill()
own.wait()
try:
    os.kill(leftover, signal.SIGKILL)
except OSError:
    pass
shutil.rmtree(work, ignore_errors=True)
print("test-parental-reprise :", "réussi" if failures == 0 else f"{failures} échec(s)")
sys.exit(1 if failures else 0)
