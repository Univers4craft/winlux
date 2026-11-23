#!/usr/bin/python3
"""Capture d'écran (#3046) : logique pure, puis vraie capture sous Xvfb.

    xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-capture.py

Sans DISPLAY, GTK ou xdotool, seule la partie sans écran tourne.
"""
import os
from pathlib import Path
import runpy
import shutil
import struct
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
TOOL = HERE / "winlux-capture.py"
os.environ["WINLUX_LANG"] = "fr"
m = runpy.run_path(str(TOOL), run_name="test_capture")

# ── Sans écran ──
assert m["normalized"]((50, 40), (10, 20)) == (10, 20, 40, 20)
assert m["clipped"]((-10, -5, 100, 50), 1366, 768) == (0, 0, 90, 45)
assert m["clipped"]((1400, 10, 50, 50), 1366, 768) is None
stack = [(0, 0, 1366, 768), (100, 100, 400, 300), (300, 200, 400, 300)]   # du bas vers le haut
assert m["window_at"](stack, 350, 250) == (300, 200, 400, 300), "la fenêtre du dessus gagne"
assert m["window_at"](stack, 150, 150) == (100, 100, 400, 300)
assert m["window_at"]([], 1, 1) is None

home = Path(tempfile.mkdtemp(prefix="test-capture-"))
(home / ".config").mkdir()
(home / ".config/user-dirs.dirs").write_text('XDG_DESKTOP_DIR="$HOME/Bureau"\nXDG_PICTURES_DIR="$HOME/Photos perso"\n')
assert m["captures_dir"](home) == home / "Photos perso/Captures d’écran"
(home / ".config/user-dirs.dirs").write_text('XDG_PICTURES_DIR="$HOME"\n')
assert m["captures_dir"](home) == home / "Images/Captures d’écran", "Images par défaut si XDG pointe le dossier personnel"
folder = home / "c"
folder.mkdir()
moment = time.struct_time((2026, 11, 22, 20, 30, 15, 6, 326, -1))
first = m["file_name"](moment, folder)
assert first.name == "Capture d’écran 2026-11-22 203015.png", first.name
first.write_bytes(b"x")
assert m["file_name"](moment, folder).name == "Capture d’écran 2026-11-22 203015 (2).png"
print("logique : ok")

# ── Sous Xvfb ──
python = sys.executable
gi_ok = subprocess.run([python, "-c", "import gi; gi.require_version('Gtk', '3.0'); from gi.repository import Gtk"],
                       capture_output=True).returncode == 0
if not os.environ.get("DISPLAY") or not gi_ok:
    print("capture réelle : ignorée (pas de DISPLAY ou pas de GTK)")
    sys.exit(0)

env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"), NO_AT_BRIDGE="1")
(home / ".config/user-dirs.dirs").write_text('XDG_PICTURES_DIR="$HOME/Images"\n')
shots = home / "Images/Captures d’écran"


def png_size(path):
    data = path.read_bytes()[:24]
    assert data[:8] == b"\x89PNG\r\n\x1a\n", f"{path} n'est pas un PNG"
    return struct.unpack(">II", data[16:24])


def screen_size():
    out = subprocess.run(["xdpyinfo"], capture_output=True, text=True).stdout if shutil.which("xdpyinfo") else ""
    for line in out.splitlines():
        if "dimensions:" in line:
            w, h = line.split()[1].split("x")
            return int(w), int(h)
    return 1366, 768


def newest():
    files = sorted(shots.glob("*.png"), key=lambda p: p.stat().st_mtime)
    return files[-1] if files else None


# Plein écran sans question (Windows + Impr. écran) : PNG de la taille de l'écran.
started = time.monotonic()
run = subprocess.run([python, "-B", str(TOOL), "ecran"], env=env, capture_output=True, text=True, timeout=30)
elapsed = (time.monotonic() - started) * 1000
print(f"ecran : rc={run.returncode} en {elapsed:.0f} ms, sortie={run.stdout.strip()!r} erreurs={run.stderr.strip()[-300:]!r}")
assert run.returncode == 0
path = Path(run.stdout.strip().splitlines()[-1])
assert path.parent == shots and path.is_file(), path
assert png_size(path) == screen_size(), (png_size(path), screen_size())
if shutil.which("xclip"):
    targets = subprocess.run(["xclip", "-selection", "clipboard", "-o", "-t", "TARGETS"],
                             capture_output=True, text=True, timeout=5).stdout
    print(f"presse-papiers : {targets.split()}")
    assert "image/png" in targets.split(), "l'image doit rester au presse-papiers après la fermeture de l'outil"
    served = subprocess.run(["xclip", "-selection", "clipboard", "-o", "-t", "image/png"],
                            capture_output=True, timeout=5).stdout
    assert served == path.read_bytes(), "le presse-papiers sert le PNG enregistré"
print("ecran : ok")

# Zone tracée à la souris : le PNG a la taille du rectangle.
if shutil.which("xdotool"):
    before = newest()
    started = time.monotonic()
    proc = subprocess.Popen([python, "-B", str(TOOL)], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    shown = None
    for _ in range(100):
        grab = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(proc.pid)],
                              capture_output=True, text=True).stdout.strip()
        if grab:
            shown = (time.monotonic() - started) * 1000
            break
        time.sleep(0.05)
    print(f"zone : écran figé affiché en {shown:.0f} ms" if shown else "zone : fenêtre non trouvée par xdotool")
    time.sleep(0.3)
    for command in (["mousemove", "200", "300"], ["mousedown", "1"], ["mousemove", "260", "340"],
                    ["mousemove", "320", "380"], ["mouseup", "1"]):
        subprocess.run(["xdotool", *command], check=True)
        time.sleep(0.08)
    out, err = proc.communicate(timeout=20)
    print(f"zone : rc={proc.returncode} sortie={out.strip()!r} erreurs={err.strip()[-300:]!r}")
    assert proc.returncode == 0
    assert "Error" not in err, "l'écran figé doit se dessiner sans erreur (python3-gi-cairo)"
    path = newest()
    assert path and path != before, "une nouvelle capture est enregistrée"
    assert png_size(path) == (120, 80), png_size(path)
    print("zone : ok")

    # Échap : rien n'est enregistré.
    count = len(list(shots.glob("*.png")))
    proc = subprocess.Popen([python, "-B", str(TOOL)], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    time.sleep(1.5)
    subprocess.run(["xdotool", "key", "Escape"], check=True)
    proc.communicate(timeout=10)
    assert proc.returncode == 0 and len(list(shots.glob("*.png"))) == count, "Échap n'enregistre rien"
    print("échap : ok")

# Rien ne reste ouvert : l'outil s'est fermé, seul xclip garde l'image.
left = subprocess.run(["pgrep", "-f", str(TOOL)], capture_output=True, text=True).stdout.split()
assert not left, f"winlux-capture encore en mémoire : {left}"
print("test-capture : ok")
