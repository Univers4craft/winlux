#!/usr/bin/env python3
"""Aucune clé en double dans les tables de traduction des scripts natifs.

Dans un dictionnaire Python, la dernière occurrence d'une clé remplace les
précédentes sans prévenir : « Images » traduit « Pictures » puis « Images »
perdait la première traduction. Vérifie chaque dictionnaire littéral de
scripts/ et integration/native/ dont les clés sont des chaînes.
"""

import ast
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    files = sorted((ROOT / "scripts").glob("*.py")) + sorted((ROOT / "integration/native").rglob("*.py"))
    problems = []
    for path in files:
        try:
            tree = ast.parse(path.read_text(encoding="utf-8"))
        except SyntaxError as error:
            problems.append(f"{path.relative_to(ROOT)} : {error}")
            continue
        for node in ast.walk(tree):
            if not isinstance(node, ast.Dict):
                continue
            seen = {}
            for key in node.keys:
                if isinstance(key, ast.Constant) and isinstance(key.value, str):
                    if key.value in seen:
                        problems.append(f"{path.relative_to(ROOT)}:{key.lineno} : clé {key.value!r} déjà donnée l.{seen[key.value]}")
                    else:
                        seen[key.value] = key.lineno
    for problem in problems:
        print("ÉCHEC", problem)
    print(f"{len(files)} fichiers, {len(problems)} clé(s) en double")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
