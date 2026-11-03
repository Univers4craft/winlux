#!/usr/bin/env python3
"""Liste les paquets sources de l'image, pour l'offre de code source GPL.

    python3 scripts/list-gpl-sources.py RACINE [SORTIE]

RACINE est la racine du système de l'ISO (celle qui contient
var/lib/dpkg/status). Le script écrit, une ligne par paquet source,
« source<TAB>version<TAB>licences repérées<TAB>paquets binaires », triée.
Avec SORTIE, il écrit aussi un en-tête qui rappelle l'offre écrite de
NOTICES. Le fichier se publie avec chaque version et se copie dans l'ISO
(/usr/share/doc/winlux/sources-gpl.tsv) : c'est la liste exacte des sources
correspondantes que la GPL oblige à fournir sur demande.

Aucune dépendance : le fichier status de dpkg est lu directement, sans dpkg.
Voir docs/RISQUES-JURIDIQUES.md, section 6.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

COPYLEFT = re.compile(r"\b(A?GPL|LGPL)(-[0-9]+(?:\.[0-9]+)*(?:\+|-or-later|-only)?)?(?![\w.-])")

HEADER = """\
# Sources correspondantes des logiciels sous GPL, LGPL et AGPL de cette image.
# Offre écrite : voir NOTICES (section « Code source »). Chaque ligne donne le
# paquet source Ubuntu et sa version exacte : « apt-get source PAQUET=VERSION ».
# source\tversion\tlicences\tpaquets binaires
"""


def paragraphs(text: str):
    for block in text.split("\n\n"):
        fields: dict[str, str] = {}
        key = None
        for line in block.splitlines():
            if line[:1] in (" ", "\t") and key:
                fields[key] += "\n" + line.strip()
            elif ":" in line:
                key, _, value = line.partition(":")
                fields[key] = value.strip()
        if fields.get("Package"):
            yield fields


def licences(root: Path, package: str) -> str:
    """Licences copyleft citées dans le fichier copyright du paquet (indicatif)."""
    copyright_file = root / "usr/share/doc" / package / "copyright"
    try:
        text = copyright_file.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return "?"
    found = sorted({m.group(1) + (m.group(2) or "") for m in COPYLEFT.finditer(text)})
    return ",".join(found) or "-"


def collect(root: Path) -> list[tuple[str, str, str, str]]:
    status = (root / "var/lib/dpkg/status").read_text(encoding="utf-8", errors="replace")
    sources: dict[tuple[str, str], dict] = {}
    for fields in paragraphs(status):
        if " installed" not in fields.get("Status", ""):
            continue
        package = fields["Package"]
        version = fields.get("Version", "")
        source = fields.get("Source", package)
        # « Source: nom (version) » quand la version source diffère.
        match = re.fullmatch(r"(\S+)\s*\((.+)\)", source)
        if match:
            source, version = match.group(1), match.group(2)
        entry = sources.setdefault((source, version), {"binaries": [], "licences": set()})
        entry["binaries"].append(package)
        for name in licences(root, package).split(","):
            if name not in ("", "-", "?"):
                entry["licences"].add(name)
    rows = []
    for (source, version), entry in sorted(sources.items()):
        rows.append((source, version, ",".join(sorted(entry["licences"])) or "-",
                     ",".join(sorted(entry["binaries"]))))
    return rows


def main(argv: list[str]) -> int:
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0 if argv else 2
    rows = collect(Path(argv[0]))
    text = "".join("\t".join(row) + "\n" for row in rows)
    if len(argv) > 1:
        Path(argv[1]).write_text(HEADER + text, encoding="utf-8")
        copyleft = sum(1 for row in rows if row[2] != "-")
        print(f"{len(rows)} paquets sources, dont {copyleft} sous licence copyleft : {argv[1]}")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
