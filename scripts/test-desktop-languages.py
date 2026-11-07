#!/usr/bin/env python3
"""Les lanceurs .desktop livrés parlent les quatre langues de Winlux (#786).

Le menu Démarrer, « Ouvrir avec » et le Store affichent le `Name` (et le
`Comment`) du fichier .desktop dans la langue de la session : sans
`Name[en]`, `Name[de]` et `Name[es]`, une session anglaise, allemande ou
espagnole voyait « Paramètres » ou « Gestionnaire de tâches » en français.
Le français est la valeur par défaut (`Name=`). Les lanceurs cachés
(`NoDisplay=true`, `Hidden=true`) ne sont pas vérifiés.
"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LANGUAGES = ("en", "de", "es")


def entries(path):
    values, section = {}, None
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line.startswith("["):
            section = line
        elif section == "[Desktop Entry]" and "=" in line and not line.startswith("#"):
            key, value = line.split("=", 1)
            values[key.strip()] = value.strip()
    return values


def main():
    problems, checked = [], 0
    for path in sorted((ROOT / "integration").rglob("*.desktop")):
        values = entries(path)
        if values.get("NoDisplay", "").lower() == "true" or values.get("Hidden", "").lower() == "true":
            continue
        if "Name" not in values:
            continue
        checked += 1
        for key in ("Name", "Comment"):
            if key not in values:
                continue
            missing = [lang for lang in LANGUAGES if not values.get(f"{key}[{lang}]")]
            if missing:
                problems.append(f"{path.relative_to(ROOT)} : {key}[{', '.join(missing)}] absent ({key}={values[key]})")
    for problem in problems:
        print("ÉCHEC", problem)
    print(f"{checked} lanceurs vérifiés, {len(problems)} traduction(s) manquante(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
