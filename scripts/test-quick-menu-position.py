#!/usr/bin/python3
"""Windows + X : le menu d'accès rapide s'ouvre au-dessus du bouton Démarrer.

Relevé VM du 7 novembre 2026 (#574) : le menu s'ouvrait sous le pointeur de la
souris (ShowMenu d'openbox sans <position>). Windows 11 l'ouvre au-dessus du
bouton Démarrer, quel que soit l'endroit du pointeur.

1. rc.xml livré : position au-dessus de la barre (alignement à gauche par
   défaut, barre de 40 px).
2. winlux-run place-menu réécrit la position dans le rc.xml du compte
   (copie figée) selon l'alignement et la taille de la barre ; appelé par
   winlux-control align-* (chaque session) et par la taille de la barre.
3. openbox réel (si présent) : menu collé au bord gauche, juste au-dessus
   de la barre, pointeur ailleurs.
"""
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
failures = []


def ok(condition, label):
    print(("OK   " if condition else "ÉCHEC ") + label)
    if not condition:
        failures.append(label)


# ── 1. Modèle livré ─────────────────────────────────────────────────────────
shipped = (ROOT / "integration/native/config/openbox/rc.xml").read_text()
ok('<keybind key="W-x"><action name="ShowMenu"><menu>winlux-quick-link</menu>'
   '<position><x>0</x><y>-40</y></position></action></keybind>' in shipped,
   "rc.xml livré : menu au-dessus du bouton Démarrer (à gauche, barre de 40 px)")

# ── 2. Réécriture dans le rc.xml du compte ──────────────────────────────────
run = runpy.run_path(str(ROOT / "scripts/winlux-run.py"), run_name="quick_menu_test")
old = ('<?xml version="1.0"?>\n<openbox_config xmlns="http://openbox.org/3.4/rc">\n<keyboard>\n'
       '    <keybind key="W-r"><action name="Execute"><command>winlux-run</command></action></keybind>\n'
       '    <keybind key="W-x"><action name="ShowMenu"><menu>winlux-quick-link</menu></action></keybind>\n'
       '</keyboard>\n</openbox_config>\n')
with tempfile.TemporaryDirectory() as folder:
    config = Path(folder) / "rc.xml"
    config.write_text(old)
    os.environ.pop("DISPLAY", None)
    ok(run["place_menu"]("left", "40", config) == 0, "place-menu à gauche réussit")
    text = config.read_text()
    ok("<menu>winlux-quick-link</menu><position><x>0</x><y>-40</y></position></action>" in text,
       "ancien rc.xml (menu sous le pointeur) : position ajoutée")
    ok(text.replace("<position><x>0</x><y>-40</y></position>", "") == old, "reste du rc.xml intact")
    mtime = config.stat().st_mtime_ns
    run["place_menu"]("left", "40", config)
    ok(config.stat().st_mtime_ns == mtime, "deuxième passage : fichier non réécrit (pas de rechargement d'openbox)")
    run["place_menu"]("center", "48", config)
    text = config.read_text()
    ok(text.count("<position>") == 1 and "<position><x>center</x><y>-48</y></position>" in text,
       "barre centrée de 48 px : menu centré au-dessus de la barre, une seule position")
    ok(run["place_menu"]("droite", "40", config) == 2, "alignement inconnu refusé")

control = (ROOT / "scripts/winlux-control.sh").read_text()
block = control[control.index("        align-left|align-center)"):]
block = block[:block.index('TASKBAR_ALIGN "$alignment" ;;')]
ok('winlux-run place-menu "$alignment"' in block, "winlux-control align-* place le menu (appelé à chaque session)")
ok('"winlux-run","place-menu"' in (ROOT / "scripts/winlux-personalize.py").read_text(),
   "nouvelle taille de barre : menu replacé")

# ── 3. openbox réel ─────────────────────────────────────────────────────────
tools = [shutil.which(name) for name in ("Xvfb", "openbox", "xdotool", "import")]
try:
    from PIL import Image
except ImportError:
    tools.append(None)
if not all(tools):
    print("Xvfb, openbox, xdotool, import ou PIL absent : openbox réel non lancé")
else:
    with tempfile.TemporaryDirectory() as folder:
        work = Path(folder)
        (work / "menu.xml").write_text(
            '<openbox_menu><menu id="winlux-quick-link" label="Accès rapide">'
            + "".join(f'<item label="Élément {n}"/>' for n in range(6)) + '</menu></openbox_menu>')
        config = work / "rc.xml"
        config.write_text(f'<?xml version="1.0"?><openbox_config xmlns="http://openbox.org/3.4/rc">'
                          f'<menu><file>{work}/menu.xml</file></menu><keyboard>'
                          '<keybind key="W-x"><action name="ShowMenu"><menu>winlux-quick-link</menu></action></keybind>'
                          '</keyboard></openbox_config>')
        run["place_menu"]("left", "40", config)
        display = ":%d" % (90 + os.getpid() % 100)
        server = subprocess.Popen(["Xvfb", display, "-screen", "0", "1280x800x24"],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        env = {**os.environ, "DISPLAY": display}
        try:
            time.sleep(1.5)
            wm = subprocess.Popen(["openbox", "--config-file", str(config)], env=env,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(1.5)
            subprocess.run(["xdotool", "mousemove", "900", "200"], env=env, check=True)
            subprocess.run(["xdotool", "key", "super+x"], env=env, check=True)
            time.sleep(1)
            subprocess.run(["import", "-window", "root", str(work / "shot.png")], env=env, check=True)
            wm.terminate()
            shot = Image.open(work / "shot.png").convert("L")
            box = shot.point(lambda value: 255 if value > 8 else 0).getbbox()
            ok(box is not None and box[0] == 0 and box[3] == 800 - 40,
               f"menu contre le bord gauche, juste au-dessus de la barre (cadre {box})")
        finally:
            server.terminate()

if failures:
    raise SystemExit(f"{len(failures)} échec(s)")
print("OK: Windows + X ouvre le menu au-dessus du bouton Démarrer")
