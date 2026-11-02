#!/usr/bin/python3
"""Copier le texte d'une image (#280) : langues, nettoyage, moteur absent ou présent, branchements."""
import ast
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
SCRIPT = HERE / "winlux-ocr.py"
m = runpy.run_path(str(SCRIPT), run_name="ocr_test")

# Langue de l'interface d'abord, seulement les quatre langues prévues.
assert m["language_order"](["eng", "fra", "osd", "deu"], "fr") == "fra+eng+deu"
assert m["language_order"](["eng", "fra", "spa", "deu"], "de") == "deu+fra+eng+spa"
assert m["language_order"](["eng", "osd"], "es") == "eng" and m["language_order"](["osd"], "fr") == ""
assert m["clean"]("Bonjour  \n\n\n\nMonde\f\n\n") == "Bonjour\n\nMonde"
assert m["linux_path"]("Z:\\tmp\\a.png") == "/tmp/a.png"
assert m["linux_path"]("C:\\Images\\b.png") == str(Path.home() / "Images/b.png")
assert m["linux_path"]("file:///tmp/c%20d.png") == "/tmp/c d.png"
for key, row in m["_TR"].items():
    assert len(row) == 3 and all(row), key

with tempfile.TemporaryDirectory() as folder:
    # Moteur absent : statut à 0, pas de texte.
    env = dict(os.environ, PATH=folder, WINLUX_OCR_ENGINE="")
    out = subprocess.run([sys.executable, "-I", str(SCRIPT), "status"], env=env, capture_output=True, text=True, timeout=20)
    assert out.returncode == 0 and "installed=0" in out.stdout, out
    # Moteur factice : arguments, langues et nettoyage vérifiés.
    fake = Path(folder) / "tesseract"
    fake.write_text("#!/bin/sh\nif [ \"$1\" = --list-langs ]; then printf 'List of available languages (3):\\neng\\nfra\\nosd\\n'; exit 0; fi\n"
                    "echo \"$@\" > \"$(dirname \"$0\")/args\"\nprintf 'Facture n° 42  \\n\\n\\n\\nTotal : 12,50 €\\f'\n")
    fake.chmod(0o755)
    env = dict(os.environ, WINLUX_OCR_ENGINE=str(fake), WINLUX_LANG="fr")
    out = subprocess.run([sys.executable, "-I", str(SCRIPT), "status"], env=env, capture_output=True, text=True, timeout=20)
    assert "installed=1" in out.stdout and "languages=fra,eng" in out.stdout, out.stdout
    out = subprocess.run([sys.executable, "-I", str(SCRIPT), "text", "/tmp/image.png"], env=env, capture_output=True, text=True, timeout=20)
    assert out.returncode == 0 and out.stdout == "Facture n° 42\n\nTotal : 12,50 €\n", repr(out.stdout)
    assert (Path(folder) / "args").read_text().split() == ["/tmp/image.png", "stdout", "--psm", "3", "-l", "fra+eng"]

# Vrai moteur s'il est là (poste de développement) : texte accentué lu.
if shutil.which("tesseract") and shutil.which("convert"):
    with tempfile.TemporaryDirectory() as folder:
        image = Path(folder) / "texte.png"
        subprocess.run(["convert", "-size", "700x90", "xc:white", "-pointsize", "28", "-fill", "black",
                        "-annotate", "+20+55", "Bonjour, ceci est un essai.", str(image)], check=True, capture_output=True)
        if image.exists():
            text = m["recognize"](image)
            assert text and "Bonjour" in text, text

# Branchements : raccourci, migration, image, Thunar, visionneuse, Paramètres, menu Démarrer.
ns = "{http://openbox.org/3.4/rc}"
rc = ET.parse(ROOT / "integration/native/config/openbox/rc.xml").getroot()
bindings = {k.get("key"): k.findtext(f"{ns}action/{ns}command") for k in rc.iter(f"{ns}keybind")}
assert bindings.get("W-S-t") == "winlux-ocr", bindings
config = (HERE / "winlux-native-config.py").read_text()
assert '"W-S-t"' in config and '"winlux-ocr"' in config
assert "'winlux-ocr.py:winlux-ocr'" in (HERE / "stage-desktop-components.sh").read_text()
uca_file = ROOT / "integration/native/config/Thunar/uca.xml"
uca = ET.parse(uca_file).getroot()
action = next(a for a in uca.iter("action") if (a.findtext("command") or "").startswith("winlux-ocr"))
assert action.find("image-files") is not None and action.findtext("range") == "1"
assert {n.get("{http://www.w3.org/XML/1998/namespace}lang") for n in action.iter("name")} == {None, "en", "es", "de"}
# Les actions Thunar livrées après coup arrivent une fois dans un uca.xml existant.
names = {}
for node in ast.parse(config).body:
    if isinstance(node, ast.FunctionDef) and node.name == "migrate_uca" or \
            isinstance(node, ast.Assign) and getattr(node.targets[0], "id", "") == "UCA_ADDED":
        exec(compile(ast.Module([node], []), "native-config", "exec"), names)
with tempfile.TemporaryDirectory() as folder:
    mine = Path(folder) / "uca.xml"
    mine.write_text('<?xml version="1.0"?>\n<actions>\n<action><name>Perso</name><command>perso %f</command></action>\n</actions>\n')
    names["migrate_uca"](mine, uca_file)
    first = mine.read_text()
    names["migrate_uca"](mine, uca_file)
    assert mine.read_text() == first and "perso %f" in first and "winlux-ocr %f" in first and "winlux-rename %F" in first
    ET.fromstring(first)
viewer = (ROOT / "testexe/src/shell/imageviewer.rs").read_text()
assert "B_OCR => copy_text(s)" in viewer and 'c"Z:\\\\usr\\\\bin\\\\winlux-ocr"' in viewer
pages = (HERE / "winlux-settings-pages.py").read_text()
assert "def ocr_tool():" in pages and "'Copier le texte d’une image':" in pages
desktop = (ROOT / "integration/native/applications/winlux-ocr.desktop").read_text()
for lang in ("en", "es", "de"):
    assert f"Name[{lang}]=" in desktop and f"Comment[{lang}]=" in desktop
print("copier le texte d'une image : OK")
