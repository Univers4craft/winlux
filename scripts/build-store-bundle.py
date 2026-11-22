#!/usr/bin/python3
"""Paquet de la Boutique publié sur GitHub Pages (#3133).

    build-store-bundle.py paquet DOSSIER     # MANIFESTE + fichiers, à signer
    build-store-bundle.py livree FICHIER     # numéro de la copie livrée (ISO)

Le numéro est le nombre de commits de master en first-parent. Il ne fait
que croître tant que master n'est jamais réécrit, ce que la règle du projet
garantit (fusion de master, jamais de rebase ni de push forcé). Une date ne
conviendrait pas : une republication du même jour enfermerait les PC dans
une version. Un clone superficiel compterait trop peu de commits, et le
script le refuse donc.
"""
import hashlib
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Nom publié -> fichier du dépôt ; mêmes noms que NOMS dans winlux-store-lanceur.py.
FICHIERS = (
    ("winlux-store.py", "scripts/winlux-store.py"),
    ("winlux-store-confirm.py", "scripts/winlux-store-confirm.py"),
    ("store-windows-valides.json", "integration/store-windows-valides.json"),
    ("store-editeurs.json", "integration/store-editeurs.json"),
    ("store-catalog.txt", "integration/store-catalog.txt"),
)
LANCEUR = 1


def git(*args):
    return subprocess.run(["git", "-C", str(ROOT), *args], check=True, capture_output=True,
                          text=True).stdout.strip()


def numero():
    if git("rev-parse", "--is-shallow-repository") == "true":
        raise SystemExit("clone superficiel : le numéro de la Boutique serait faux (fetch-depth: 0)")
    return int(git("rev-list", "--count", "--first-parent", "HEAD"))


def paquet(sortie):
    sortie.mkdir(parents=True, exist_ok=True)
    n, commit = numero(), git("rev-parse", "HEAD")
    lignes = ["boutique=1", f"numero={n}", f"commit={commit}", f"lanceur={LANCEUR}"]
    for nom, source in FICHIERS:
        donnees = (ROOT / source).read_bytes()
        shutil.copyfile(ROOT / source, sortie / nom)
        lignes.append(f"fichier={hashlib.sha256(donnees).hexdigest()} {len(donnees)} {nom}")
    (sortie / "MANIFESTE").write_text("\n".join(lignes) + "\n", encoding="utf-8")
    print(f"Boutique {n} ({commit[:12]}) : {len(FICHIERS)} fichiers dans {sortie}")


def livree(fichier):
    """Numéro de la copie livrée. Hors d'un clone complet (CI de PR, archive),
    il vaut 0 : toute Boutique signée plus récente pourra alors la remplacer."""
    fichier.parent.mkdir(parents=True, exist_ok=True)
    try:
        n, commit = numero(), git("rev-parse", "HEAD")
    except (SystemExit, OSError, subprocess.CalledProcessError) as erreur:
        print(f"::warning::numéro de la Boutique livrée inconnu ({erreur}) : 0", file=sys.stderr)
        n, commit = 0, ""
    fichier.write_text(f"numero={n}\ncommit={commit}\n", encoding="utf-8")


def main(argv):
    if len(argv) == 3 and argv[1] == "paquet":
        paquet(Path(argv[2]))
        return 0
    if len(argv) == 3 and argv[1] == "livree":
        livree(Path(argv[2]))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
