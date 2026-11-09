#!/usr/bin/python3 -I
"""Retire le dépôt APT qu'un logiciel d'éditeur a déclaré (Store, #1056).

    winlux-store-repos clean <id>
    winlux-store-repos check <id>

« check » ne modifie rien : le Store le lance en administrateur juste après la
confirmation, pour que le Contrôle de compte d'utilisateur vienne avant le
téléchargement (#1451) ; l'autorisation vaut ensuite pour apt-get.

Lancé en administrateur (relais winlux-root) après la purge du paquet. Les
adresses autorisées viennent de /usr/share/winlux/store-editeurs.json (champ
« hosts ») : un fichier de /etc/apt/sources.list.d n'est supprimé que si
toutes ses sources pointent vers l'éditeur, et une clé (signed-by) seulement
si plus aucune source ne s'en sert. Refus tant que le paquet est installé."""
import json
import os
import re
import subprocess
import sys
import urllib.parse
from pathlib import Path

ROOT = os.environ.get("WINLUX_CONTROL_ROOT", "")
DEFS = Path(os.environ.get("WINLUX_STORE_EDITEURS", ROOT + "/usr/share/winlux/store-editeurs.json"))
SOURCES = Path(ROOT + "/etc/apt/sources.list.d")
KEY_DIRS = ("/usr/share/keyrings/", "/etc/apt/keyrings/", "/etc/apt/trusted.gpg.d/")


def sources_of(text, suffix):
    """(adresses, clés signed-by) d'un fichier .list ou .sources."""
    uris, keys = [], []
    if suffix == ".sources":
        for line in text.splitlines():
            key, _, value = line.partition(":")
            if key.strip().lower() == "uris":
                uris += value.split()
            elif key.strip().lower() == "signed-by" and value.strip().startswith("/"):
                keys.append(value.strip().split()[0])
        return uris, keys
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if not re.match(r"^deb(-src)?\s", line):
            continue
        rest = line.split(None, 1)[1]
        if rest.startswith("["):
            options, _, rest = rest[1:].partition("]")
            for option in options.split():
                if option.startswith("signed-by="):
                    keys += [k for k in option[10:].split(",") if k.startswith("/")]
        if rest.split():
            uris.append(rest.split()[0])
    return uris, keys


def installed(package):
    try:
        out = subprocess.run(["dpkg-query", "-W", "-f", "${db:Status-Abbrev}", package],
                             capture_output=True, text=True, timeout=30).stdout
    except (OSError, subprocess.SubprocessError):
        return False
    return out.startswith("ii")


def clean(ident):
    entry = next((e for e in json.loads(DEFS.read_text()).get("apps", []) if e.get("id") == ident), None)
    if entry is None:
        print(f"logiciel inconnu : {ident}", file=sys.stderr)
        return 2
    if installed(entry["package"]):
        print(f"{entry['package']} est encore installé : dépôt gardé", file=sys.stderr)
        return 3
    hosts = set(entry.get("hosts", []))
    files = sorted(p for p in SOURCES.glob("*") if p.suffix in (".list", ".sources") and p.is_file()) \
        if SOURCES.is_dir() else []
    removed, freed = [], set()
    for path in files:
        uris, keys = sources_of(path.read_text(errors="replace"), path.suffix)
        if uris and all((urllib.parse.urlparse(u).hostname or "") in hosts for u in uris):
            path.unlink()
            removed.append(str(path))
            freed.update(keys)
    still = set()
    for path in (p for p in files if p.exists()):
        still.update(sources_of(path.read_text(errors="replace"), path.suffix)[1])
    for key in sorted(freed - still):
        if key.startswith(KEY_DIRS) and ".." not in key.split("/"):
            target = Path(ROOT + key)
            if target.is_file():
                target.unlink()
                removed.append(str(target))
    for line in removed:
        print(f"retiré : {line}")
    return 0


def check(ident):
    known = any(e.get("id") == ident for e in json.loads(DEFS.read_text()).get("apps", []))
    if not known:
        print(f"logiciel inconnu : {ident}", file=sys.stderr)
        return 2
    return 0


def main(argv):
    if len(argv) == 2 and argv[0] == "clean" and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,40}", argv[1]):
        return clean(argv[1])
    if len(argv) == 2 and argv[0] == "check" and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,40}", argv[1]):
        return check(argv[1])
    print(__doc__, file=sys.stderr)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
