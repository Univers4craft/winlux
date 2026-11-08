#!/usr/bin/env python3
"""Aucune clé en double dans les tables de traduction des scripts natifs.

Dans un dictionnaire Python, la dernière occurrence d'une clé remplace les
précédentes sans prévenir : « Images » traduit « Pictures » puis « Images »
perdait la première traduction. Vérifie chaque dictionnaire littéral de
scripts/ et integration/native/ dont les clés sont des chaînes.

Cohérence (#1039) : un nom de page ou de catégorie des Paramètres (PAGES et
CATEGORIES de winlux-settings.py, la barre latérale) a la même traduction
anglaise, espagnole et allemande dans tous les scripts qui le traduisent
(titres de page, recherche du menu Démarrer, assistant…).
"""

import ast
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def translation_tables(tree):
    """Tables _TR (français -> (anglais, espagnol, allemand)) d'un script."""
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(getattr(target, "id", "") == "_TR" for target in node.targets):
            try:
                table = ast.literal_eval(node.value)
            except ValueError:
                continue
            if isinstance(table, dict):
                yield table


def page_names(tree):
    """Noms français de PAGES et CATEGORIES (les _t("…") de ces deux listes)."""
    names = set()
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(getattr(target, "id", "") in ("PAGES", "CATEGORIES") for target in node.targets):
            for call in ast.walk(node.value):
                if (isinstance(call, ast.Call) and getattr(call.func, "id", "") == "_t" and call.args
                        and isinstance(call.args[0], ast.Constant)):
                    names.add(call.args[0].value)
    return names


def inconsistent_page_names(trees):
    """Noms de page traduits autrement qu'en barre latérale, script par script."""
    settings = trees[ROOT / "scripts/winlux-settings.py"]
    reference = {}
    for table in translation_tables(settings):
        reference.update(table)
    names = page_names(settings)
    problems = []
    for path, tree in trees.items():
        for table in translation_tables(tree):
            for name in sorted(names & table.keys()):
                if (name in reference and isinstance(table[name], tuple) and len(table[name]) == 3
                        and table[name] != reference[name]):
                    problems.append(f"{path.relative_to(ROOT)} : {name!r} traduit {table[name]} au lieu de "
                                    f"{reference[name]} (barre latérale des Paramètres, #1039)")
    return names, problems


def main():
    files = sorted((ROOT / "scripts").glob("*.py")) + sorted((ROOT / "integration/native").rglob("*.py"))
    problems = []
    trees = {}
    for path in files:
        try:
            tree = ast.parse(path.read_text(encoding="utf-8"))
        except SyntaxError as error:
            problems.append(f"{path.relative_to(ROOT)} : {error}")
            continue
        trees[path] = tree
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
    duplicates = len(problems)
    names, mismatches = inconsistent_page_names(trees)
    problems += mismatches
    for problem in problems:
        print("ÉCHEC", problem)
    print(f"{len(files)} fichiers, {duplicates} clé(s) en double")
    print(f"{len(names)} noms de page et de catégorie, {len(mismatches)} traduction(s) divergente(s) (#1039)")
    return 1 if problems or len(names) < 40 else 0


if __name__ == "__main__":
    sys.exit(main())
