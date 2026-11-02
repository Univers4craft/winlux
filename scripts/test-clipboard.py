#!/usr/bin/python3
"""Historique du presse-papiers Win+V : 25 éléments, épingles, doublons, réglage, raccourci."""
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "winlux-clipboard.py"
module = runpy.run_path(str(SCRIPT), run_name="clipboard_test")
add, text_item, clear, toggle_pin, remove = (module[name] for name in ("add", "text_item", "clear", "toggle_pin", "remove"))

# 25 éléments au plus, les plus récents en tête ; un doublon remonte sans se répéter.
items = []
for number in range(30):
    items = add(items, text_item(f"élément {number}"))
assert len(items) == 25 and items[0]["text"] == "élément 29" and items[-1]["text"] == "élément 5"
items = add(items, text_item("élément 10"))
assert len(items) == 25 and items[0]["text"] == "élément 10" and [i["text"] for i in items].count("élément 10") == 1
# Épinglé : jamais poussé dehors, gardé par « Effacer tout », épingle conservée sur un doublon.
pinned_key = items[-1]["key"]
items = toggle_pin(items, pinned_key)
for number in range(40):
    items = add(items, text_item(f"nouveau {number}"))
assert any(i["key"] == pinned_key and i["pinned"] for i in items) and len(items) == 25
again = add(items, text_item(next(i["text"] for i in items if i["key"] == pinned_key)))
assert again[0]["key"] == pinned_key and again[0]["pinned"]
assert [i["key"] for i in clear(again)] == [pinned_key]
assert all(i["key"] != pinned_key for i in remove(again, pinned_key))
# Aperçu : trois lignes, lignes longues coupées.
assert module["preview"]("a\n\n b \nc\nd") == "a\nb\nc"
assert module["preview"]("x" * 100).endswith("…") and len(module["preview"]("x" * 100)) == 60
# Les gestionnaires de mots de passe sont respectés.
assert "x-kde-passwordManagerHint" in module["PASSWORD_HINTS"]
for key, row in module["_TR"].items():
    assert len(row) == 3 and all(row), key

# Commandes : réglage, ajout, épingle, effacement, fichiers privés.
with tempfile.TemporaryDirectory() as folder:
    env = dict(os.environ, XDG_STATE_HOME=f"{folder}/state", XDG_CONFIG_HOME=f"{folder}/config")

    def tool(*args):
        result = subprocess.run([sys.executable, "-I", str(SCRIPT), *args], env=env, capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, (args, result.stderr)
        return result.stdout

    assert "history=on" in tool("status")
    tool("add-text", "bonjour")
    tool("add-text", "deux\nlignes")
    tool("pin", "bonjour")
    assert tool("list").splitlines() == ["  deux\\nlignes", "* bonjour"]
    tool("clear")
    assert tool("list").splitlines() == ["* bonjour"]
    tool("set", "HISTORY", "off")
    assert "history=off" in tool("status")
    store = Path(folder) / "state/winlux/clipboard"
    assert (store.stat().st_mode & 0o777) == 0o700

# Branché : Win+V, démarrage avec la session, page Paramètres, image.
namespace = "{http://openbox.org/3.4/rc}"
rc = ET.parse(HERE.parent / "integration/native/config/openbox/rc.xml").getroot()
bindings = {item.get("key"): item.findtext(f"{namespace}action/{namespace}command") for item in rc.iter(f"{namespace}keybind")}
assert bindings.get("W-v") == "winlux-clipboard show", bindings
assert '"W-v"' in (HERE / "winlux-native-config.py").read_text()
assert "winlux-clipboard daemon" in (HERE / "winlux-user-session.sh").read_text()
assert "'winlux-clipboard.py:winlux-clipboard'" in (HERE / "stage-desktop-components.sh").read_text()
pages = (HERE / "winlux-settings-pages.py").read_text()
assert 'PAGE_CLIPBOARD: "page_clipboard"' in pages and "'Historique du presse-papiers':" in pages
print("historique du presse-papiers : OK")
