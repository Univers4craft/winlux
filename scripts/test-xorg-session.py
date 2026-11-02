#!/usr/bin/env python3
"""Xorg :0 de l'init : sa propre session, donc la gestion des VT.

Sur un système installé, « Changer d'utilisateur » restait sur un écran noir :
le Xorg :0 lancé par l'init (pid 1) échouait sur setsid (« setsid failed:
Operation not permitted »), n'avait pas son VT comme terminal de contrôle et
ne rendait pas l'affichage au Xorg du second compte (« drmSetMaster: Device or
resource busy »).

Le test exécute le lancement de Xorg tel qu'il est écrit dans l'init
(scripts/build-live-iso.sh) avec un faux Xorg qui refait les appels de
xf86OpenConsole (setpgid vers le groupe du parent, puis setsid) et vérifie :
- que son parent est chef de sa propre session (ce qui permet à setsid de
  réussir même quand l'init est le grand-parent) ;
- que setsid réussit ;
- que le pid suivi par l'init ($xorg_pid) vit tant que Xorg vit, et qu'un
  TERM sur ce pid arrête Xorg.
"""
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
source = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
init = re.search(r'^cat >"\$stage/init" <<.INIT.\n(.*?)^INIT$', source, re.S | re.M).group(1)
match = re.search(r"^    /bin/busybox touch /run/winlux/splash.stop\n(.*?^    xorg_pid=\$!)$", init, re.S | re.M)
assert match, "lancement de Xorg :0 introuvable dans l'init"
launch = match.group(1)
assert "/usr/bin/Xorg" in launch

with tempfile.TemporaryDirectory() as tmp:
    tmp = pathlib.Path(tmp)
    report = tmp / "report"
    fake = tmp / "Xorg"
    fake.write_text(f"""#!{sys.executable}
import os, signal, sys, time
lines = []
ppid = os.getppid()
lines.append("parent_session_leader=%d" % (os.getsid(ppid) == ppid))
ppgid = os.getpgid(ppid)
# xf86OpenConsole (hw/xfree86/os-support/linux/lnx_init.c), sans -keeptty.
if ppgid != ppid:
    try:
        os.setpgid(0, ppgid)
    except OSError as error:
        lines.append("setpgid=%s" % error.strerror)
try:
    os.setsid()
    lines.append("setsid=ok")
except OSError as error:
    lines.append("setsid=%s" % error.strerror)
lines.append("args=" + " ".join(sys.argv[1:]))
open({str(report)!r}, "w").write("\\n".join(lines) + "\\n")
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
time.sleep(60)
""")
    fake.chmod(0o755)
    script = f"""
set -u
{launch.replace('/usr/bin/Xorg', str(fake)).replace('/tmp/xorg-console.log', str(tmp / 'console.log'))}
for i in $(seq 1 100); do test -s {report} && break; sleep 0.05; done
echo "pid=$xorg_pid"
kill -0 "$xorg_pid" && echo alive=1
kill "$xorg_pid"
for i in $(seq 1 100); do kill -0 "$xorg_pid" 2>/dev/null || break; sleep 0.05; done
kill -0 "$xorg_pid" 2>/dev/null && echo after_term=alive || echo after_term=gone
"""
    out = subprocess.run(["sh", "-c", script], capture_output=True, text=True, timeout=30)
    text = report.read_text() if report.exists() else ""
    print(text + out.stdout + out.stderr, end="")
    assert "parent_session_leader=1" in text, "le parent de Xorg n'est pas chef de session"
    assert "setsid=ok" in text, "setsid échoue : Xorg n'aura pas la gestion du VT"
    assert "args=:0 " in text and "-logfile /tmp/Xorg.0.log" in text, text
    assert "alive=1" in out.stdout, "le pid suivi par l'init ne vit pas avec Xorg"
    assert "after_term=gone" in out.stdout, "TERM sur le pid suivi n'arrête pas Xorg"
    time.sleep(0.2)
    leftover = subprocess.run(["pgrep", "-f", str(fake)], capture_output=True, text=True).stdout.strip()
    assert not leftover, f"Xorg toujours en vie : {leftover}"
# Les Xorg des autres comptes (winlux-sessions) sont lancés de la même façon.
sessions = (ROOT / "scripts/winlux-sessions.sh").read_text(encoding="utf-8")
assert re.search(r"setsid /bin/sh -c .*wait .*winlux-xorg", sessions), "winlux-sessions : Xorg sans session propre"
print("Xorg :0 : session propre, setsid réussi, arrêt suivi")
