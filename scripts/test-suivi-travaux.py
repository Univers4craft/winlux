#!/usr/bin/python3
"""Filtres de la galerie « Suivi des travaux » (#1779), sans réseau.

Deux fausses exécutions de CI : une capture en liste blanche publiée une seule
fois (même image dans les deux exécutions), une capture hors liste restée
privée, une capture en liste mais portant une marque Microsoft refusée par
l'OCR, un faux PNG refusé, et sans tesseract avec --exiger-ocr : rien publié.
Sans Pillow, le test est sauté (code 0, message SAUTÉ).
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
failures = []


def check(cond, label):
    print(("ok   " if cond else "ÉCHEC ") + label)
    if not cond:
        failures.append(label)


def screen(path, text, dark=True):
    from PIL import Image, ImageDraw, ImageFont
    image = Image.new("RGB", (1280, 720), (32, 32, 40) if dark else (245, 245, 250))
    draw = ImageDraw.Draw(image)
    try:
        font = ImageFont.truetype("DejaVuSans.ttf", 40)
    except OSError:
        font = ImageFont.load_default(size=40)
    draw.text((80, 300), text, fill=(235, 235, 240) if dark else (20, 20, 30), font=font)
    image.save(path)


def run(work, out, *extra, env=None):
    return subprocess.run([sys.executable, "-I", str(HERE / "suivi-travaux.py"), "construire", str(work), str(out), *extra],
                          capture_output=True, text=True, env=env)


def main():
    try:
        import PIL  # noqa: F401
    except ImportError:
        print("SAUTÉ : Pillow absent")
        return 0
    tmp = Path(tempfile.mkdtemp(prefix="suivi-"))
    try:
        work, out = tmp / "travail", tmp / "site"
        for number, (run_id, date) in enumerate(((101, "2026-11-10T08:00:00Z"), (102, "2026-11-11T09:00:00Z"))):
            images = work / str(run_id) / "images"
            images.mkdir(parents=True)
            screen(images / "parametres-02.png", "Réseau : connecté par câble")
            screen(images / "outils-apps.png", "Liste des applications du runner")
            screen(images / "parametres-03.png", "Fabricant : Microsoft Corporation")
            (images / "popup-volume.png").write_bytes(b"pas une image")
            (images / "parametres-02.log").write_text("journal", encoding="utf-8")
            (work / str(run_id) / "meta.json").write_text(json.dumps({
                "run": run_id, "numero": number + 1, "url": f"https://github.com/x/y/actions/runs/{run_id}",
                "commit": f"{run_id:07d}abcdef", "branche": "master", "evenement": "push", "pr": None,
                "titre": f"Commit {run_id}", "date": date, "conclusion": "success"}), encoding="utf-8")

        has_ocr = shutil.which("tesseract") is not None
        result = run(work, out, "--exiger-ocr")
        check(result.returncode == 0, "construire se termine sans erreur")
        data = json.loads((out / "data.json").read_text(encoding="utf-8"))
        runs = data["executions"]
        if has_ocr:
            check([r["run"] for r in runs] == [102, 101], "exécutions de la plus récente à la plus ancienne")
            names = [i["nom"] for r in runs for i in r["images"]]
            check(names == ["parametres-02", "parametres-02"], f"seule la capture permise est publiée ({names})")
            check(len(list((out / "img").iterdir())) == 1 and len(list((out / "min").iterdir())) == 1,
                  "image identique stockée une seule fois, avec sa vignette")
            first = runs[0]["images"][0]
            check(first["legende"]["de"].startswith("Einstellungen") and "capture-ui.py" in first["test"],
                  "légende en quatre langues et test d'origine")
            check("parametres-03 (OCR" in result.stdout, "capture portant « Microsoft » refusée par l'OCR")
        else:
            check(runs == [] and not any((out / "img").iterdir()), "sans tesseract, --exiger-ocr ne publie rien")
        check("outils-apps" in result.stdout and "restées privées" in result.stdout,
              "capture hors liste blanche restée privée")
        check("popup-volume (fichier PNG invalide" in result.stdout, "faux PNG refusé")
        check(not any(p.suffix == ".log" for p in out.rglob("*")), "aucun journal publié")

        # Sans tesseract dans le PATH, --exiger-ocr ferme tout.
        env = dict(os.environ, PATH=str(tmp / "vide"))
        closed = run(work, tmp / "ferme", "--exiger-ocr", env=env)
        data = json.loads((tmp / "ferme" / "data.json").read_text(encoding="utf-8"))
        check(closed.returncode == 0 and data["executions"] == [], "OCR absent : aucune capture publiée")

        # Page index.html de l'artefact captures-ui.
        artefact = work / "102" / "images"
        (artefact / "r.json").write_text('{"ecrans": {"parametres-02": 3.5}}', encoding="utf-8")
        index = subprocess.run([sys.executable, "-I", str(HERE / "index-captures.py"), str(artefact),
                                "--rapport", str(artefact / "r.json")], capture_output=True, text=True,
                               env=dict(os.environ, GITHUB_SHA="abcdef123456", GITHUB_REPOSITORY="x/y", GITHUB_RUN_ID="7"))
        page = (artefact / "index.html").read_text(encoding="utf-8") if index.returncode == 0 else ""
        check('src="parametres-02.png"' in page and "3.50 % changés" in page and "/x/y/actions/runs/7" in page,
              "index.html de l'artefact : vignettes, pourcentage changé, lien de l'exécution")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print("Suivi des travaux : " + ("tout est correct." if not failures else f"{len(failures)} échec(s)."))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
