#!/usr/bin/env python3
"""Vérifie que Winlux fournit chaque fonction système importée par les vrais
jeux testés (docs/jeux/imports-jeux.tsv, voir docs/VRAIS-JEUX.md).

Usage : python3 scripts/test-games-imports.py [WINLUX]

WINLUX est le runtime compilé (target/release/winlux par défaut) : sa liste
`--exports` est comparée au manifeste avec les mêmes alias de DLL que le
chargeur (scripts/compat-imports.py). Sans runtime compilé, le test vérifie
seulement le manifeste et l'indique.
"""

import collections
import importlib.util
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "docs", "jeux", "imports-jeux.tsv")


def load_compat():
    spec = importlib.util.spec_from_file_location(
        "compat_imports", os.path.join(ROOT, "scripts", "compat-imports.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def main():
    rows = []
    with open(MANIFEST, encoding="utf-8") as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            game, dll, func = line.rstrip("\n").split("\t")
            rows.append((game, dll, func))
    games = sorted({g for g, _, _ in rows})
    if len(rows) < 1000 or len(games) < 2:
        print(f"manifeste incomplet : {len(rows)} imports, {len(games)} jeux")
        return 1
    runtime = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "winlux")
    if not os.access(runtime, os.X_OK):
        print(f"runtime absent ({runtime}) : manifeste lu ({len(rows)} imports), comparaison sautée")
        return 0
    out = subprocess.run([runtime, "--exports"], capture_output=True, text=True, check=True).stdout
    exports = collections.defaultdict(set)
    for line in out.splitlines():
        dll, _, name = line.partition("\t")
        exports[dll].add(name)
    compat = load_compat()
    missing = collections.defaultdict(list)
    for game, dll, func in rows:
        n = compat.normalize(dll)
        if n not in exports:
            missing[game].append(f"[{dll}]")
        elif not compat.lookup(exports, n, func):
            missing[game].append(f"{n}!{func}")
    for game in games:
        lst = sorted(set(missing.get(game, [])))
        print(f"{game} : {'tout est fourni' if not lst else str(len(lst)) + ' manquant(s)'}")
        for m in lst:
            print(f"  {m}")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
