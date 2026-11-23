#!/usr/bin/env python3
"""Xorg qui ne démarre pas : seconde tentative, jamais de console root (#3148).

Exécute les fonctions xorg_start, xorg_wait, display_unavailable et
xorg_recover de l'init (scripts/build-live-iso.sh) avec un faux Xorg, un faux
BusyBox et des chemins temporaires :
1. Xorg échoue une fois : la configuration générée par Winlux (*-winlux-*.conf)
   est écartée, celle d'un paquet reste, la seconde tentative tient, le repère
   xorg-secours est noté et l'init continue.
2. Xorg échoue deux fois sur un système installé : le message d'affichage
   impossible est écrit, aucun shell ne lit l'entrée, et le bouton
   d'alimentation (SIGUSR2 → safe_power) est servi tout de suite.
3. Session qui sort trois fois de suite : écran de secours, puis message
   quand l'affichage se perd, jamais de shell.
4. Écran de secours : pas de bouton Terminal hors session live, quel que
   soit le scénario qui le lance ; proposé en session live.
"""
import os
import pathlib
import re
import signal
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
source = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
init = re.search(r'^cat >"\$stage/init" <<.INIT.\n(.*?)^INIT$', source, re.S | re.M).group(1)


def function(name):
    found = re.search(rf"^    {name}\(\) \{{\n.*?^    \}}\n", init, re.S | re.M)
    assert found, f"{name} introuvable dans l'init"
    return found.group(0)


args = re.search(r"^    xorg_args=.*\n", init, re.M).group(0)
body = args + "".join(function(n) for n in ("xorg_start", "xorg_wait", "display_unavailable", "xorg_recover",
                                           "session_failed"))


def scenario(failures, live=False, tail=None):
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="xorg-secours-"))
    for d in ("tmp/.X11-unix", "run/winlux", "conf.d", "etc/winlux", "etc/default"):
        (tmp / d).mkdir(parents=True, exist_ok=True)
    (tmp / "conf.d/20-winlux-nvidia.conf").write_text("Section \"Device\"\nEndSection\n")
    (tmp / "conf.d/10-paquet.conf").write_text("# fichier d'un paquet\n")
    (tmp / "etc/default/locale").write_text("LANG=fr_FR.UTF-8\n")
    if live:
        (tmp / "etc/winlux/live-session").write_text("")
    count = tmp / "essais"
    xorg = tmp / "Xorg"
    xorg.write_text(f"""#!{sys.executable}
import os, socket, sys, time
p = {str(count)!r}
n = int(open(p).read()) + 1 if os.path.exists(p) else 1
open(p, "w").write(str(n))
open({str(tmp / 'tmp/Xorg.0.log')!r}, "w").write("essai %d\\n" % n)
if n <= {failures}:
    sys.exit(1)
s = socket.socket(socket.AF_UNIX)
s.bind({str(tmp / 'tmp/.X11-unix/X0')!r})
time.sleep(60)
""")
    busybox = tmp / "busybox"
    busybox.write_text('#!/bin/sh\ncmd=$1; shift\ncase "$cmd" in chvt) exit 0 ;; esac\nexec "$cmd" "$@"\n')
    update = tmp / "winlux-update"
    update.write_text("#!/bin/sh\nexit 1\n")
    # Faux écran de secours : note son lancement, puis l'affichage se perd.
    rescue = tmp / "rescue"
    rescue.write_text(f"#!/bin/sh\necho lance >>{tmp / 'rescue.log'}\nkill \"$XORG_PID\"\n")
    for f in (xorg, busybox, update, rescue):
        f.chmod(0o755)
    # /tmp/ d'abord : les chemins temporaires insérés ensuite sont sous /tmp.
    text = (body.replace("/tmp/", str(tmp / "tmp") + "/")
            .replace("/bin/busybox", str(busybox))
            .replace("/usr/bin/Xorg", str(xorg))
            .replace("/usr/bin/xrandr", str(tmp / "absent-xrandr"))
            .replace("/usr/bin/winlux-update", str(update))
            .replace("/usr/bin/winlux-rescue-screen", str(tmp / "rescue"))
            .replace("/usr/share/X11/xorg.conf.d", str(tmp / "conf.d"))
            .replace("/etc/X11/xorg.conf.d", str(tmp / "etc-conf.d"))
            .replace("/etc/winlux/", str(tmp / "etc/winlux") + "/")
            .replace("/etc/default/locale", str(tmp / "etc/default/locale"))
            .replace("/dev/tty1", str(tmp / "tty1"))
            .replace("/run/winlux", str(tmp / "run/winlux")))
    script = f"""
boot_mark() {{ echo "$1" >>{tmp / 'marks'}; }}
trap 'echo ALIMENTATION; exit 0' USR2
{text}
{tail or """xorg_start
xorg_wait
if ! kill -0 "$xorg_pid" 2>/dev/null; then
    xorg_recover
fi
echo SUITE_DE_L_INIT
kill "$xorg_pid" 2>/dev/null"""}
"""
    proc = subprocess.Popen(["sh", "-c", script], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, start_new_session=True)
    # Un shell qui lirait l'entrée exécuterait cette ligne.
    proc.stdin.write("echo SHELL_ROOT\n")
    proc.stdin.flush()
    return tmp, proc


def finish(proc, wait=15):
    try:
        out = proc.communicate(timeout=wait)[0]
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out = proc.communicate()[0]
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    return out


# 1. Un échec, puis l'affichage générique tient.
tmp, proc = scenario(failures=1)
out = finish(proc)
print(out, end="")
assert "SUITE_DE_L_INIT" in out, "l'init ne continue pas après la seconde tentative"
assert "SHELL_ROOT" not in out, "un shell a lu l'entrée"
assert (tmp / "essais").read_text() == "2", "pas de seconde tentative"
assert not (tmp / "conf.d/20-winlux-nvidia.conf").exists(), "configuration Winlux pas écartée"
assert (tmp / "run/winlux/xorg-secours/20-winlux-nvidia.conf").exists(), "configuration écartée perdue"
assert (tmp / "conf.d/10-paquet.conf").exists(), "fichier d'un paquet déplacé"
assert (tmp / "run/winlux/xorg-secours/Xorg.premier-essai.log").read_text() == "essai 1\n"
assert (tmp / "marks").read_text().split() == ["xorg-secours"], (tmp / "marks").read_text()
print("un échec : seconde tentative sans *-winlux-*.conf, repère xorg-secours, l'init continue")

# 2. Deux échecs sur un système installé : message, pas de shell, alimentation servie.
tmp, proc = scenario(failures=2)
deadline = time.monotonic() + 15
while time.monotonic() < deadline and not (tmp / "tty1").exists():
    time.sleep(0.05)
time.sleep(0.3)
assert proc.poll() is None, "l'init s'est arrêtée au lieu d'attendre"
started = time.monotonic()
os.killpg(proc.pid, signal.SIGUSR2)
out = finish(proc, wait=5)
print(out, end="")
elapsed = time.monotonic() - started
screen = (tmp / "tty1").read_text()
assert "n'a pas pu démarrer l'affichage" in screen and "Winlux secours" in screen, screen
assert "WINLUX_DISPLAY_UNAVAILABLE" in out
assert "SHELL_ROOT" not in out and "SUITE_DE_L_INIT" not in out, "shell ou session après deux échecs"
assert "ALIMENTATION" in out and elapsed < 3, f"bouton d'alimentation pas servi ({elapsed:.1f} s)"
assert (tmp / "run/winlux/xorg-secours/Xorg.second-essai.log").exists()
print(f"deux échecs : message à l'écran, aucun shell, alimentation servie en {elapsed:.2f} s")

# 3. Session qui ne tient pas (trois sorties rapides) : écran de secours, puis
#    l'affichage perdu donne le message, jamais un shell.
tmp, proc = scenario(failures=0, tail="""xorg_start
xorg_wait
XORG_PID=$xorg_pid; export XORG_PID
session_failed
echo SUITE_DE_L_INIT""")
deadline = time.monotonic() + 15
while time.monotonic() < deadline and not (tmp / "tty1").exists():
    time.sleep(0.05)
time.sleep(0.3)
os.killpg(proc.pid, signal.SIGUSR2)
out = finish(proc, wait=5)
print(out, end="")
assert (tmp / "rescue.log").read_text().split() == ["lance"], "écran de secours pas lancé"
assert "WINLUX_DISPLAY_UNAVAILABLE" in out and "ALIMENTATION" in out
assert "SHELL_ROOT" not in out and "SUITE_DE_L_INIT" not in out, "shell ou suite après l'échec de session"
print("session en échec : écran de secours, puis message, aucun shell")

# 4. Écran de secours : Terminal seulement en session live, quel que soit le
#    scénario qui le lance (il décide lui-même, l'init ne lui passe rien).
sys.path.insert(0, str(ROOT / "scripts"))
import importlib.util
spec = importlib.util.spec_from_file_location("secours", ROOT / "scripts/winlux-rescue-screen.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
real_exists = os.path.exists
module.os.path.exists = lambda p: False if p == "/etc/winlux/live-session" else real_exists(p)
assert module.terminal_command() is None, "Terminal root proposé sur un système installé"
module.os.path.exists = lambda p: True if p == "/etc/winlux/live-session" else real_exists(p)
live_terminal = module.terminal_command()
module.os.path.exists = real_exists
print("écran de secours : Terminal en live :", bool(live_terminal) or "aucun terminal installé ici")
print("écran de secours : pas de Terminal hors session live")
print("Xorg en échec : OK")
