#!/usr/bin/python3 -I
"""Sonde des dépôts d'éditeurs du Store (#1056), lancée par la CI (non bloquante).

Pour chaque entrée de integration/store-editeurs.json : version, empreinte et
taille que le Store proposerait aujourd'hui. Pour une entrée « deb » (fichier
épinglé), télécharge le fichier dans un dossier neuf et vide, donne l'adresse
finale, la version et l'empreinte à épingler, et les lignes de dépôt APT que
ses scripts de maintenance déclarent (lues comme du texte, jamais exécutées).

Puis, pour les logiciels candidats (CANDIDATS ci-dessous), la dernière version
publiée, son adresse et son empreinte, à recopier dans le catalogue après relecture.

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
                # Adresse du paquet, comme le Store la calcule : relative à la racine du dépôt.
                url = urllib.parse.urljoin(e["base"], last.get("Filename", "").removeprefix("./"))
                head = subprocess.run(["curl", "--silent", "--head", "--location", "--proto", "=https", "--max-time", "60",
                                       "-o", "/dev/null", "-w", "%{http_code}", url], capture_output=True, text=True)
                print(f"    {head.stdout.strip()} {url}")
                failures += head.stdout.strip() != "200"
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
    candidats()
    return 0


# Logiciels proposés pour la boutique (10 nov.) : version et empreinte relevées
# ici, puisque seule la CI atteint les sites des éditeurs.
CANDIDATS = [
    {"id": "protonmail", "url": "https://proton.me/download/mail/linux/1.16.1/ProtonMail-desktop-beta.deb"},
    {"id": "warzone2100", "github": "Warzone2100/warzone2100", "tag": "4.", "asset": r"(?i)^warzone2100.*win.*x64.*\.exe$"},
]


def candidats():
    print("--- Candidats ---")
    for c in CANDIDATS:
        with tempfile.TemporaryDirectory(prefix="editeur-candidat-") as work:
            work = Path(work)
            try:
                if "github" in c:
                    r = curl(f"https://api.github.com/repos/{c['github']}/releases?per_page=40", work / "r.json")
                    if r.returncode:
                        print(f"{c['id']}: API injoignable ({r.stderr.strip()})")
                        continue
                    rels = [x for x in json.loads((work / "r.json").read_text())
                            if x["tag_name"].startswith(c["tag"]) and not x["prerelease"] and not x["draft"]]
                    assets = [a for a in (rels[0]["assets"] if rels else []) if re.match(c["asset"], a["name"])]
                    if not assets:
                        print(f"{c['id']}: aucun fichier trouvé ; {rels[0]['tag_name'] if rels else '?'} : "
                              + " ".join(a["name"] for a in (rels[0]["assets"] if rels else []))[:600])
                        continue
                    a = assets[0]
                    deb = work / "paquet.deb"
                    r = curl(a["browser_download_url"], deb)
                    if r.returncode:
                        print(f"{c['id']}: téléchargement impossible ({r.stderr.strip()})")
                        continue
                    fields = subprocess.run(["dpkg-deb", "-f", str(deb), "Package", "Version", "Depends"],
                                            capture_output=True, text=True).stdout if a["name"].endswith(".deb") else ""
                    print(f"{c['id']}: {rels[0]['tag_name']} url={a['browser_download_url']} "
                          f"finale={urllib.parse.urlparse(r.stdout.strip()).hostname} "
                          f"sha256={hashlib.sha256(deb.read_bytes()).hexdigest()} api={a.get('digest')} "
                          f"size={deb.stat().st_size} " + " ".join(fields.split())[:400])
                elif "index" in c:
                    r = curl(urllib.parse.urljoin(c["base"], c["index"]), work / "Packages")
                    if r.returncode:
                        print(f"{c['id']}: index injoignable ({r.stderr.strip()})")
                        continue
                    data = (work / "Packages").read_text(errors="replace")
                    names = sorted({m for m in re.findall(r"^Package: (\S+)", data, re.M)})
                    blocks = [b for b in data.split("\n\n") if re.search(rf"^Package: {re.escape(c['package'])}$", b, re.M)]
                    print(f"{c['id']}: paquets de l'index : {' '.join(names)[:400]}")
                    if blocks:
                        last = dict(l.split(": ", 1) for l in blocks[-1].splitlines() if ": " in l and not l.startswith(" "))
                        print(f"    dernière {last.get('Version')} Filename={last.get('Filename')} "
                              f"SHA256={last.get('SHA256')} Size={last.get('Size')}")
                elif "page" in c:
                    r = curl(c["page"], work / "page.html")
                    links = sorted(set(re.findall(c["link"], (work / "page.html").read_text(errors="replace")))) \
                        if not r.returncode else []
                    print(f"{c['id']}: liens {links[:5]}")
                    for link in links[:1]:
                        f = work / "fichier"
                        url = urllib.parse.urljoin(c["page"], link)
                        r = curl(url, f)
                        if not r.returncode:
                            print(f"    url={url} sha256={hashlib.sha256(f.read_bytes()).hexdigest()} size={f.stat().st_size}")
                elif "url" in c:
                    f = work / "fichier"
                    r = curl(c["url"], f)
                    if not r.returncode and c["url"].endswith(".deb"):
                        print("    " + " ".join(subprocess.run(["dpkg-deb", "-f", str(f), "Package", "Version", "Depends"],
                                                               capture_output=True, text=True).stdout.split())[:400])
                    print(f"{c['id']}: " + (f"url={c['url']} finale={urllib.parse.urlparse(r.stdout.strip()).hostname} "
                                            f"sha256={hashlib.sha256(f.read_bytes()).hexdigest()} size={f.stat().st_size}"
                                            if not r.returncode else f"injoignable ({r.stderr.strip()})"))
                else:
                    r = curl(c["json"], work / "v.json")
                    print(f"{c['id']}: " + ((work / "v.json").read_text(errors="replace")[:1500] if not r.returncode
                                            else f"injoignable ({r.stderr.strip()})"))
            except Exception as error:  # sonde informative : on continue
                print(f"{c['id']}: erreur {error!r}")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
