#!/usr/bin/python3
"""Bouton d'alimentation pendant le verrouillage (#2254) : l'écran d'arrêt
demande la saisie à winlux-lock, qui ne la cède qu'à un vrai processus
winlux-power (pid vivant, propriétaire root ou le compte verrouillé). Hors
ligne, sans affichage."""
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time

here = Path(__file__).resolve().parent
tmp = Path(tempfile.mkdtemp())
os.environ["WINLUX_LOCK_ROOT"] = str(tmp)
os.environ["WINLUX_CONTROL_ROOT"] = str(tmp)
os.environ["DISPLAY"] = ":0"
os.environ["WINLUX_LANG"] = "fr"
(tmp / "etc/winlux").mkdir(parents=True)
(tmp / "run/winlux/lock").mkdir(parents=True)
lock = runpy.run_path(str(here / "winlux-lock.py"), run_name="lock_test")
power = runpy.run_path(str(here / "winlux-power.py"), run_name="power_test")
request = tmp / "run/winlux/lock/power"

# Rien de demandé : le verrou garde la saisie.
assert lock["power_request_pid"]() is None

# Faux écran d'arrêt : un programme nommé winlux-power, et un intrus.
bins = Path(tempfile.mkdtemp())
fake = bins / "winlux-power"
fake.write_text("import time\ntime.sleep(30)\n")
intrus = bins / "autre-programme"
intrus.write_text("import time\ntime.sleep(30)\n")
genuine = subprocess.Popen([sys.executable, str(fake)])
other = subprocess.Popen([sys.executable, str(intrus)])
try:
    time.sleep(0.2)
    request.write_text(f"{genuine.pid}\n")
    assert lock["power_request_pid"]() == genuine.pid, "vrai écran d'arrêt refusé"
    request.write_text(f"{other.pid}\n")
    assert lock["power_request_pid"]() is None, "un autre programme obtient la saisie du verrou"
    request.write_text("999999999\n")
    assert lock["power_request_pid"]() is None, "pid mort accepté"
    request.write_text("pas un pid\n")
    assert lock["power_request_pid"]() is None
    if os.getuid() == 0:
        # Fichier d'un autre compte : ignoré même s'il désigne un vrai winlux-power.
        request.write_text(f"{genuine.pid}\n")
        os.chown(request, 4242, 4242)
        lock["os"].getuid = lambda: 1000
        assert lock["power_request_pid"]() is None, "demande d'un autre compte acceptée"
        lock["os"].getuid = os.getuid
finally:
    genuine.kill()
    other.kill()
request.unlink(missing_ok=True)

# Côté winlux-power : marqueur de verrou, demande puis retrait.
assert not power["screen_locked"]()
(tmp / "run/winlux/lock/active").write_text(f"{os.getpid()}\n")
assert power["screen_locked"]()
(tmp / "run/winlux/lock/active").write_text("999999999\n")
assert not power["screen_locked"]()
old_umask = os.umask(0o077)  # umask du démon d'alimentation root
try:
    assert power["request_lock_yield"]()
finally:
    os.umask(old_umask)
assert request.read_text().strip() == str(os.getpid())
assert request.stat().st_mode & 0o777 == 0o644, oct(request.stat().st_mode)
power["release_lock_yield"]()
assert not request.exists()
# Ne retire jamais la demande d'un autre écran d'arrêt.
request.write_text("12345\n")
power["release_lock_yield"]()
assert request.exists()

# Second écran (:1) : marqueurs séparés, comme winlux-lock.
os.environ["DISPLAY"] = ":1"
assert power["power_request_path"]().endswith("/run/winlux/lock/power-1")
assert str(lock["power_request_file"]()).endswith("/run/winlux/lock/power-1")

src = (here / "winlux-power.py").read_text(encoding="utf-8")
assert 'log(f"saisie refusée' in src, "échec de saisie de nouveau silencieux"
assert "shown = window_class.split(\".\")[-1] if self.locked else title" in src, "titres de fenêtres affichés sur l'écran verrouillé"
daemon = (here.parent / "runtime/src/power/daemon.rs").read_text(encoding="utf-8")
branch = daemon[daemon.index("Hw::PowerButton => {"):]
branch = branch[:branch.index("}")]
assert "display_on(root)" in branch and "touch_activity_at(root)" in branch, "bouton : écran laissé éteint"
print("OK: bouton d'alimentation pendant le verrouillage, écran d'arrêt atteignable, demande vérifiée (#2254)")
