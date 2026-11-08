#!/usr/bin/env python3
"""Notices et licences embarquées dans le paquet winlux-desktop (#33).

    python3 scripts/stage-notices.py RACINE      # pose les fichiers
    python3 scripts/stage-notices.py --verifier  # NOTICES couvre-t-il tout ?

Dans RACINE (la racine de l'image en construction), le script écrit
usr/share/doc/winlux-desktop/ :
- copyright : fichier au format Debian (DEP-5) du paquet, qui renvoie à la
  GPL-3.0-or-later de Winlux et à chaque bibliothèque Rust compilée dedans ;
- LICENSE et NOTICES du dépôt ;
- crates/<nom>-<version>/ : les fichiers de licence de chaque bibliothèque
  Rust (MIT et Apache exigent de livrer le texte et la mention de copyright).

La liste des bibliothèques vient de `cargo metadata` : toutes celles que le
binaire Linux `winlux` compile (dépendances normales et de construction).
Avec --verifier, le script échoue si NOTICES oublie l'une d'elles ou donne
une autre licence que son Cargo.toml.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

PROJECT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-unknown-linux-gnu"
LICENCE_PREFIXES = ("LICEN", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE")


def metadata(project: Path = PROJECT) -> dict:
    command = ["cargo", "metadata", "--format-version", "1", "--filter-platform", TARGET]
    for extra in (["--offline"], []):
        result = subprocess.run(command + extra, cwd=project, capture_output=True, text=True)
        if result.returncode == 0:
            return json.loads(result.stdout)
    raise SystemExit(f"cargo metadata a échoué :\n{result.stderr}")


def crates(meta: dict, root_name: str = "winlux") -> list[dict]:
    """Bibliothèques tierces compilées dans `root_name`, triées par nom."""
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == root_name)
    seen, todo = set(), [root]
    while todo:
        current = todo.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            if any(kind["kind"] in (None, "build") for kind in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    found = [packages[i] for i in seen if packages[i]["source"]]
    return sorted(found, key=lambda p: (p["name"], p["version"]))


def licence_files(package: dict) -> list[Path]:
    folder = Path(package["manifest_path"]).parent
    return sorted(f for f in folder.iterdir()
                  if f.is_file() and f.name.upper().startswith(LICENCE_PREFIXES))


def missing_from_notices(notices: str, packages: list[dict]) -> list[str]:
    """Bibliothèques absentes de NOTICES ou citées avec une autre licence."""
    lines = notices.splitlines()
    problems = []
    for package in packages:
        name, licence = package["name"], package["license"] or "?"
        entries = [l for l in lines if l.split() and l.split()[0] == name]
        if not entries:
            problems.append(f"{name} {package['version']} ({licence}) absent de NOTICES")
        elif not any(licence in l for l in entries):
            problems.append(f"{name} : NOTICES ne cite pas sa licence « {licence} »")
    return sorted(set(problems))


def copyright_file(packages: list[dict], version: str) -> str:
    parts = [
        "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n"
        f"Upstream-Name: Winlux\nSource: https://github.com/Univers4craft/winlux\n"
        f"Comment: winlux-desktop {version}. Provenance détaillée : NOTICES ; liste des\n"
        " sources des paquets sous GPL de l'image : /usr/share/doc/winlux/sources-gpl.tsv.\n",
        "Files: *\nCopyright: Winlux contributors\nLicense: GPL-3.0-or-later\n"
        " Le texte complet est dans LICENSE (et /usr/share/common-licenses/GPL-3).\n",
    ]
    for package in packages:
        folder = f"usr/share/doc/winlux-desktop/crates/{package['name']}-{package['version']}"
        parts.append(
            f"Files: {folder}/*\n"
            f"Copyright: {', '.join(package.get('authors') or []) or 'voir les fichiers du dossier'}\n"
            f"License: {package['license'] or '?'}\n"
            f"Comment: bibliothèque Rust {package['name']} {package['version']}, compilée dans\n"
            f" /usr/bin/winlux ; textes de licence dans ce dossier.\n")
    return "\n".join(parts)


def stage(root: Path, meta: dict, project: Path = PROJECT) -> Path:
    packages = crates(meta)
    problems = missing_from_notices((project / "NOTICES").read_text(encoding="utf-8"), packages)
    if problems:
        raise SystemExit("NOTICES incomplet :\n  " + "\n  ".join(problems))
    doc = root / "usr/share/doc/winlux-desktop"
    if doc.exists():
        shutil.rmtree(doc)
    (doc / "crates").mkdir(parents=True)
    shutil.copy(project / "LICENSE", doc / "LICENSE")
    shutil.copy(project / "NOTICES", doc / "NOTICES")
    version = (project / "VERSION").read_text().strip() if (project / "VERSION").exists() else "?"
    (doc / "copyright").write_text(copyright_file(packages, version), encoding="utf-8")
    for package in packages:
        files = licence_files(package)
        if not files:
            raise SystemExit(f"{package['name']} {package['version']} : aucun fichier de licence trouvé")
        target = doc / "crates" / f"{package['name']}-{package['version']}"
        target.mkdir()
        for licence in files:
            shutil.copy(licence, target / licence.name)
    return doc


def main(argv: list[str]) -> int:
    if argv[1:] == ["--verifier"]:
        packages = crates(metadata())
        problems = missing_from_notices((PROJECT / "NOTICES").read_text(encoding="utf-8"), packages)
        for problem in problems:
            print("ÉCHEC " + problem)
        if problems:
            return 1
        print(f"OK : NOTICES couvre les {len(packages)} bibliothèques Rust compilées dans winlux")
        return 0
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    doc = stage(Path(argv[1]), metadata())
    print(f"Notices et licences posées dans {doc}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
