#!/usr/bin/python3
"""Sélecteur d'emojis Win+. : données, recherche en quatre langues, récents, raccourcis."""
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "winlux-emoji.py"
DATA = HERE.parent / "integration/native/emoji/emoji.tsv"

# Données : un emoji par ligne, quatre langues renseignées, groupes connus.
lines = DATA.read_text(encoding="utf-8").splitlines()
assert len(lines) > 1500, len(lines)
groups = set()
for line in lines:
    fields = line.split("\t")
    assert len(fields) == 6 and all(fields), line
    groups.add(int(fields[1]))
assert groups == {0, 1, 3, 4, 5, 6, 7, 8, 9}, groups


def found(lang, query):
    result = subprocess.run([sys.executable, "-I", str(SCRIPT), "--search", query],
                            env=dict(os.environ, WINLUX_LANG=lang, WINLUX_EMOJI_DATA=str(DATA)),
                            capture_output=True, text=True, timeout=20)
    assert result.returncode == 0, result.stderr
    return result.stdout.split()


# Recherche dans la langue de l'interface, sans accents ni casse, et en anglais partout.
assert "😺" in found("fr", "chat") and "😺" in found("de", "Katze") and "😺" in found("es", "gato")
assert "🐱" in found("en", "cat") and "🐱" in found("de", "cat")
assert any(char.startswith("❤") for char in found("es", "CORAZON"))
assert "🎂" in found("fr", "gateau") and "🎂" in found("de", "geburtstag")
assert "🇫🇷" in found("fr", "drapeau france") and "🇩🇪" in found("de", "flagge deutschland")
assert found("fr", "zzzzqqq") == [] and found("fr", "   ") == []

module = runpy.run_path(str(SCRIPT), run_name="emoji_test")
fold = module["fold"]
assert fold("Gâteau") == "gateau" and fold("Straße") == "strasse" and fold("CŒUR") == "coeur"
# Récents : le dernier choisi en premier, sans doublon, 24 au plus.
with tempfile.TemporaryDirectory() as folder:
    state = Path(folder) / "winlux"
    module["recent_add"].__globals__["STATE"] = state
    module["recent_add"].__globals__["RECENT"] = state / "emoji-recent"
    for char in ["😀", "🎂", "😀"] + [chr(0x1F400 + n) for n in range(30)]:
        module["recent_add"](char)
    recent = module["recent_load"]()
    assert len(recent) == 24 and recent[0] == chr(0x1F400 + 29) and len(set(recent)) == 24, recent
    module["recent_add"]("😀")
    assert module["recent_load"]()[0] == "😀"
# Symboles et kaomojis : rien de vide, pas de doublon dans une catégorie.
for title, chars in module["SYMBOLS"]:
    assert title in module["_TR"], title
    items = chars.split()
    assert items and len(items) == len(set(items)), title
assert len(module["KAOMOJI"]) >= 30
# Tous les textes de l'interface sont traduits en anglais, espagnol et allemand.
for key, row in module["_TR"].items():
    assert len(row) == 3 and all(row), key
for _, _, title in module["GROUPS"]:
    assert title in module["_TR"], title

# Win+. et Win+; ouvrent le sélecteur, y compris pour un compte déjà créé (migration).
namespace = "{http://openbox.org/3.4/rc}"
rc = ET.parse(HERE.parent / "integration/native/config/openbox/rc.xml").getroot()
bindings = {item.get("key"): item.findtext(f"{namespace}action/{namespace}command") for item in rc.iter(f"{namespace}keybind")}
assert bindings.get("W-period") == "winlux-emoji" and bindings.get("W-semicolon") == "winlux-emoji", bindings
config = (HERE / "winlux-native-config.py").read_text()
assert '"W-period", "W-semicolon"' in config
assert "'winlux-emoji.py:winlux-emoji'" in (HERE / "stage-desktop-components.sh").read_text()
assert "emojibase-data" in (HERE.parent / "NOTICES").read_text()
print("sélecteur d'emojis : OK")
