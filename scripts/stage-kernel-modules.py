#!/usr/bin/env python3
"""Embarque dans l'image des dossiers entiers de pilotes du noyau.

    stage-kernel-modules.py RACINE VERSION LISTE [--modules /lib/modules] [--dry-run]

LISTE (scripts/hardware-module-dirs.list) nomme des chemins relatifs à
/lib/modules/VERSION/kernel ; « !chemin » en exclut une partie. Chaque module
retenu vient avec ses dépendances (modules.dep du noyau de construction), même
hors des dossiers listés. Ils restent compressés tels qu'Ubuntu les livre
(.ko.zst, lus par modprobe et udev) : 3 à 4 fois moins de place sur le disque
installé. Un module déjà présent, compressé ou non, n'est pas recopié.
Politique de support matériel : docs/MATERIEL.md.
"""
import argparse
import os
from pathlib import Path
import shutil
import sys

SUFFIXES = (".ko.zst", ".ko.xz", ".ko.gz", ".ko")


def bare(path):
    """Chemin sans extension de module : « drivers/md/raid1 »."""
    for suffix in SUFFIXES:
        if path.endswith(suffix):
            return path[: -len(suffix)]
    return path


def read_list(path):
    include, exclude = [], []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        target = exclude if line.startswith("!") else include
        target.append(bare(line.lstrip("!").strip().strip("/")))
    return include, exclude


def covered(relative, prefixes):
    """relative : « kernel/drivers/md/raid1.ko.zst » ; prefixes sans « kernel/ »."""
    path = bare(relative.removeprefix("kernel/"))
    return any(path == p or path.startswith(p + "/") for p in prefixes)


def dependencies(modules_dir):
    deps = {}
    for line in (modules_dir / "modules.dep").read_text().splitlines():
        if ":" not in line:
            continue
        module, rest = line.split(":", 1)
        deps[module.strip()] = rest.split()
    return deps


def select(modules_dir, include, exclude):
    deps = dependencies(modules_dir)
    chosen = {m for m in deps if covered(m, include) and not covered(m, exclude)}
    pending = list(chosen)
    while pending:
        for dep in deps.get(pending.pop(), []):
            if dep not in chosen:
                chosen.add(dep)
                pending.append(dep)
    return sorted(chosen)


def present(target_dir, relative):
    stem = target_dir / bare(relative)
    return any(stem.with_name(stem.name + suffix).exists() for suffix in SUFFIXES)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("stage", type=Path)
    parser.add_argument("release")
    parser.add_argument("list", type=Path)
    parser.add_argument("--modules", type=Path, default=Path(os.environ.get("WINLUX_HOST_MODULES", "/lib/modules")))
    parser.add_argument("--dry-run", action="store_true", help="compte sans copier")
    args = parser.parse_args()
    modules_dir = args.modules / args.release
    if not (modules_dir / "modules.dep").is_file():
        print(f"modules.dep absent : {modules_dir}", file=sys.stderr)
        return 1
    include, exclude = read_list(args.list)
    chosen = select(modules_dir, include, exclude)
    target_dir = args.stage / "lib/modules" / args.release
    added = already = size = 0
    for relative in chosen:
        source = modules_dir / relative
        if present(target_dir, relative):
            already += 1
            continue
        size += source.stat().st_size
        added += 1
        if not args.dry_run:
            destination = target_dir / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
    print(f"Pilotes du noyau : {added} modules ajoutés ({size / 2**20:.1f} Mio compressés), "
          f"{already} déjà présents, {len(chosen)} retenus par {args.list.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
