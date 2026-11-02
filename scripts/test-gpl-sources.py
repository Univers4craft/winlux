#!/usr/bin/env python3
"""Liste des sources GPL de l'ISO (scripts/list-gpl-sources.py) et offre
écrite de NOTICES : un faux système avec une base dpkg suffit."""
import runpy
import subprocess
import sys
import tempfile
from pathlib import Path

project = Path(__file__).resolve().parent.parent
script = project / "scripts/list-gpl-sources.py"
tool = runpy.run_path(str(script))

with tempfile.TemporaryDirectory() as temp:
    root = Path(temp) / "racine"
    (root / "var/lib/dpkg").mkdir(parents=True)
    (root / "var/lib/dpkg/status").write_text("""\
Package: busybox-static
Status: install ok installed
Version: 1:1.37.0-6ubuntu1
Description: deux lignes
 suite de la description

Package: libavcodec61
Status: install ok installed
Source: ffmpeg (7:7.1.1-1ubuntu2)
Version: 7:7.1.1-1ubuntu2+b1

Package: ffmpeg
Status: install ok installed
Version: 7:7.1.1-1ubuntu2

Package: libc6
Status: install ok installed
Source: glibc
Version: 2.42-0ubuntu3

Package: retire
Status: deinstall ok config-files
Version: 1.0
""", encoding="utf-8")
    for package, text in {"busybox-static": "License: GPL-2\n",
                          "libavcodec61": "License: LGPL-2.1+\n and GPL-2+\n",
                          "libc6": "License: LGPL-2.1-or-later\n"}.items():
        (root / "usr/share/doc" / package).mkdir(parents=True)
        (root / "usr/share/doc" / package / "copyright").write_text(text, encoding="utf-8")

    rows = tool["collect"](root)
    by_source = {row[0]: row for row in rows}
    assert set(by_source) == {"busybox-static", "ffmpeg", "glibc"}, rows
    assert by_source["ffmpeg"][1] == "7:7.1.1-1ubuntu2", "version source tirée de « Source: nom (version) »"
    assert by_source["ffmpeg"][3] == "ffmpeg,libavcodec61", "binaires regroupés sous leur source"
    assert "GPL-2+" in by_source["ffmpeg"][2] and "LGPL-2.1+" in by_source["ffmpeg"][2], by_source["ffmpeg"]
    assert by_source["glibc"][1] == "2.42-0ubuntu3" and by_source["glibc"][2] == "LGPL-2.1-or-later"
    assert "retire" not in by_source, "paquet désinstallé listé"

    output = Path(temp) / "sources-gpl.tsv"
    result = subprocess.run([sys.executable, "-B", str(script), str(root), str(output)],
                            capture_output=True, text=True, check=True)
    assert "3 paquets sources, dont 3 sous licence copyleft" in result.stdout, result.stdout
    lines = output.read_text(encoding="utf-8").splitlines()
    assert lines[0].startswith("# Sources correspondantes") and len([l for l in lines if not l.startswith("#")]) == 3

# L'offre écrite de NOTICES doit exister et renvoyer à cette liste.
notices = (project / "NOTICES").read_text(encoding="utf-8")
assert "offre écrite" in notices and "trois ans" in notices and "sources-gpl.tsv" in notices, "offre écrite absente de NOTICES"
print("OK: liste des sources GPL de l'ISO (paquets sources, versions, licences) et offre écrite de NOTICES")
