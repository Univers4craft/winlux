#!/usr/bin/python3
"""Contrôle parental : noms des applications juste après le démarrage (#2387).

time.monotonic() compte depuis le démarrage du noyau. Le cache des noms du
service partait de 0 et ne se chargeait qu'après 600 s : pendant les 10
premières minutes, l'activité nommait Mousepad « mousepad » pour la journée.
Le test simule une horloge à 117 s, comme une machine qui vient de démarrer.
Sans root : racine factice (WINLUX_PARENTAL_ROOT), session et usage simulés.
"""
import importlib.util
from importlib.machinery import SourceFileLoader
import os
from pathlib import Path
import shutil
import sys
import tempfile
from unittest import mock

HERE = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="winlux-parental-noms."))
os.environ["WINLUX_PARENTAL_ROOT"] = str(work)
os.environ["WINLUX_LANG"] = "fr"
failures = 0


def check(cond, label):
    global failures
    print(("OK   " if cond else "ÉCHEC ") + label, flush=True)
    if not cond:
        failures += 1


apps = work / "usr/share/applications"
apps.mkdir(parents=True)
(apps / "org.xfce.mousepad.desktop").write_text("[Desktop Entry]\nType=Application\nName=Mousepad\nExec=mousepad %U\n",
                                                encoding="utf-8")

loader = SourceFileLoader("winlux_parental", str(HERE / "winlux-parental.py"))
spec = importlib.util.spec_from_loader("winlux_parental", loader)
parental = importlib.util.module_from_spec(spec)
loader.exec_module(parental)

with mock.patch.object(parental.time, "monotonic", return_value=117.0):
    daemon = parental.Daemon()
    names = daemon.app_names()
check(names.get("bin:mousepad") == "Mousepad",
      f"service lancé 117 s après le démarrage : noms des lanceurs chargés aussitôt (#2387) ({names})")

# La minute comptée juste après le démarrage nomme l'application comme plus tard.
usage = {"minutes": 0, "apps": {}, "names": {}}
with mock.patch.object(parental.time, "monotonic", return_value=118.0), \
        mock.patch.object(daemon, "sessions", return_value=[("enfant", 1003, ":1", True)]), \
        mock.patch.object(parental, "lock_running", return_value=False), \
        mock.patch.object(parental, "foreground_pid", return_value=4242), \
        mock.patch.object(parental, "proc_uid", return_value=1003), \
        mock.patch.object(parental, "proc_info", return_value=("/usr/bin/mousepad", ["mousepad"])), \
        mock.patch.object(parental, "update_usage", side_effect=lambda user, change, moment=None: change(usage)):
    daemon.count_minute()
check(usage["names"].get("bin:mousepad") == "Mousepad",
      f"activité juste après le démarrage : « Mousepad », pas « mousepad » ({usage['names']})")

# Le cache se recharge toujours après 10 minutes.
(apps / "ristretto.desktop").write_text("[Desktop Entry]\nType=Application\nName=Ristretto\nExec=ristretto\n",
                                        encoding="utf-8")
with mock.patch.object(parental.time, "monotonic", return_value=118.0 + 601):
    later = daemon.app_names()
check(later.get("bin:ristretto") == "Ristretto", "cache rechargé après 10 minutes")

shutil.rmtree(work, ignore_errors=True)
print("test-parental-noms-demarrage :", "réussi" if failures == 0 else f"{failures} échec(s)")
sys.exit(1 if failures else 0)
