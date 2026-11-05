#!/usr/bin/env python3
"""Journaux du démarrage sans erreurs parasites (testeur VM, A5 à A7).

- A5 : le bus D-Bus système part avant Xorg (« (EE) dbus-core: error
  connecting to system bus » à chaque démarrage).
- A6 : PipeWire sans RTKit (« RTKit error: ServiceUnknown ») : la session du
  compte reçoit RLIMIT_RTPRIO et RLIMIT_NICE.
- A7 : xscreensaver-systemd s'arrête sans logind (« inhibit sleep failed »),
  donc personne ne répondait à org.freedesktop.ScreenSaver.Inhibit : Winlux
  y répond (winlux-screensaver-inhibit, activé par D-Bus).
"""
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
build = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
init = build[build.index('cat >"$stage/init" <<'):]

# ── A5 ──────────────────────────────────────────────────────────────────
bus = init.index("/usr/bin/dbus-daemon --system --fork")
xorg = init.index("setsid /bin/sh -c 'trap")
assert bus < xorg, "bus système après Xorg"
guard = init[init.rindex("if ", 0, bus):bus]
assert "system_bus_socket" in guard and "'^messagebus:' /etc/passwd" in guard, guard
services = (ROOT / "scripts/winlux-system-services.sh").read_text(encoding="utf-8")
assert "if ! test -S /run/dbus/system_bus_socket; then" in services, "winlux-system-services relancerait un second bus"
print("A5 : bus D-Bus système lancé avant Xorg (compte messagebus présent), pas de second bus ensuite")

# ── A6 ──────────────────────────────────────────────────────────────────
assert "session_limits='/usr/bin/prlimit --rtprio=20 --nice=31'" in init
launch = init[init.index("WINLUX_SESSION_LOGS="):init.index("/usr/bin/winlux-user-session >>")]
assert launch.index("$session_limits") < launch.index("/usr/bin/setpriv"), launch
if shutil.which("prlimit"):
    out = subprocess.run(["prlimit", "--rtprio=20", "--nice=31", "sh", "-c", "ulimit -r; ulimit -e"],
                         capture_output=True, text=True)
    if out.returncode == 0 and out.stdout.split() and out.stdout.split()[0].isdigit():
        assert out.stdout.split()[:2] == ["20", "31"], out.stdout
print("A6 : session du compte avec priorité temps réel 20 et nice -11 (PipeWire sans RTKit)")

# ── A7 ──────────────────────────────────────────────────────────────────
service = (ROOT / "integration/native/dbus/org.freedesktop.ScreenSaver.service").read_text()
assert "Name=org.freedesktop.ScreenSaver" in service and "Exec=/usr/bin/winlux-screensaver-inhibit" in service
stage = (ROOT / "scripts/stage-desktop-components.sh").read_text(encoding="utf-8")
assert "winlux-screensaver-inhibit; do" in stage and "org.freedesktop.ScreenSaver.service" in stage
apt = runpy.run_path(str(ROOT / "scripts/stage-apt-root.py"), run_name="journaux_test")
assert apt["winlux_path"]("usr/share/dbus-1/services/org.freedesktop.ScreenSaver.service")
assert apt["winlux_path"]("usr/bin/winlux-screensaver-inhibit")

tool = runpy.run_path(str(ROOT / "scripts/winlux-screensaver-inhibit.py"), run_name="journaux_test")
with tempfile.TemporaryDirectory() as tmp:
    pokes = []
    marker = Path(tmp) / "power-inhibit" / "1-screensaver"
    box = tool["Inhibitors"](poke=lambda: pokes.append(1), inhibit_file=marker)
    first = box.add(":1.5", "firefox", "video-playing")
    second = box.add(":1.5", "firefox", "audio")
    third = box.add(":1.9", "mpv", "video")
    assert len({first, second, third}) == 3 and pokes == [1], pokes
    assert marker.read_text().strip() == "0x80000003"
    box.forget(":1.5")
    assert list(box.items) == [third] and marker.exists()
    box.remove(third)
    assert not box.items and not marker.exists()
print("A7 : org.freedesktop.ScreenSaver.Inhibit tenu par Winlux (demandes, programme parti, éveil)")

# Service réel sur un bus de session, si D-Bus et PyGObject sont là.
try:
    import gi  # noqa: F401
    from gi.repository import Gio  # noqa: F401
    usable = shutil.which("dbus-daemon") and shutil.which("gdbus")
except Exception:
    usable = False
if usable:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        (tmp / "bin").mkdir()
        log = tmp / "calls"
        for name in ("xscreensaver-command", "xset"):
            fake = tmp / "bin" / name
            fake.write_text(f'#!/bin/sh\necho "{name} $*" >>"{log}"\n'); fake.chmod(0o755)
        env = dict(os.environ, PATH=f"{tmp / 'bin'}:{os.environ['PATH']}", WINLUX_POWER_INHIBIT_DIR=str(tmp / "inhibit"))
        script = f'''
{sys.executable} -B {ROOT / "scripts/winlux-screensaver-inhibit.py"} &
for i in $(seq 1 50); do gdbus call --session -d org.freedesktop.DBus -o /org/freedesktop/DBus -m org.freedesktop.DBus.NameHasOwner org.freedesktop.ScreenSaver 2>/dev/null | grep -q true && break; sleep 0.1; done
gdbus call --session -d org.freedesktop.ScreenSaver -o /org/freedesktop/ScreenSaver -m org.freedesktop.ScreenSaver.Inhibit test video
ls {tmp / "inhibit"}; echo "env=$WINLUX_POWER_INHIBIT_DIR"
'''
        out = subprocess.run(["dbus-run-session", "--", "sh", "-c", script], env=env, capture_output=True, text=True, timeout=60)
        assert "(uint32 1,)" in out.stdout, out.stdout + out.stderr
        assert "-screensaver" in out.stdout, out.stdout + out.stderr
        assert "xscreensaver-command -deactivate" in log.read_text()
    print("A7 : service réel sur un bus de session : Inhibit → cookie, économiseur réveillé, demande d'éveil posée")
else:
    print("(pas de D-Bus ou de PyGObject ici : essai du service réel sauté)")
