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
    holds = []

    class FakeHold:
        def start(self): holds.append("start")
        def stop(self): holds.append("stop")
        def reassert(self): pass
    box = tool["Inhibitors"](poke=lambda: pokes.append(1), inhibit_file=marker, hold=FakeHold())
    first = box.add(":1.5", "firefox", "video-playing")
    second = box.add(":1.5", "firefox", "audio")
    third = box.add(":1.9", "mpv", "video")
    assert len({first, second, third}) == 3 and pokes == [1], pokes
    assert marker.read_text().strip() == "0x80000003"
    box.forget(":1.5")
    assert list(box.items) == [third] and marker.exists()
    box.remove(third)
    assert not box.items and not marker.exists()
    assert holds[0] == "start" and holds[-1] == "stop", holds

# #634 : le DPMS du serveur X éteignait l'écran à 600 s malgré l'économiseur
# bloqué ; suspendu pendant une demande, rétabli seulement s'il était actif.
with tempfile.TemporaryDirectory() as tmp:
    calls = []
    def fake_xset(*args, enabled=True):
        calls.append(args)
        return "DPMS (Display Power Management Signaling):\n  DPMS is Enabled\n" if args == ("q",) and enabled else ""
    hold = tool["DisplayHold"](xset=fake_xset, marker=Path(tmp) / "dpms")
    hold.start(); hold.reassert(); hold.start()
    assert calls == [("q",), ("-dpms",), ("-dpms",), ("-dpms",)], calls
    hold.stop()
    assert ("+dpms",) in calls and calls[-1] == ("q",) and not (Path(tmp) / "dpms").exists(), calls
    calls.clear()
    off = tool["DisplayHold"](xset=lambda *args: fake_xset(*args, enabled=False), marker=Path(tmp) / "dpms")
    off.start(); off.stop()
    assert calls == [("q",)], f"DPMS désactivé par l'utilisateur : laissé tel quel ({calls})"
    # Service tué pendant une demande : le suivant rétablit le DPMS.
    calls.clear()
    killed = tool["DisplayHold"](xset=fake_xset, marker=Path(tmp) / "dpms")
    killed.start()
    (Path(tmp) / "dpms").write_text("999999999\n")   # service tué : son numéro n'existe plus
    tool["DisplayHold"](xset=fake_xset, marker=Path(tmp) / "dpms").recover()
    assert ("+dpms",) in calls and not (Path(tmp) / "dpms").exists(), calls
# #634 : mise à jour en cours (automatique, Paramètres fermé) : demande tenue puis rendue.
events, state = [], {"busy": False}
watch = tool["UpdateWatch"](lambda: state["busy"], lambda: events.append("inhibit") or 4, lambda cookie: events.append(("un", cookie)))
watch.tick(); state["busy"] = True; watch.tick(); watch.tick(); state["busy"] = False; watch.tick(); watch.tick()
assert events == ["inhibit", ("un", 4)], events
def failing():
    raise RuntimeError("bus absent")
retry = tool["UpdateWatch"](lambda: True, failing, lambda cookie: None)
assert retry.tick() and retry.cookie is None, "échec réessayé au tour suivant"
for script in ("winlux-native-session.sh", "winlux-user-session.sh"):
    assert "/usr/bin/winlux-screensaver-inhibit watch-updates" in (ROOT / "scripts" / script).read_text(encoding="utf-8"), script
print("A7 : org.freedesktop.ScreenSaver.Inhibit tenu par Winlux (demandes, programme parti, éveil, DPMS suspendu, mise à jour en cours)")

# #634 : xscreensaver-systemd (qui plante sans logind) remplacé dans l'image et
# dans le paquet APT (dpkg-divert), par un assistant qui attend XScreenSaver.
assert '"$project/scripts/winlux-xscreensaver-systemd.sh" "$stage/usr/libexec/xscreensaver/xscreensaver-systemd"' in stage
assert "usr/libexec/xscreensaver/xscreensaver-systemd" in apt["DIVERTED"] and apt["winlux_path"]("usr/libexec/xscreensaver/xscreensaver-systemd")
stub = ROOT / "scripts/winlux-xscreensaver-systemd.sh"
assert os.access(stub, os.X_OK)
started = time.time()
# Le parent (XScreenSaver) part au bout d'une seconde : l'assistant s'arrête sans erreur.
helper = subprocess.run(["sh", "-c", f'sh "{stub}" >/dev/null 2>&1 & child=$!; sleep 1; echo $child'], capture_output=True, text=True, timeout=10, env=dict(os.environ, WINLUX_XSS_SYSTEMD_INTERVAL="1"))
child = int(helper.stdout.strip())
for _ in range(80):
    try: os.kill(child, 0)
    except OSError: break
    time.sleep(0.5)
else:
    raise AssertionError("l'assistant xscreensaver-systemd survit à XScreenSaver")
assert "systemd" not in stub.read_text().split("exit 0")[0].split("\nparent=")[1], "l'assistant ne doit rien appeler"
print("#634 : xscreensaver-systemd remplacé par un assistant silencieux (image et paquet APT)")

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
            fake.write_text(f'#!/bin/sh\necho "{name} $*" >>"{log}"\ntest "$*" = q && echo "  DPMS is Enabled"\nexit 0\n'); fake.chmod(0o755)
        (tmp / "bin/winlux-update").write_text(f'#!/bin/sh\ntest "$1" = --busy && test -e "{tmp / "busy"}"\n')
        (tmp / "bin/winlux-update").chmod(0o755)
        env = dict(os.environ, PATH=f"{tmp / 'bin'}:{os.environ['PATH']}", WINLUX_POWER_INHIBIT_DIR=str(tmp / "inhibit"),
                   XDG_RUNTIME_DIR=str(tmp), WINLUX_UPDATE_WATCH_SECONDS="1")
        script = f'''
{sys.executable} -B {ROOT / "scripts/winlux-screensaver-inhibit.py"} &
for i in $(seq 1 50); do gdbus call --session -d org.freedesktop.DBus -o /org/freedesktop/DBus -m org.freedesktop.DBus.NameHasOwner org.freedesktop.ScreenSaver 2>/dev/null | grep -q true && break; sleep 0.1; done
{sys.executable} -B -c "$CLIENT"
sleep 1
echo "après départ du client : [$(ls {tmp / "inhibit"})]"
touch {tmp / "busy"}
{sys.executable} -B {ROOT / "scripts/winlux-screensaver-inhibit.py"} watch-updates &
sleep 3
echo "installation : [$(ls {tmp / "inhibit"})]"
rm {tmp / "busy"}
sleep 3
echo "installation finie : [$(ls {tmp / "inhibit"})]"
kill %2 2>/dev/null
'''
        # Client resté sur le bus (comme un navigateur pendant la vidéo) : la
        # demande tient ; il part : elle tombe d'elle-même.
        env["CLIENT"] = f"""
import os
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
reply = bus.call_sync("org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver", "org.freedesktop.ScreenSaver",
                      "Inhibit", GLib.Variant("(ss)", ("test", "video")), None, 0, 5000, None)
print("cookie", reply.unpack()[0])
print("pendant :", os.listdir({str(tmp / "inhibit")!r}))
"""
        out = subprocess.run(["dbus-run-session", "--", "sh", "-c", script], env=env, capture_output=True, text=True, timeout=60)
        assert "cookie 1" in out.stdout, out.stdout + out.stderr
        assert "-screensaver']" in out.stdout, out.stdout + out.stderr
        assert "après départ du client : []" in out.stdout, out.stdout + out.stderr
        assert "installation : [" in out.stdout and "-screensaver]" in out.stdout.split("installation : ")[1].splitlines()[0], out.stdout + out.stderr
        assert "installation finie : []" in out.stdout, out.stdout + out.stderr
        assert "xscreensaver-command -deactivate" in log.read_text()
        calls = log.read_text().splitlines()
        assert "xset -dpms" in calls and calls.index("xset +dpms") > calls.index("xset -dpms"), calls
    print("A7 : service réel sur un bus de session : Inhibit → cookie, économiseur réveillé, DPMS suspendu puis rétabli, demande d'éveil posée puis retirée au départ du client, écran tenu pendant une mise à jour")
else:
    print("(pas de D-Bus ou de PyGObject ici : essai du service réel sauté)")
