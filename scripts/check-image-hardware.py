#!/usr/bin/env python3
"""Vérifie que l'image finale porte les pilotes et firmwares essentiels.

    check-image-hardware.py RACINE VERSION [--essentials LISTE] [--modules /lib/modules]

Appelé par build-live-iso.sh sur la racine finale (après stage-apt-root.py) ;
sort en erreur si un élément de scripts/hardware-essentials.list manque. Un
module que le noyau de construction ne connaît pas du tout (renommé ou retiré)
est signalé sans échec. Un module présent doit aussi figurer dans
modules.dep, sinon modprobe ne le trouverait pas. Politique : docs/MATERIEL.md.
"""
import argparse
import fnmatch
import os
from pathlib import Path
import sys

SUFFIXES = (".ko.zst", ".ko.xz", ".ko.gz", ".ko")
FIRMWARE_SUFFIXES = ("", ".zst", ".xz")


def norm(name):
    return name.replace("-", "_")


def module_name(path):
    base = os.path.basename(path)
    for suffix in SUFFIXES:
        if base.endswith(suffix):
            return norm(base[: -len(suffix)])
    return None


def read_essentials(path):
    items = []
    for number, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        kind, _, value = line.partition(" ")
        value = value.strip()
        if kind not in ("module", "firmware") or not value:
            raise SystemExit(f"{path}:{number} : ligne invalide « {line} »")
        items.append((kind, value))
    return items


def names_in(listing):
    """Noms de modules d'un fichier modules.builtin / modules.dep."""
    names = set()
    if listing.is_file():
        for line in listing.read_text().splitlines():
            name = module_name(line.split(":", 1)[0].strip())
            if name:
                names.add(name)
    return names


def files_under(directory):
    found = set()
    if directory.is_dir():
        for current, _, files in os.walk(directory, followlinks=True):
            for file in files:
                name = module_name(file)
                if name:
                    found.add(name)
    return found


def firmware_files(root):
    base = root / "lib/firmware"
    if not base.exists():
        base = root / "usr/lib/firmware"
    files = []
    if base.is_dir():
        for current, _, names in os.walk(base, followlinks=True):
            rel = os.path.relpath(current, base)
            for name in names:
                files.append(name if rel == "." else f"{rel}/{name}")
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("root", type=Path)
    parser.add_argument("release")
    parser.add_argument("--essentials", type=Path, default=Path(__file__).with_name("hardware-essentials.list"))
    parser.add_argument("--modules", type=Path, default=Path(os.environ.get("WINLUX_HOST_MODULES", "/lib/modules")))
    args = parser.parse_args()
    image = args.root / "lib/modules" / args.release
    if not image.is_dir():
        image = args.root / "usr/lib/modules" / args.release
    host = args.modules / args.release
    builtin = names_in(image / "modules.builtin") | names_in(host / "modules.builtin")
    known = builtin | files_under(host / "kernel")
    shipped = files_under(image)
    indexed = names_in(image / "modules.dep")
    firmware = firmware_files(args.root)
    missing, unindexed, unknown = [], [], []
    for kind, value in read_essentials(args.essentials):
        if kind == "module":
            name = norm(value)
            if name in builtin:
                continue
            if name not in known:
                unknown.append(name)
            elif name not in shipped:
                missing.append(f"module {value}")
            elif name not in indexed:
                unindexed.append(value)
        else:
            patterns = [value + suffix for suffix in FIRMWARE_SUFFIXES]
            if not any(fnmatch.fnmatch(f, p) for f in firmware for p in patterns):
                missing.append(f"firmware {value}")
    if unknown:
        print("Matériel : inconnus du noyau " + args.release + " (renommés ?) : " + " ".join(unknown), file=sys.stderr)
    if unindexed:
        print("Matériel : modules absents de modules.dep (depmod non relancé) : " + " ".join(unindexed), file=sys.stderr)
    if missing:
        print("Matériel : éléments essentiels absents de l'image :", file=sys.stderr)
        for item in missing:
            print("  " + item, file=sys.stderr)
    if missing or unindexed:
        return 1
    print(f"Matériel : {len(shipped)} modules dans l'image, pilotes et firmwares essentiels présents")
    return 0


if __name__ == "__main__":
    sys.exit(main())
