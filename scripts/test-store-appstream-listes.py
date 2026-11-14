#!/usr/bin/env python3
"""#2185 : apt télécharge les fiches AppStream (DEP-11) des dépôts grâce à
50winlux-appstream, la Boutique les lit dans /var/lib/apt/lists, et un cache
vide écrit avant leur arrivée ne les masque pas (apt garde la date du
serveur, plus ancienne que le cache)."""
import gzip
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
store = project / "scripts/winlux-store.py"
conf = project / "integration/apt/50winlux-appstream"
failures = []

def ok(cond, label):
    print(("OK    " if cond else "ÉCHEC ") + label)
    if not cond:
        failures.append(label)

def mock(bindir, name, body):
    p = bindir / name
    p.write_text("#!/bin/sh\n" + body)
    p.chmod(0o755)

# Le réglage est lu par apt tel quel (apt-config, s'il est installé).
if shutil.which("apt-config"):
    dump = subprocess.run(["apt-config", "-c", str(conf), "dump"], capture_output=True, text=True).stdout
    ok('Acquire::IndexTargets::deb::DEP-11::MetaKey "$(COMPONENT)/dep11/Components-$(NATIVE_ARCHITECTURE).yml"' in dump
       and 'KeepCompressedAs "gz"' in dump, "50winlux-appstream lu par apt : cible DEP-11 des dépôts (#2185)")

YAML = """---
File: DEP-11
Origin: ubuntu-noble-universe
MediaBaseUrl: https://media.example/
---
Type: desktop-application
ID: org.example.Bidule.desktop
Package: bidule
Name:
  C: Bidule
Summary:
  C: A thing
  fr: Un bidule
Categories:
- Utility
"""

with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    root, home, bindir, run = w / "root", w / "home", w / "bin", w / "run"
    for d in (root, home, bindir, run):
        d.mkdir()
    lists = root / "var/lib/apt/lists"
    lists.mkdir(parents=True)
    (lists / "archive.ubuntu.com_ubuntu_dists_noble_universe_binary-amd64_Packages").write_text("")
    catalog = w / "catalog.txt"
    catalog.write_text("")
    mock(bindir, "dpkg-query", "exit 0\n")
    mock(bindir, "apt", "exit 0\n")
    mock(bindir, "apt-cache", 'case "$1" in policy) shift; for p in "$@"; do echo "$p:"; echo "  Candidate: 1.0"; done;; esac\n')
    mock(bindir, "apt-get", "exit 0\n")
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_CONTROL_ROOT": str(root),
           "XDG_CACHE_HOME": str(home / ".cache"), "XDG_DATA_HOME": str(home / ".local/share"),
           "WINLUX_STORE_RUN": str(run), "WINLUX_STORE_CATALOG": str(catalog), "WINLUX_STORE_OFFLINE": "1",
           "WINLUX_LANG": "fr", "WINLUX_STORE_CATALOGS_DEFS": str(w / "aucun.json")}

    def refresh():
        subprocess.run([sys.executable, "-I", "-B", str(store), "refresh"], env=env, capture_output=True,
                       text=True, timeout=300)
        return (run / "store.txt").read_text()

    # 1. Pas encore de fiches : cache vide écrit.
    txt = refresh()
    ok("app=apt:bidule|" not in txt, "sans fiches AppStream, pas de bidule")
    # 2. apt range les fiches, datées du serveur (2023, avant le cache).
    yml = lists / "archive.ubuntu.com_ubuntu_dists_noble_universe_dep11_Components-amd64.yml.gz"
    yml.write_bytes(gzip.compress(YAML.encode()))
    os.utime(yml, (1699750000, 1699750000))
    txt = refresh()
    line = next((l for l in txt.splitlines() if l.startswith("app=apt:bidule|")), "")
    ok(line != "", "fiche AppStream lue dans /var/lib/apt/lists, malgré un cache plus récent (#2185)")
    ok("Un bidule" in line, "résumé localisé de la fiche (#2185)")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique, fiches AppStream des listes apt : OK")
