#!/usr/bin/python3 -I
"""Sonde des dépôts d'éditeurs du Store (#1056), lancée par la CI (non bloquante).

Pour chaque entrée de integration/store-editeurs.json : version, empreinte et
taille que le Store proposerait aujourd'hui. Pour une entrée « deb » (fichier
épinglé), télécharge le fichier dans un dossier neuf et vide, donne l'adresse
finale, la version et l'empreinte à épingler, et les lignes de dépôt APT que
ses scripts de maintenance déclarent (lues comme du texte, jamais exécutées).

    check-store-editeurs.py [integration/store-editeurs.json]"""
import gzip
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import urllib.parse
from pathlib import Path


def curl(url, output):
    return subprocess.run(["curl", "--fail", "--location", "--silent", "--show-error", "--proto", "=https",
                           "--max-time", "900", "--max-filesize", "1073741824", "-w", "%{url_effective}",
                           "--output", str(output), url], capture_output=True, text=True, timeout=960)


def main(argv):
    path = Path(argv[0]) if argv else Path(__file__).resolve().parent.parent / "integration/store-editeurs.json"
    failures = 0
    for e in json.loads(path.read_text())["apps"]:
        with tempfile.TemporaryDirectory(prefix="editeur-sonde-") as work:
            work = Path(work)
            if e["kind"] == "index":
                r = curl(urllib.parse.urljoin(e["base"], e["index"]), work / "Packages")
                if r.returncode:
                    print(f"{e['id']}: index injoignable ({r.stderr.strip()})")
                    failures += 1
                    continue
                data = (work / "Packages").read_bytes()
                data = gzip.decompress(data) if data[:2] == b"\x1f\x8b" else data
                found = [dict(l.split(": ", 1) for l in s.splitlines() if ": " in l and not l.startswith(" "))
                         for s in data.decode("utf-8", "replace").split("\n\n")]
                found = [f for f in found if f.get("Package") == e["package"] and f.get("Architecture") in ("amd64", "all")]
                if not found:
                    print(f"{e['id']}: {e['package']} absent de l'index")
                    failures += 1
                    continue
                last = found[-1]
                print(f"{e['id']}: {len(found)} versions, dernière lue {last.get('Version')} "
                      f"Filename={last.get('Filename')} SHA256={last.get('SHA256')} Size={last.get('Size')} "
                      f"Depends={last.get('Depends', '')[:300]}")
                # Adresse du paquet : racine du dépôt, ou dossier de l'index pour un dépôt « plat ».
                roots = [e["base"]]
                if e.get("flat"):
                    roots.append(urllib.parse.urljoin(e["base"], e["index"].rsplit("/", 1)[0] + "/"))
                for root in roots:
                    url = urllib.parse.urljoin(root, last.get("Filename", "").removeprefix("./"))
                    head = subprocess.run(["curl", "--silent", "--head", "--location", "--proto", "=https", "--max-time", "60",
                                           "-o", "/dev/null", "-w", "%{http_code}", url], capture_output=True, text=True)
                    print(f"    {head.stdout.strip()} {url}")
                continue
            deb = work / "paquet.deb"
            r = curl(e["url"], deb)
            if r.returncode:
                print(f"{e['id']}: téléchargement impossible ({r.stderr.strip()})")
                failures += 1
                continue
            digest = hashlib.sha256(deb.read_bytes()).hexdigest()
            fields = subprocess.run(["dpkg-deb", "-f", str(deb), "Package", "Version"], capture_output=True, text=True).stdout
            print(f"{e['id']}: url finale {r.stdout.strip()} sha256={digest} size={deb.stat().st_size} "
                  + " ".join(fields.split()))
            control = work / "control"
            subprocess.run(["dpkg-deb", "-e", str(deb), str(control)], capture_output=True)
            for script in sorted(control.glob("*")) if control.is_dir() else []:
                for line in script.read_text(errors="replace").splitlines():
                    if re.search(r"https?://|signed-by|sources\.list|keyring|\.gpg|\.asc|Suites|Components|Types:", line):
                        print(f"    {script.name}: {line.strip()[:240]}")
    print(f"{failures} dépôt(s) en échec")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
