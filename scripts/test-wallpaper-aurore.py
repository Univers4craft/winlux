#!/usr/bin/python3
"""Fond « Winlux Aurore » (#596) : six images livrées, bonnes définitions,
choix de la définition selon l'écran dans les deux sessions."""
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
WALL = ROOT / "assets" / "wallpapers"
failures = []


def check(ok, message):
    if not ok:
        failures.append(message)


for name in ("winlux-prisme-aurore", "winlux-prisme-aurore-clair"):
    for (w, h), suffix in (((1920, 1080), ""), ((2560, 1440), "-2560x1440"), ((3840, 2160), "-3840x2160")):
        path = WALL / f"{name}{suffix}.png"
        check(path.is_file(), f"{path.name} manquant")
        if path.is_file():
            head = path.read_bytes()[:24]
            size = struct.unpack(">II", head[16:24]) if head[:8] == b"\x89PNG\r\n\x1a\n" else None
            check(size == (w, h), f"{path.name} : {size} au lieu de {(w, h)}")

# Même fonction aurora_wallpaper dans les deux sessions, essayée sur un faux
# dossier : 4K, 1440p, 1080p, définition absente, écran de hauteur inconnue.
for script in ("winlux-native-session.sh", "winlux-user-session.sh"):
    text = (ROOT / "scripts" / script).read_text()
    match = re.search(r"^ *aurora_wallpaper\(\) \{\n.*?^ *\}\n", text, re.M | re.S)
    check(match is not None, f"{script} : aurora_wallpaper absente")
    if not match:
        continue
    with tempfile.TemporaryDirectory() as folder:
        base = Path(folder) / "winlux-prisme-aurore"
        function = match.group(0).replace("/usr/share/winlux/wallpapers/winlux-prisme-aurore", str(base))
        for suffix in ("", "-3840x2160"):
            Path(f"{base}{suffix}.png").write_bytes(b"png")
        for height, expected in (("2160", "-3840x2160"), ("1440", ""), ("1080", ""), ("0", ""), ("", "")):
            out = subprocess.run(["sh", "-c", function + f'aurora_wallpaper "{height}"'],
                                 capture_output=True, text=True).stdout.strip()
            check(out == f"{base}{expected}.png", f"{script} : hauteur {height!r} → {out}")
        Path(f"{base}-2560x1440.png").write_bytes(b"png")
        out = subprocess.run(["sh", "-c", function + "aurora_wallpaper 1440"], capture_output=True, text=True).stdout.strip()
        check(out == f"{base}-2560x1440.png", f"{script} : 1440p non choisi ({out})")

if failures:
    print("ÉCHEC :\n  " + "\n  ".join(failures), file=sys.stderr)
    sys.exit(1)
print("fond Aurore : OK")
