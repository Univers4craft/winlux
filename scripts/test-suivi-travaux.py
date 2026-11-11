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
            # Page absente du runner photographiée sur « Écran » avant #1944.
            screen(images / "parametres-08.png", "Définition de l'écran")
            screen(images / "parametres-26.png", "Définition de l'écran")
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
            check(names == ["parametres-02", "parametres-08"] * 2, f"seules les captures permises sont publiées ({names})")
            check(len(list((out / "img").iterdir())) == 2 and len(list((out / "min").iterdir())) == 2,
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
        check("parametres-26 (page « Écran » sous un autre nom (#1944))" in result.stdout,
              "page de Paramètres identique à « Écran » écartée (#1944)")
        check(not any(p.suffix == ".log" for p in out.rglob("*")), "aucun journal publié")

        # Sans tesseract dans le PATH, --exiger-ocr ferme tout.
        env = dict(os.environ, PATH=str(tmp / "vide"))
        closed = run(work, tmp / "ferme", "--exiger-ocr", env=env)
        data = json.loads((tmp / "ferme" / "data.json").read_text(encoding="utf-8"))
        check(closed.returncode == 0 and data["executions"] == [], "OCR absent : aucune capture publiée")

        # Un tesseract qui échoue refuse l'image au lieu d'arrêter la construction (#1829).
        faux = tmp / "faux-bin"
        faux.mkdir()
        (faux / "tesseract").write_text("#!/bin/sh\nexit 1\n", encoding="utf-8")
        (faux / "tesseract").chmod(0o755)
        casse = run(work, tmp / "casse", "--exiger-ocr", env=dict(os.environ, PATH=f"{faux}:{os.environ.get('PATH', '')}"))
        data = json.loads((tmp / "casse" / "data.json").read_text(encoding="utf-8")) if casse.returncode == 0 else None
        check(data is not None and data["executions"] == [] and "OCR impossible" in casse.stdout,
              "OCR en échec : capture refusée, construction menée à son terme (#1829)")

        # Galerie reprise d'une publication précédente quand l'OCR manque (#1871).
        import io, tarfile, zipfile
        tar_buf = io.BytesIO()
        with tarfile.open(fileobj=tar_buf, mode="w") as tar:
            def ajoute(nom, donnees, **kw):
                info = tarfile.TarInfo(nom)
                info.size = len(donnees)
                for k, v in kw.items():
                    setattr(info, k, v)
                tar.addfile(info, io.BytesIO(donnees))
            ajoute("./fr/suivi.html", b"<html>")
            ajoute("./suivi/data.json", json.dumps({"executions": [{"images": [{"nom": "parametres-02"}]}]}).encode())
            ajoute("./suivi/img/0123456789abcdef.webp", b"RIFF-img")
            ajoute("./suivi/min/0123456789abcdef.webp", b"RIFF-min")
            ajoute("./suivi/../../evade.webp", b"x")
            ajoute("./suivi/img/notes.txt", b"x")
            lien = tarfile.TarInfo("./suivi/img/fedcba9876543210.webp")
            lien.type, lien.linkname = tarfile.SYMTYPE, "/etc/passwd"
            tar.addfile(lien)
        archive = tmp / "pages.zip"
        with zipfile.ZipFile(archive, "w") as z:
            z.writestr("artifact.tar", tar_buf.getvalue())
        reprise = tmp / "reprise" / "suivi"
        prec = subprocess.run([sys.executable, "-I", str(HERE / "suivi-galerie-precedente.py"), "x/y", str(reprise),
                               "--archive", str(archive)], capture_output=True, text=True)
        fichiers = sorted(p.relative_to(tmp / "reprise").as_posix() for p in (tmp / "reprise").rglob("*") if p.is_file())
        check(prec.returncode == 0 and fichiers == ["suivi/data.json", "suivi/img/0123456789abcdef.webp",
                                                   "suivi/min/0123456789abcdef.webp"]
              and not (tmp / "evade.webp").exists()
              and json.loads((reprise / "data.json").read_text(encoding="utf-8")).get("reprise") is True,
              f"galerie précédente reprise, sans lien ni chemin sortant (#1871) ({fichiers})")
        vide = tmp / "vide.zip"
        with zipfile.ZipFile(vide, "w") as z:
            z.writestr("artifact.tar", b"")
        sans = subprocess.run([sys.executable, "-I", str(HERE / "suivi-galerie-precedente.py"), "x/y",
                               str(tmp / "sans"), "--archive", str(vide)], capture_output=True, text=True)
        check(sans.returncode == 1, "archive sans galerie : rien de repris, code 1 (#1871)")

        # Vignette recadrée sur une petite fenêtre posée sur un bureau noir (#1925).
        import importlib.util
        from PIL import Image
        spec = importlib.util.spec_from_file_location("suivi_travaux", HERE / "suivi-travaux.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        quatre_k = Image.new("RGB", (3840, 2160), (0, 0, 0))
        quatre_k.paste(Image.new("RGB", (600, 700), (240, 240, 245)), (3100, 1300))
        boite = module.cadrage(quatre_k)
        check(boite is not None and boite[0] <= 3100 and boite[1] <= 1300 and boite[2] >= 3700 and boite[3] >= 2000
              and boite[2] - boite[0] <= 700 and boite[3] - boite[1] <= 800,
              f"4K à fond noir : vignette recadrée sur la fenêtre, sans bande noire (#1925, #1957) ({boite})")
        coin = Image.new("RGB", (2560, 1440), (0, 0, 0))
        coin.paste(Image.new("RGB", (1020, 740), (233, 236, 244)), (0, 0))
        boite = module.cadrage(coin)
        check(boite == (0, 0, 1020, 740), f"fenêtre posée en (0, 0) : recadrée au ras, sans bande noire (#1943) (#1957) ({boite})")
        bureau = Image.new("RGB", (1920, 1080), (40, 60, 120))
        bureau.paste(Image.new("RGB", (1920, 48), (20, 20, 30)), (0, 1032))
        bureau.paste(Image.new("RGB", (1200, 800), (240, 240, 245)), (300, 100))
        check(module.cadrage(bureau) is None and module.cadrage(Image.new("RGB", (800, 500))) is None,
              "capture remplie ou vide : vignette de l'image entière (#1925)")

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
