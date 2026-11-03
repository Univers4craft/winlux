#!/usr/bin/python3
"""Régénère integration/native/emoji/emoji.tsv (sélecteur d'emojis Win+.).

Source : paquet npm emojibase-data (licence MIT, annotations CLDR d'Unicode),
dossier décompressé passé en argument :

    npm pack emojibase-data && tar xzf emojibase-data-*.tgz
    python3 scripts/generate-emoji-data.py package

Une ligne par emoji jusqu'à Emoji 15.1 (les plus récents s'affichent en
carré vide avec les polices des distributions actuelles), sans variantes de
teinte ni composants : caractère,
numéro de groupe, puis pour en, fr, es et de le nom suivi de cinq mots-clés,
séparés par « | ».
"""
import json
from pathlib import Path
import sys

LANGS = ("en", "fr", "es", "de")
MAX_VERSION = 15.1


def main(folder):
    folder = Path(folder)
    data = {lang: {item["hexcode"]: item for item in json.loads((folder / lang / "compact.json").read_text(encoding="utf-8"))}
            for lang in LANGS}
    versions = {item["hexcode"]: item.get("version", 0)
                for item in json.loads((folder / "en" / "data.json").read_text(encoding="utf-8"))}
    base = [item for item in data["en"].values() if "group" in item and item["group"] != 2
            and versions.get(item["hexcode"], 99) <= MAX_VERSION]
    base.sort(key=lambda item: item.get("order", 0))
    rows = []
    for item in base:
        columns = [item["unicode"], str(item["group"])]
        for lang in LANGS:
            entry = data[lang].get(item["hexcode"], {})
            label = entry.get("label", "")
            words = [label] + [tag for tag in entry.get("tags", []) if tag != label][:5]
            columns.append("|".join(word.replace("\t", " ").replace("|", " ") for word in words if word))
        rows.append("\t".join(columns))
    target = Path(__file__).resolve().parent.parent / "integration/native/emoji/emoji.tsv"
    target.write_text("\n".join(rows) + "\n", encoding="utf-8")
    print(f"{len(rows)} emojis -> {target}")


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1]) if len(sys.argv) == 2 else 2)
