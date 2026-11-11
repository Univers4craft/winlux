#!/usr/bin/python3
"""Page index.html qui présente une série de captures d'écran (#1779).

    index-captures.py DOSSIER [--rapport rapport.json] [--titre TEXTE]

Écrit DOSSIER/index.html : une vignette par PNG du dossier (sous-dossiers
compris), groupées par préfixe de nom, avec un champ de filtre, la taille de
chaque image et, si --rapport pointe vers le rapport.json de
compare-captures.py, le pourcentage de pixels changés par écran. En CI, le
commit, la branche et le lien de l'exécution viennent des variables GITHUB_*.

La page est autonome (aucune ressource externe, images en chemins relatifs) :
on décompresse l'artefact et on ouvre index.html dans le navigateur.
Bibliothèque standard seulement, pour tourner partout avec « python3 -I ».
"""
import argparse
import datetime
import html
import json
import os
from pathlib import Path
import struct
import sys
from urllib.parse import quote


def png_size(path):
    """(largeur, hauteur) lue dans l'en-tête IHDR, ou None."""
    try:
        with open(path, "rb") as handle:
            head = handle.read(24)
    except OSError:
        return None
    if len(head) < 24 or head[:8] != b"\x89PNG\r\n\x1a\n" or head[12:16] != b"IHDR":
        return None
    return struct.unpack(">II", head[16:24])


def group_of(name):
    stem = name.rsplit("/", 1)[-1]
    for sep in ("-", "_"):
        if sep in stem:
            return stem.split(sep, 1)[0]
    return stem


def context():
    env = os.environ
    bits = []
    sha = env.get("GITHUB_SHA", "")
    repo = env.get("GITHUB_REPOSITORY", "")
    server = env.get("GITHUB_SERVER_URL", "https://github.com")
    if sha:
        label = html.escape(sha[:7])
        bits.append(f'commit <a href="{server}/{repo}/commit/{sha}">{label}</a>' if repo else f"commit {label}")
    ref = env.get("GITHUB_HEAD_REF") or env.get("GITHUB_REF_NAME")
    if ref:
        bits.append("branche " + html.escape(ref))
    run = env.get("GITHUB_RUN_ID")
    if run and repo:
        bits.append(f'<a href="{server}/{repo}/actions/runs/{run}">exécution {html.escape(run)}</a>')
    bits.append(datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M UTC"))
    return " · ".join(bits)


PAGE = """<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{titre}</title>
<style>
:root {{ --fond:#f6f5fa; --carte:#fff; --texte:#1c1b22; --doux:#5d5a6b; --accent:#6A3DF0; --alerte:#b3261e; --bord:#e1deea; }}
@media (prefers-color-scheme: dark) {{
  :root {{ --fond:#16151b; --carte:#211f29; --texte:#ecebf2; --doux:#a9a6b8; --accent:#B69CFF; --alerte:#ffb4ab; --bord:#34313f; }}
}}
* {{ box-sizing: border-box; }}
body {{ margin:0; padding:16px; background:var(--fond); color:var(--texte); font:15px/1.4 system-ui, sans-serif; }}
h1 {{ font-size:22px; margin:0 0 4px; }}
h2 {{ font-size:17px; margin:24px 0 8px; color:var(--doux); }}
a {{ color:var(--accent); }}
.meta {{ color:var(--doux); margin-bottom:12px; }}
input {{ width:100%; max-width:420px; padding:8px 10px; border:1px solid var(--bord); border-radius:6px; background:var(--carte); color:var(--texte); font:inherit; }}
.grille {{ display:grid; grid-template-columns:repeat(auto-fill, minmax(240px, 1fr)); gap:12px; }}
.carte {{ background:var(--carte); border:1px solid var(--bord); border-radius:8px; overflow:hidden; }}
.carte img {{ display:block; width:100%; height:160px; object-fit:contain; background:#0001; }}
.nom {{ padding:6px 8px 0; word-break:break-all; font-weight:600; font-size:13px; }}
.info {{ padding:0 8px 8px; color:var(--doux); font-size:12px; }}
.fort {{ color:var(--alerte); font-weight:600; }}
.cache {{ display:none; }}
</style>
</head>
<body>
<h1>{titre}</h1>
<div class="meta">{meta}</div>
<p><input id="filtre" type="search" placeholder="Filtrer par nom d'écran…" autofocus> <span id="compte">{total} image(s)</span></p>
{groupes}
<script>
const champ = document.getElementById("filtre"), compte = document.getElementById("compte");
champ.addEventListener("input", () => {{
  const q = champ.value.trim().toLowerCase(); let n = 0;
  for (const g of document.querySelectorAll("section")) {{
    let vus = 0;
    for (const c of g.querySelectorAll(".carte")) {{
      const ok = !q || c.dataset.nom.includes(q); c.classList.toggle("cache", !ok); if (ok) vus++;
    }}
    g.classList.toggle("cache", vus === 0); n += vus;
  }}
  compte.textContent = n + " image(s)";
}});
</script>
</body>
</html>
"""


def build(folder, report, title):
    images = sorted(p for p in folder.rglob("*.png") if p.is_file())
    changes = {}
    if report:
        try:
            changes = json.loads(Path(report).read_text(encoding="utf-8")).get("ecrans", {})
        except (OSError, ValueError):
            changes = {}
    groups = {}
    for path in images:
        rel = path.relative_to(folder).as_posix()
        groups.setdefault(group_of(rel), []).append((rel, path))
    sections = []
    for name in sorted(groups):
        cards = []
        for rel, path in groups[name]:
            size = png_size(path)
            info = [f"{size[0]}×{size[1]}" if size else "PNG",
                    f"{path.stat().st_size // 1024} Ko"]
            stem = path.stem[:-5] if path.stem.endswith("-diff") else path.stem
            if stem in changes:
                pct = changes[stem]
                cls = ' class="fort"' if pct >= 2 else ""
                info.append(f"<span{cls}>{pct:.2f} % changés</span>")
            href = quote(rel)
            label = html.escape(rel)
            cards.append(
                f'<div class="carte" data-nom="{html.escape(rel.lower())}">'
                f'<a href="{href}"><img src="{href}" alt="{label}" loading="lazy"></a>'
                f'<div class="nom">{label}</div><div class="info">{" · ".join(info)}</div></div>')
        sections.append(f'<section><h2>{html.escape(name)} ({len(cards)})</h2>'
                        f'<div class="grille">{"".join(cards)}</div></section>')
    if not sections:
        sections.append("<p>Aucune capture dans cet artefact.</p>")
    page = PAGE.format(titre=html.escape(title), meta=context(), total=len(images),
                       groupes="\n".join(sections))
    (folder / "index.html").write_text(page, encoding="utf-8")
    return len(images)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("dossier", type=Path)
    parser.add_argument("--rapport", help="rapport.json de compare-captures.py")
    parser.add_argument("--titre", default="Captures d'écran Winlux")
    args = parser.parse_args()
    if not args.dossier.is_dir():
        print(f"index-captures : dossier absent : {args.dossier}", file=sys.stderr)
        return 2
    count = build(args.dossier, args.rapport, args.titre)
    print(f"index-captures : {args.dossier / 'index.html'} ({count} image(s))")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
