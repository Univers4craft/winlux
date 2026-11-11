#!/usr/bin/python3
"""Reprend la dernière galerie « Suivi des travaux » déjà publiée (#1871).

    suivi-galerie-precedente.py DEPOT SORTIE [--archive FICHIER.zip]

Quand l'OCR ne peut pas tourner (miroirs Ubuntu injoignables, donc ni
tesseract ni Pillow), la galerie n'est pas reconstruite : on reprend le
dossier suivi/ du plus récent artefact github-pages de DEPOT qui en contient
un avec au moins une image. Ces images ont déjà passé la liste blanche et
l'OCR au moment de leur publication ; rien de nouveau n'est publié.

Les artefacts sont listés et téléchargés par « gh api » (GH_TOKEN). Seuls
les fichiers suivi/data.json, suivi/img/*.webp et suivi/min/*.webp sont
extraits, sans lien ni chemin qui sorte de SORTIE. Avec --archive, une
archive locale est lue à la place (tests). Code 0 si une galerie a été
reprise, 1 sinon. Bibliothèque standard seulement.
"""
import argparse
import io
import json
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import zipfile

PERMIS = re.compile(r"^suivi/(data\.json|(img|min)/[0-9a-f]{8,64}\.webp)$")
MAX_FICHIER = 16 * 1024 * 1024
MAX_ARTEFACTS = 20


def membres(tar):
    """Membres de l'archive Pages permis, avec leur chemin relatif à suivi/."""
    for info in tar.getmembers():
        nom = info.name[2:] if info.name.startswith("./") else info.name
        if not info.isfile() or not PERMIS.match(nom) or info.size > MAX_FICHIER:
            continue
        yield nom[len("suivi/"):], info


def reprendre(archive_zip, sortie):
    """Extrait suivi/ d'une archive github-pages (zip contenant artifact.tar)."""
    with zipfile.ZipFile(archive_zip) as z:
        noms = [n for n in z.namelist() if n.endswith(".tar")]
        if not noms:
            return 0
        donnees = z.read(noms[0])
    with tarfile.open(fileobj=io.BytesIO(donnees)) as tar:
        trouves = {rel: info for rel, info in membres(tar)}
        if "data.json" not in trouves:
            return 0
        data = json.loads(tar.extractfile(trouves["data.json"]).read().decode("utf-8"))
        images = sum(len(r.get("images", [])) for r in data.get("executions", []))
        if not images:
            return 0
        for rel, info in trouves.items():
            cible = sortie / rel
            cible.parent.mkdir(parents=True, exist_ok=True)
            cible.write_bytes(tar.extractfile(info).read())
        return images


def artefacts(depot):
    sortie = subprocess.run(
        ["gh", "api", f"repos/{depot}/actions/artifacts?name=github-pages&per_page={MAX_ARTEFACTS}"],
        capture_output=True, text=True, check=True).stdout
    liste = json.loads(sortie).get("artifacts", [])
    liste = [a for a in liste if not a.get("expired")
             and a.get("workflow_run", {}).get("head_branch") == "master"
             and a.get("workflow_run", {}).get("head_repository_id") == a.get("workflow_run", {}).get("repository_id")]
    # Les galeries complètes pèsent plusieurs Mo ; les sites sans galerie ~0,5 Mo.
    return sorted((a for a in liste if a.get("size_in_bytes", 0) > 2_000_000),
                  key=lambda a: a.get("created_at", ""), reverse=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("depot")
    parser.add_argument("sortie", type=Path)
    parser.add_argument("--archive", type=Path, help="archive github-pages locale (tests)")
    args = parser.parse_args()
    args.sortie.mkdir(parents=True, exist_ok=True)
    if args.archive:
        try:
            images = reprendre(args.archive, args.sortie)
        except (OSError, zipfile.BadZipFile, tarfile.TarError, ValueError):
            images = 0
        print(f"galerie reprise de {args.archive} : {images} image(s)" if images else "aucune galerie dans l'archive")
        return 0 if images else 1
    try:
        candidats = artefacts(args.depot)
    except (subprocess.SubprocessError, OSError, ValueError) as erreur:
        print(f"::error::artefacts github-pages illisibles : {erreur}")
        return 1
    for art in candidats:
        with tempfile.TemporaryDirectory() as tmp:
            zip_path = Path(tmp) / "pages.zip"
            try:
                with open(zip_path, "wb") as fichier:
                    subprocess.run(["gh", "api", f"repos/{args.depot}/actions/artifacts/{art['id']}/zip"],
                                   stdout=fichier, check=True, timeout=300)
                images = reprendre(zip_path, args.sortie)
            except (subprocess.SubprocessError, OSError, zipfile.BadZipFile, tarfile.TarError, ValueError) as erreur:
                print(f"artefact {art['id']} illisible : {erreur}")
                continue
        if images:
            run = art.get("workflow_run", {}).get("id")
            print(f"::warning::OCR indisponible : galerie reprise de la publication {run} "
                  f"({art.get('created_at')}, {images} image(s)), rien de nouveau publié (#1871)")
            return 0
    print("::error::aucune galerie publiée à reprendre (#1871)")
    return 1


if __name__ == "__main__":
    sys.exit(main())
