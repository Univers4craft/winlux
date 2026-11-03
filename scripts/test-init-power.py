#!/usr/bin/env python3
"""Arrêt et redémarrage par l'init : jamais bloqués.

Après une installation depuis la session live, « Redémarrer maintenant »
laissait la machine sur le logo : l'init avait arrêté les programmes puis
restait bloqué avant l'appel au noyau. Le test exécute la fonction
safe_power de l'init (scripts/build-live-iso.sh) avec un faux BusyBox :
- cas normal : sync, remontage en lecture seule, puis « reboot -f » ;
- sync bloqué : le garde-fou écrit s, u puis b (ou o pour l'arrêt) dans
  /proc/sysrq-trigger ;
- mise à jour en cours : rien n'est arrêté.
"""
import os
import pathlib
import re
import stat
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
source = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
init = re.search(r'^cat >"\$stage/init" <<.INIT.\n(.*?)^INIT$', source, re.S | re.M).group(1)
function = re.search(r"^safe_power\(\) \{\n.*?^\}\n", init, re.S | re.M)
assert function, "safe_power introuvable dans l'init"
function = function.group(0)


def run(action, sync_hangs=False, busy=False):
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        log = tmp / "log"
        sysrq = tmp / "sysrq"
        fake = tmp / "busybox"
        fake.write_text(f"""#!/bin/sh
echo "$*" >> {log}
case "$1" in
    sleep) case "$2" in 25) sleep 0.6 ;; 5) sleep 0.1 ;; *) sleep 0.05 ;; esac ;;
    sync) {'sleep 30' if sync_hangs else ':'} ;;
esac
exit 0
""")
        update = tmp / "winlux-update"
        update.write_text(f"#!/bin/sh\nexit {0 if busy else 1}\n")
        for path in (fake, update):
            path.chmod(path.stat().st_mode | stat.S_IEXEC)
        body = (function.replace("/bin/busybox", str(fake))
                .replace("/usr/bin/winlux-update", str(update))
                .replace(">/proc/sysrq-trigger", ">>" + str(sysrq)))
        script = body + f"\nsafe_power {action}\n"
        process = subprocess.Popen(["sh", "-c", script], stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   text=True, start_new_session=True)
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            done = log.exists() and f"{action} -f" in log.read_text()
            forced = sysrq.exists() and sysrq.read_text().strip().endswith("b" if action == "reboot" else "o")
            if done or forced or process.poll() is not None and not sync_hangs:
                break
            time.sleep(0.05)
        time.sleep(0.3)
        try:
            os.killpg(process.pid, 9)
        except ProcessLookupError:
            pass
        out = process.communicate()[0]
        return (log.read_text() if log.exists() else ""), (sysrq.read_text() if sysrq.exists() else ""), out


calls, keys, out = run("reboot")
assert "kill -TERM -1" in calls and "sync" in calls and "mount -o remount,ro /" in calls, calls
assert "reboot -f" in calls, calls
print("reboot normal : sync, lecture seule, reboot -f")

calls, keys, out = run("reboot", sync_hangs=True)
assert "reboot -f" not in calls, calls
assert keys.split() == ["s", "u", "b"], f"garde-fou absent : {keys!r}\n{out}"
print("sync bloqué : redémarrage forcé par le noyau (s, u, b)")

calls, keys, out = run("poweroff", sync_hangs=True)
assert keys.split() == ["s", "u", "o"], f"garde-fou absent : {keys!r}\n{out}"
print("sync bloqué : arrêt forcé par le noyau (s, u, o)")

calls, keys, out = run("reboot", busy=True)
assert "kill" not in calls and not keys, calls
assert "Installation en cours" in out
print("mise à jour en cours : arrêt différé")
