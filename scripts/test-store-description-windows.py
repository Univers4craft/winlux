#!/usr/bin/env python3
"""#2892 : la fiche d'un programme Windows a toujours une description.

Le format de catalogue (docs/STORE.md) rend `description` facultatif à côté
de `summary`. Sans description, le résumé la remplace : la fiche n'affiche
plus de section « Description » vide. Une vraie description reste prioritaire."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

project = Path(__file__).resolve().parent.parent
store = project / "scripts/winlux-store.py"
failures = []


def ok(cond, label):
    print(("OK    " if cond else "ÉCHEC ") + label)
    if not cond:
        failures.append(label)


with tempfile.TemporaryDirectory() as tmp:
    w = Path(tmp)
    home, bindir, run = w / "home", w / "bin", w / "run"
    for d in (home, bindir, run):
        d.mkdir()
    installer = w / "setup.exe"
    installer.write_bytes(b"MZ")
    inst = {"url": installer.as_uri(), "sha256": hashlib.sha256(b"MZ").hexdigest()}
    catalog = w / "windows.json"
    catalog.write_text(json.dumps({"apps": [
        {"id": "resume", "name": "Éditeur", "version": "1", "summary": "Éditeur de texte et de code.", "installer": inst},
        {"id": "complet", "name": "Lecteur", "version": "1", "summary": "Lecteur",
         "description": ["Premier paragraphe.", "Second paragraphe."], "installer": inst},
        {"id": "vide", "name": "Outil", "version": "1", "summary": "Outil pratique.", "description": "",
         "installer": inst}]}))
    conf = home / ".config/winlux/store-sources.d"
    conf.mkdir(parents=True)
    (conf / "local.json").write_text(json.dumps({"id": "local", "name": "Catalogue local", "file": str(catalog)}))
    (bindir / "winlux").write_text("#!/bin/sh\nexit 0\n")
    (bindir / "winlux").chmod(0o755)
    env = {"PATH": f"{bindir}:/usr/bin:/bin", "HOME": str(home), "WINLUX_DRIVE_C": str(home),
           "XDG_DATA_HOME": str(home / ".local/share"), "XDG_CACHE_HOME": str(home / ".cache"),
           "XDG_CONFIG_HOME": str(home / ".config"), "WINLUX_STORE_RUN": str(run),
           "WINLUX_STORE_OFFLINE": "1", "WINLUX_CONTROL_ROOT": str(w / "root"), "WINLUX_LANG": "fr"}

    def desc(ref):
        subprocess.run([sys.executable, "-I", "-B", str(store), "details", f"windows:local/{ref}"], env=env,
                       capture_output=True, text=True, timeout=120)
        lines = (run / "store-detail.txt").read_text().splitlines()
        return [l[5:] for l in lines if l.startswith("desc=")]

    subprocess.run([sys.executable, "-I", "-B", str(store), "refresh"], env=env, capture_output=True, timeout=120)
    ok(desc("resume") == ["Éditeur de texte et de code."], f"résumé seul : il sert de description ({desc('resume')})")
    ok(desc("complet") == ["Premier paragraphe.", "Second paragraphe."],
       f"vraie description : gardée telle quelle ({desc('complet')})")
    ok(desc("vide") == ["Outil pratique."], f"description vide : remplacée par le résumé ({desc('vide')})")
    shipped = json.loads((project / "integration/store-windows-valides.json").read_text(encoding="utf-8"))
    ok(all(a.get("summary") or a.get("description") for a in shipped["apps"]),
       "chaque fiche de « Testés sur Winlux » a un texte à montrer")

if failures:
    sys.exit(f"{len(failures)} échec(s)")
print("Boutique : fiche d'un programme Windows jamais sans description (#2892)")
