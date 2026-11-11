#!/usr/bin/python3
"""Galerie « Suivi des travaux » du site : captures de la CI (#1779).

    suivi-travaux.py telecharger DEPOT TRAVAIL [--nombre 30]
    suivi-travaux.py construire TRAVAIL SORTIE [--exiger-ocr]

« telecharger » (dans site.yml, avec gh et GH_TOKEN) prend les NOMBRE dernières
exécutions terminées du workflow CI qui ont gardé l'artefact captures-ui, sur
master et sur les PR ouvertes depuis CE dépôt (jamais un fork : n'importe qui
pourrait sinon publier une image sur le site). Pour chacune, TRAVAIL/<run>/
reçoit les PNG et meta.json, rempli par l'API GitHub, jamais par l'artefact.

« construire » écrit SORTIE/data.json, SORTIE/img/<empreinte>.webp (image
entière) et SORTIE/min/<empreinte>.webp (vignette de 480 px). Filtres, dans
cet ordre, tous fermés par défaut :
  1. liste blanche site/suivi-captures.tsv : nom exact, sinon l'image reste
     privée (dans l'artefact de la CI) ;
  2. fichier PNG valide, au plus 16 Mo et 40 mégapixels ;
  3. lecture OCR (tesseract) : refusée si une marque Microsoft ou un nom de
     la machine du runner (Azure, Hyper-V…) apparaît ; sans tesseract, plus
     rien n'est publié avec --exiger-ocr ;
  4. image décodée puis ré-encodée par Pillow (rien d'autre que les pixels).
Une image identique d'une exécution à l'autre n'est stockée qu'une fois
(empreinte SHA-256 des pixels). Le résumé liste ce qui a été écarté.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
LISTE = ROOT / "site" / "suivi-captures.tsv"
LANGS = ("fr", "en", "de", "es")
MAX_BYTES = 16 * 1024 * 1024
MAX_PIXELS = 40_000_000
THUMB_WIDTH = 480
OCR_DELAI = 90
# Mots refusés à la lecture OCR. « Windows » seul reste permis : il décrit les
# raccourcis et les programmes, comme le texte du site.
INTERDIT = re.compile(
    r"micro\s*soft|\bmsft\b|\bazure\b|hyper\s*-?\s*v\b|\bedge\b|\bbing\b|\bxbox\b|"
    r"\boffice\b|\boutlook\b|\bteams\b|one\s*drive|copilot|\bmsn\b|visual\s+studio",
    re.I)


def gh(*args):
    return subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout


def liste_blanche(path=LISTE):
    """{nom: {"test": ..., "fr": ..., "en": ..., "de": ..., "es": ...}}"""
    entries = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        cells = line.split("\t")
        if len(cells) != 6 or not all(c.strip() for c in cells):
            raise SystemExit(f"{path.name}:{number} : six colonnes non vides attendues")
        name, test, *captions = cells
        if not re.fullmatch(r"[a-z0-9-]+", name) or name in entries:
            raise SystemExit(f"{path.name}:{number} : nom invalide ou en double « {name} »")
        entries[name] = dict(test=test, **dict(zip(LANGS, captions)))
    return entries


# -- téléchargement ---------------------------------------------------------

def telecharger(depot, travail, nombre):
    travail.mkdir(parents=True, exist_ok=True)
    pris, page = 0, 1
    while pris < nombre and page <= 10:
        runs = json.loads(gh("api", f"repos/{depot}/actions/workflows/ci.yml/runs?status=completed&per_page=50&page={page}"))
        page += 1
        if not runs.get("workflow_runs"):
            break
        for run in runs["workflow_runs"]:
            if pris >= nombre:
                break
            head = (run.get("head_repository") or {}).get("full_name")
            if head != depot or run.get("event") not in ("push", "pull_request", "workflow_dispatch"):
                continue
            arts = json.loads(gh("api", f"repos/{depot}/actions/runs/{run['id']}/artifacts?name=captures-ui"))
            if not any(a["name"] == "captures-ui" and not a["expired"] for a in arts.get("artifacts", [])):
                continue
            cible = travail / str(run["id"])
            if cible.exists():
                shutil.rmtree(cible)
            images = cible / "images"
            try:
                gh("run", "download", str(run["id"]), "-R", depot, "-n", "captures-ui", "-D", str(images))
            except subprocess.CalledProcessError as exc:
                print(f"exécution {run['id']} : téléchargement refusé ({exc.stderr.strip()[:200]})")
                shutil.rmtree(cible, ignore_errors=True)
                continue
            prs = [p["number"] for p in run.get("pull_requests") or []]
            meta = {"run": run["id"], "numero": run.get("run_number"), "url": run["html_url"],
                    "commit": run["head_sha"], "branche": run.get("head_branch"), "evenement": run["event"],
                    "pr": prs[0] if prs else None, "titre": run.get("display_title") or "",
                    "date": run.get("run_started_at") or run["created_at"], "conclusion": run.get("conclusion")}
            (cible / "meta.json").write_text(json.dumps(meta, ensure_ascii=False, indent=1), encoding="utf-8")
            pris += 1
            print(f"exécution {run['id']} ({run['head_sha'][:7]}, {run['event']}) prise")
    print(f"{pris} exécution(s) avec des captures dans {travail}")
    return 0


# -- construction -----------------------------------------------------------

def ocr_disponible():
    return shutil.which("tesseract") is not None


def texte_ocr(image):
    """Texte lu par tesseract en niveaux de gris, puis inversé (les thèmes sombres
    se lisent mieux en noir sur blanc). L'image est ramenée à 1920 px de large au
    plus et n'est agrandie que sous 1600 px : une 4K agrandie ou un 1080p doublé
    dépassaient deux minutes (#1829). Un fil OpenMP par tesseract, puisque
    plusieurs tournent déjà en parallèle."""
    from PIL import ImageOps
    gris = image.convert("L")
    if gris.width > 1920:
        gris = gris.resize((1920, round(gris.height * 1920 / gris.width)))
    elif gris.width < 1600:
        gris = gris.resize((gris.width * 2, gris.height * 2))
    env = dict(os.environ, OMP_THREAD_LIMIT="1")
    textes = []
    for variante in (gris, ImageOps.invert(gris)):
        with tempfile.NamedTemporaryFile(suffix=".png") as tmp:
            variante.save(tmp.name)
            sortie = subprocess.run(["tesseract", tmp.name, "-", "-l", langues_ocr(), "--psm", "11"],
                                    capture_output=True, text=True, timeout=OCR_DELAI, env=env, check=True)
            textes.append(sortie.stdout)
    return "\n".join(textes)


_LANGUES = None


def langues_ocr():
    global _LANGUES
    if _LANGUES is None:
        try:
            dispo = subprocess.run(["tesseract", "--list-langs"], capture_output=True, text=True).stdout.split()
        except OSError:
            dispo = []
        _LANGUES = "+".join(l for l in ("eng", "fra", "deu", "spa") if l in dispo) or "eng"
    return _LANGUES


def png_valide(path):
    if path.stat().st_size > MAX_BYTES:
        return False
    with open(path, "rb") as handle:
        return handle.read(8) == b"\x89PNG\r\n\x1a\n"


def ouvrir(png):
    from PIL import Image
    with Image.open(png) as source:
        source.load()
        return source.convert("RGB")


def empreinte_de(image):
    return hashlib.sha256(image.tobytes() + repr(image.size).encode()).hexdigest()[:24]


def verdict_ocr(png):
    """(publiable, raison). Une image que l'OCR ne relit pas est refusée (#1829)."""
    try:
        texte = texte_ocr(ouvrir(png))
    except (subprocess.SubprocessError, OSError) as exc:
        return (False, f"OCR impossible ({type(exc).__name__})")
    trouve = INTERDIT.search(texte)
    return (trouve is None, f"OCR : « {trouve.group(0)} »" if trouve else "")


def construire(travail, sortie, exiger_ocr, liste=LISTE, cache=None, travaux=4):
    from PIL import Image
    Image.MAX_IMAGE_PIXELS = MAX_PIXELS
    blanche = liste_blanche(liste)
    ocr = ocr_disponible()
    if exiger_ocr and not ocr:
        print("::error::tesseract absent : aucune capture publiée (#1779)")
    # Verdicts OCR déjà rendus (actions/cache) : seules les images nouvelles
    # sont relues. Ils ne valent que pour le même motif INTERDIT.
    connus = {}
    if cache and cache.is_file():
        try:
            stocke = json.loads(cache.read_text(encoding="utf-8"))
            if stocke.get("motif") == INTERDIT.pattern:
                connus = {k: tuple(v) for k, v in stocke.get("verdicts", {}).items()}
        except ValueError:
            connus = {}
    if sortie.exists():
        shutil.rmtree(sortie)
    (sortie / "img").mkdir(parents=True)
    (sortie / "min").mkdir()
    privees, refusees = set(), {}
    metas = []
    for meta_path in travail.glob("*/meta.json"):
        try:
            metas.append((json.loads(meta_path.read_text(encoding="utf-8")), meta_path.parent))
        except ValueError:
            continue
    metas.sort(key=lambda m: m[0].get("date", ""), reverse=True)
    # 1. Liste blanche, PNG valide, empreinte des pixels.
    retenues = []  # (meta, [(nom, empreinte, png)])
    source_de = {}  # empreinte -> premier PNG
    tailles = {}
    for meta, dossier in metas:
        lot = []
        for png in sorted((dossier / "images").glob("*.png")):
            name = png.stem
            if name not in blanche:
                privees.add(name)
                continue
            if not png_valide(png):
                refusees[name] = "fichier PNG invalide ou trop lourd"
                continue
            try:
                image = ouvrir(png)
            except Exception as exc:  # noqa: BLE001 - toute image illisible est refusée
                refusees[name] = f"illisible ({type(exc).__name__})"
                continue
            empreinte = empreinte_de(image)
            source_de.setdefault(empreinte, png)
            tailles[empreinte] = image.size
            lot.append((name, empreinte))
        retenues.append((meta, lot))
    # 2. OCR des images jamais vues, en parallèle (tesseract est un processus).
    verdicts = {}
    a_lire = []
    for empreinte in source_de:
        if not ocr:
            verdicts[empreinte] = (not exiger_ocr, "pas d'OCR")
        elif empreinte in connus:
            verdicts[empreinte] = connus[empreinte]
        else:
            a_lire.append(empreinte)
    if a_lire:
        from concurrent.futures import ThreadPoolExecutor
        with ThreadPoolExecutor(max_workers=max(1, travaux)) as pool:
            for empreinte, verdict in zip(a_lire, pool.map(lambda e: verdict_ocr(source_de[e]), a_lire)):
                verdicts[empreinte] = verdict
    # Cache écrit dès l'OCR fini ; un échec d'OCR n'y entre pas, il sera retenté.
    if cache and ocr:
        connus.update({k: v for k, v in verdicts.items()
                       if (k in a_lire or k in connus) and not v[1].startswith("OCR impossible")})
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_text(json.dumps({"motif": INTERDIT.pattern, "verdicts": connus}), encoding="utf-8")
    # 3. Images publiées, ré-encodées, une fois par empreinte.
    for empreinte, png in source_de.items():
        if verdicts[empreinte][0]:
            image = ouvrir(png)
            image.save(sortie / "img" / f"{empreinte}.webp", "WEBP", lossless=True, method=4)
            image.thumbnail((THUMB_WIDTH, THUMB_WIDTH * 4))
            image.save(sortie / "min" / f"{empreinte}.webp", "WEBP", quality=80, method=4)
    runs = []
    ordre = list(blanche)
    for meta, lot in retenues:
        images = []
        for name, empreinte in lot:
            ok, raison = verdicts[empreinte]
            if not ok:
                refusees[name] = raison
                continue
            entry = blanche[name]
            largeur, hauteur = tailles[empreinte]
            images.append({"nom": name, "test": entry["test"], "legende": {l: entry[l] for l in LANGS},
                           "image": f"img/{empreinte}.webp", "vignette": f"min/{empreinte}.webp",
                           "largeur": largeur, "hauteur": hauteur})
        if images:
            images.sort(key=lambda i: ordre.index(i["nom"]))
            runs.append({k: meta.get(k) for k in ("run", "numero", "url", "commit", "branche", "evenement",
                                                  "pr", "titre", "date", "conclusion")} | {"images": images})
    data = {"genere": max((r["date"] for r in runs), default=None), "executions": runs}
    (sortie / "data.json").write_text(json.dumps(data, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    poids = sum(p.stat().st_size for p in sortie.rglob("*") if p.is_file())
    lignes = ["## Suivi des travaux (#1779)", "",
              f"{len(runs)} exécution(s), {sum(len(r['images']) for r in runs)} capture(s) affichée(s), "
              f"{sum(1 for v in verdicts.values() if v[0])} image(s) distincte(s), "
              f"{len(a_lire)} lue(s) par OCR, {poids / 1e6:.1f} Mo publiés."]
    if refusees:
        lignes += ["", "Refusées : " + ", ".join(f"{k} ({v})" for k, v in sorted(refusees.items()))]
    if privees:
        lignes += ["", "Hors liste blanche, restées privées : " + ", ".join(sorted(privees))]
    print("\n".join(lignes))
    resume = os.environ.get("GITHUB_STEP_SUMMARY")
    if resume:
        with open(resume, "a", encoding="utf-8") as handle:
            handle.write("\n".join(lignes) + "\n")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="action", required=True)
    t = sub.add_parser("telecharger")
    t.add_argument("depot")
    t.add_argument("travail", type=Path)
    t.add_argument("--nombre", type=int, default=30)
    c = sub.add_parser("construire")
    c.add_argument("travail", type=Path)
    c.add_argument("sortie", type=Path)
    c.add_argument("--exiger-ocr", action="store_true")
    c.add_argument("--liste", type=Path, default=LISTE)
    c.add_argument("--cache", type=Path, help="verdicts OCR gardés d'une publication à l'autre")
    c.add_argument("--travaux", type=int, default=os.cpu_count() or 2)
    args = parser.parse_args()
    if args.action == "telecharger":
        return telecharger(args.depot, args.travail, args.nombre)
    return construire(args.travail, args.sortie, args.exiger_ocr, args.liste, args.cache, args.travaux)


if __name__ == "__main__":
    raise SystemExit(main())
