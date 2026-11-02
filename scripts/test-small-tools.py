#!/usr/bin/python3
"""Petits outils (#279) : pipette Windows + Maj + C, premier plan Windows + Ctrl + T, renommage en masse."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent

# ── Pipette ──
color = runpy.run_path(str(HERE / "winlux-colorpicker.py"), run_name="colorpicker_test")
assert color["parse_hex"]("#336699") == (51, 102, 153) and color["parse_hex"]("abc") == (170, 187, 204)
assert color["parse_hex"]("#12345") is None and color["parse_hex"]("zzzzzz") is None
assert color["formats"]((51, 102, 153)) == [("HEX", "#336699"), ("RGB", "rgb(51, 102, 153)"), ("HSL", "hsl(210, 50%, 40%)")]
assert color["hsl_text"]((255, 255, 255)) == "hsl(0, 0%, 100%)" and color["hsl_text"]((255, 0, 0)) == "hsl(0, 100%, 50%)"
history = []
for number in range(25):
    history = color["remember"](history, "#%06x" % number)
assert len(history) == 20 and history[0] == "#000018"
history = color["remember"](history, "#000010")
assert history[0] == "#000010" and history.count("#000010") == 1 and len(history) == 20
for key, row in color["_TR"].items():
    assert len(row) == 3 and all(row), key
with tempfile.TemporaryDirectory() as folder:
    env = dict(os.environ, XDG_STATE_HOME=folder)
    script = str(HERE / "winlux-colorpicker.py")
    out = subprocess.run([sys.executable, "-I", script, "format", "#0F6CBD"], env=env, capture_output=True, text=True, timeout=20)
    assert out.returncode == 0 and "RGB rgb(15, 108, 189)" in out.stdout, out
    Path(folder, "winlux").mkdir()
    Path(folder, "winlux/colors").write_text("#112233\npas une couleur\n#abc\n")
    out = subprocess.run([sys.executable, "-I", script, "history"], env=env, capture_output=True, text=True, timeout=20)
    assert out.stdout.split() == ["#112233", "#AABBCC"], out.stdout

# ── Toujours au premier plan : cadre autour de la fenêtre ──
snap = runpy.run_path(str(HERE / "winlux-snap.py"), run_name="snap_test")
assert snap["border_rects"]((100, 50, 400, 300), 3) == [(97, 47, 406, 3), (97, 350, 406, 3), (97, 50, 3, 300), (500, 50, 3, 300)]
assert "ontop" in snap["__doc__"]

# ── Renommage en masse ──
rename = runpy.run_path(str(HERE / "winlux-rename.py"), run_name="rename_test")
new_name, plan, execute, undo = (rename[n] for n in ("new_name", "plan", "execute", "undo"))
assert rename["split_name"]("archive.tar.gz") == ("archive.tar", ".gz") and rename["split_name"](".bashrc") == (".bashrc", "")
assert new_name("IMG_0042.JPG", 0, {"find": "img_", "replace": "Vacances "}) == "Vacances 0042.JPG"
assert new_name("IMG_0042.JPG", 0, {"find": "img_", "replace": "x", "match_case": True}) == "IMG_0042.JPG"
assert new_name("IMG_0042.JPG", 4, {"base": "Été 2026", "start": 1, "digits": 2, "ext": "lower"}) == "Été 2026 (05).jpg"
assert new_name("a.txt", 2, {"base": "Note-#", "start": 10}) == "Note-12.txt"
assert new_name("rapport final.odt", 0, {"case": "title", "prefix": "2026 - "}) == "2026 - Rapport Final.odt"
assert new_name("photo-12-03.png", 0, {"find": r"(\d+)-(\d+)", "replace": r"\2_\1", "regex": True}) == "photo-03_12.png"
assert new_name("a+b.txt", 0, {"find": "+", "replace": "\\"}) == "a\\b.txt"   # texte simple, pas d'expression

with tempfile.TemporaryDirectory() as folder:
    os.environ["XDG_STATE_HOME"] = str(Path(folder) / "state")
    d = Path(folder) / "photos"
    d.mkdir()
    for name in ("b.jpg", "a.jpg", "c.jpg", "garde.txt"):
        (d / name).write_text(name)
    files = [d / "b.jpg", d / "a.jpg", d / "c.jpg"]
    rows = plan(files, {"base": "Photo", "order": "name"})
    assert [(p.name, n, e) for p, n, e in rows] == [("a.jpg", "Photo (1).jpg", ""), ("b.jpg", "Photo (2).jpg", ""), ("c.jpg", "Photo (3).jpg", "")]
    # Problèmes : doublon, caractère interdit, nom vide, fichier existant, expression invalide.
    assert {e for _, _, e in plan(files, {"find": "[abc]", "replace": "x", "regex": True})} == {"nom en double"}
    assert {e for _, _, e in plan(files[:1], {"replace": "?", "find": "b"})} == {"caractère interdit"}
    assert {e for _, _, e in plan(files[:1], {"find": "b", "replace": "con"})} == {"caractère interdit"}
    assert {e for _, _, e in plan(files[:1], {"find": "b", "replace": ""})} == {"nom vide"}
    assert [e for _, _, e in plan([d / "a.jpg"], {"find": "a.jpg", "replace": "garde"})] == [""], "l'extension n'est pas dans la recherche"
    assert [e for _, _, e in plan([d / "b.jpg"], {"find": "b", "replace": "a"})] == ["existe déjà"]
    assert [e for _, _, e in plan([d / "a.jpg"], {"find": "(", "regex": True})] == ["expression régulière invalide"]
    # Échange de noms : a ↔ b, sans conflit grâce aux noms provisoires.
    swap = [(d / "a.jpg", "b.jpg", ""), (d / "b.jpg", "a.jpg", "")]
    done, failed = execute(swap)
    assert failed == 0 and len(done) == 2 and (d / "a.jpg").read_text() == "b.jpg" and (d / "b.jpg").read_text() == "a.jpg"
    # Renommage puis annulation.
    done, failed = execute(plan(files, {"base": "Photo", "order": "name"}))
    assert failed == 0 and sorted(p.name for p in d.iterdir()) == ["Photo (1).jpg", "Photo (2).jpg", "Photo (3).jpg", "garde.txt"]
    assert rename["can_undo"]()
    assert undo() == (3, 0) and sorted(p.name for p in d.iterdir()) == ["a.jpg", "b.jpg", "c.jpg", "garde.txt"]
    assert not rename["can_undo"]()
    # Changer seulement la casse est permis.
    assert [e for _, _, e in plan([d / "garde.txt"], {"case": "upper"})] == [""]
    # Commandes : un dossier = ses fichiers ; apply puis undo.
    env = dict(os.environ)
    script = str(HERE / "winlux-rename.py")
    out = subprocess.run([sys.executable, "-I", script, "preview", '{"prefix": "x-"}', str(d)], env=env, capture_output=True, text=True, timeout=20)
    assert out.returncode == 0 and out.stdout.splitlines()[0] == "a.jpg\tx-a.jpg\t", out
    out = subprocess.run([sys.executable, "-I", script, "apply", json.dumps({"ext": "upper"}), str(d / "c.jpg")], env=env, capture_output=True, text=True, timeout=20)
    assert out.stdout.strip() == "1" and (d / "c.JPG").exists()
    subprocess.run([sys.executable, "-I", script, "undo"], env=env, check=True, capture_output=True, timeout=20)
    assert (d / "c.jpg").exists() and not (d / "c.JPG").exists()
    del os.environ["XDG_STATE_HOME"]
for key, row in rename["_TR"].items():
    assert len(row) == 3 and all(row), key

# ── Branchements : raccourcis, migration, image, Explorateur, Thunar ──
ns = "{http://openbox.org/3.4/rc}"
rc = ET.parse(ROOT / "integration/native/config/openbox/rc.xml").getroot()
bindings = {k.get("key"): k.findtext(f"{ns}action/{ns}command") for k in rc.iter(f"{ns}keybind")}
assert bindings.get("W-C-t") == "winlux-snap ontop" and bindings.get("W-S-c") == "winlux-colorpicker", bindings
config = (HERE / "winlux-native-config.py").read_text()
assert '"W-C-t"' in config and '"W-S-c"' in config
stage = (HERE / "stage-desktop-components.sh").read_text()
for name in ("colorpicker", "rename"):
    assert f"'winlux-{name}.py:winlux-{name}'" in stage, name
uca = ET.parse(ROOT / "integration/native/config/Thunar/uca.xml").getroot()
action = next(a for a in uca.iter("action") if (a.findtext("command") or "").startswith("winlux-rename"))
assert {n.get("{http://www.w3.org/XML/1998/namespace}lang") for n in action.iter("name")} == {None, "en", "es", "de"}
explorer = (ROOT / "testexe/src/shell/explorer.rs").read_text(encoding="utf-8")
assert "Item::BulkRename" in explorer and 'b"winlux-rename"' in explorer
desktop = (ROOT / "integration/native/applications/winlux-colorpicker.desktop").read_text()
for lang in ("en", "es", "de"):
    assert f"Name[{lang}]=" in desktop
print("petits outils : OK")
