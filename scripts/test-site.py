#!/usr/bin/env python3
"""Teste le site web de Winlux (site/, publié par .github/workflows/site.yml).

    python3 -B scripts/test-site.py [--render] [--online] [--keep DOSSIER]

Sans option, tout est hors ligne et sans dépendance :
  1. le site se construit (site/build.py) ;
  2. les quatre langues sont complètes : mêmes pages, mêmes chaînes d'interface
     non vides, même structure de page (titres, images, tableaux, listes,
     encadrés, blocs de code) et aucun paragraphe laissé en français ;
  3. aucun lien interne cassé : chaque href/src relatif mène à un fichier,
     chaque #ancre existe dans la page visée ;
  4. les liens externes ne vont que vers le dépôt GitHub du projet ;
  5. aucune marque ni ressource Microsoft (pas de logo Windows, pas de lien
     vers un site Microsoft), seule la mention légale de la marque est permise ;
  6. les images sont toutes utilisées, légères, et décrites (alt) ;
  7. la page « Premier démarrage » décrit la vraie fenêtre Bienvenue
     (scripts/winlux-welcome.py) : chacun de ses boutons y est nommé, dans
     sa traduction exacte, pour chaque langue.
--render  rend chaque page dans Chromium (Playwright, via Node) en mobile,
          tablette, portable et grand écran : pas de défilement horizontal,
          images chargées, navigation visible, colonne de texte lisible.
          WINLUX_SITE_RENDER=required rend ce test obligatoire.
--online  vérifie aussi que les liens externes répondent (wiki, releases…).
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urldefrag, urlparse

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
CONFIG = json.loads((SITE / "site.json").read_text(encoding="utf-8"))
LANGS = CONFIG["languages"]
PAGES = CONFIG["pages"]
REPO = CONFIG["repo"]
BASE_URL = CONFIG["url"].rstrip("/")

errors: list[str] = []


def fail(msg: str) -> None:
    errors.append(msg)
    print(f"ÉCHEC : {msg}")


class Page(HTMLParser):
    """Relève les liens, les ancres et la structure d'une page générée."""

    STRUCT = {"h1", "h2", "h3", "figure", "table", "tr", "li", "pre", "aside", "img"}

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.links: list[tuple[str, str]] = []  # (attribut, valeur)
        self.ids: set[str] = set()
        self.structure: list[str] = []
        self.texts: list[str] = []
        self.imgs: list[dict] = []
        self.lang = ""
        self._text_tag: str | None = None
        self._buf: list[str] = []
        self._in_main = False
        self._skip = 0

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "html":
            self.lang = a.get("lang", "")
        if "id" in a:
            if a["id"] in self.ids:
                fail(f"identifiant en double « {a['id']} »")
            self.ids.add(a["id"])
        if tag in ("a", "link") and a.get("href"):
            self.links.append(("href", a["href"]))
        if tag in ("img", "script") and a.get("src"):
            self.links.append(("src", a["src"]))
        if tag == "main":
            self._in_main = True
        if tag == "img":
            self.imgs.append(a)
        if tag in ("pre", "code", "script", "style"):
            self._skip += 1
        if self._in_main and tag in self.STRUCT:
            label = tag
            if tag == "img":
                label = f"img:{Path(a.get('src', '')).name}"
            if tag == "aside":
                label = f"aside:{a.get('class', '')}"
            self.structure.append(label)
        if self._in_main and tag in ("p", "li", "td", "th", "h2", "h3", "figcaption") and not self._skip:
            self._text_tag = tag
            self._buf = []

    def handle_endtag(self, tag):
        if tag in ("pre", "code", "script", "style") and self._skip:
            self._skip -= 1
        if tag == "main":
            self._in_main = False
        if self._text_tag == tag:
            text = re.sub(r"\s+", " ", "".join(self._buf)).strip()
            if text:
                self.texts.append(text)
            self._text_tag = None

    def handle_data(self, data):
        if self._text_tag and not self._skip:
            self._buf.append(data)


def parse(path: Path) -> Page:
    p = Page()
    p.feed(path.read_text(encoding="utf-8"))
    return p


def check_sources() -> None:
    strings = {l: json.loads((SITE / "i18n" / f"{l}.json").read_text(encoding="utf-8")) for l in LANGS}
    ref = set(strings[LANGS[0]])
    for l in LANGS:
        keys = set(strings[l])
        if keys != ref:
            fail(f"i18n/{l}.json : clés en trop {sorted(keys - ref)} ou manquantes {sorted(ref - keys)}")
        for k, v in strings[l].items():
            if not str(v).strip():
                fail(f"i18n/{l}.json : « {k} » est vide")
        for k in ref & keys:
            if l != "fr" and len(strings[l][k]) > 12 and strings[l][k] == strings["fr"][k]:
                fail(f"i18n/{l}.json : « {k} » n'est pas traduit")
    for l in LANGS:
        present = {p.stem for p in (SITE / "content" / l).glob("*.md")}
        if present != set(PAGES):
            fail(f"content/{l} : pages en trop {sorted(present - set(PAGES))} ou manquantes {sorted(set(PAGES) - present)}")
    # Toutes les images de site/static/img servent, sont légères, et viennent
    # de l'identité Winlux ou de vraies captures.
    used = set()
    for md in (SITE / "content").rglob("*.md"):
        used |= set(re.findall(r"img:([\w.-]+)", md.read_text(encoding="utf-8")))
    css_js = (SITE / "static" / "style.css").read_text(encoding="utf-8") + (ROOT / "site" / "build.py").read_text(encoding="utf-8")
    for img in (SITE / "static" / "img").iterdir():
        if img.name not in used and img.name not in css_js:
            fail(f"image inutilisée : site/static/img/{img.name}")
        if img.stat().st_size > 300_000:
            fail(f"image trop lourde (> 300 Ko) : site/static/img/{img.name}")
        if re.search(r"windows|microsoft|msft|win11", img.name, re.I):
            fail(f"nom d'image interdit : {img.name}")


WELCOME_BUTTONS = ("Visite guidée et logiciels courants", "Choisir le clavier", "Configurer le réseau",
                   "Personnaliser l’apparence", "Découvrir le secours", "Terminer")


def check_welcome_page() -> None:
    """La page Premier démarrage nomme les boutons de la vraie fenêtre Bienvenue."""
    import ast
    script = (ROOT / "scripts" / "winlux-welcome.py").read_text(encoding="utf-8")
    table = None
    for node in ast.walk(ast.parse(script)):
        if isinstance(node, ast.Assign) and any(getattr(t, "id", "") == "_TR" for t in node.targets):
            table = ast.literal_eval(node.value)
    if table is None:
        fail("scripts/winlux-welcome.py : table _TR introuvable")
        return
    column = {"en": 0, "es": 1, "de": 2}
    for label in WELCOME_BUTTONS:
        if f'"{label}"' not in script:
            fail(f"scripts/winlux-welcome.py : bouton « {label} » absent, mettez WELCOME_BUTTONS à jour")
            continue
        for l in LANGS:
            text = label if l == "fr" else table.get(label, ("", "", ""))[column[l]]
            page = (SITE / "content" / l / "guide-first-start.md").read_text(encoding="utf-8")
            if not text or text not in page:
                fail(f"content/{l}/guide-first-start.md : le bouton « {text or label} » de la fenêtre Bienvenue n'est pas décrit")


def check_build(out: Path) -> set[str]:
    pages: dict[Path, Page] = {}
    for html_file in sorted(out.rglob("*.html")):
        pages[html_file] = parse(html_file)

    # Structure identique dans les quatre langues, textes traduits.
    for slug in PAGES:
        ref = pages[out / LANGS[0] / f"{slug}.html"]
        for l in LANGS:
            p = pages[out / l / f"{slug}.html"]
            if p.lang != l:
                fail(f"{l}/{slug}.html : <html lang> vaut « {p.lang} »")
            if p.structure != ref.structure:
                fail(
                    f"{l}/{slug}.html : structure différente du français "
                    f"({len(p.structure)} éléments contre {len(ref.structure)}) — traduction incomplète ?"
                )
                for i, (a, b) in enumerate(zip(ref.structure, p.structure)):
                    if a != b:
                        print(f"    premier écart, élément {i} : fr={a} {l}={b}")
                        break
            if l == LANGS[0]:
                continue
            ref_texts = {t for t in ref.texts if len(t) > 30 and not re.fullmatch(r"[\w.+ -]+ \d[\w. ]*", t)}
            same = [t for t in p.texts if t in ref_texts]
            for t in same:
                fail(f"{l}/{slug}.html : texte resté en français : « {t[:70]} »")

    # Liens internes et externes.
    allowed_hosts = {"github.com", "api.github.com", "univers4craft.github.io"}
    external: set[str] = set()
    for path, p in pages.items():
        rel = path.relative_to(out)
        for attr, value in p.links:
            if value.startswith(("https://", "http://")):
                u = urlparse(value)
                if value.startswith("http://"):
                    fail(f"{rel} : lien non chiffré {value}")
                if u.hostname not in allowed_hosts:
                    fail(f"{rel} : lien externe vers un site non prévu : {value}")
                elif u.hostname == "github.com" and not u.path.startswith(f"/{REPO}"):
                    fail(f"{rel} : lien GitHub hors du dépôt du projet : {value}")
                if value.startswith(BASE_URL + "/") or value == BASE_URL + "/":
                    # Adresse publique du site : le fichier doit exister.
                    local = out / unquote(urldefrag(value)[0][len(BASE_URL) + 1 :])
                    if local.is_dir():
                        local = local / "index.html"
                    if not local.is_file():
                        fail(f"{rel} : adresse publique sans page : {value}")
                else:
                    external.add(value)
                continue
            if value.startswith("mailto:"):
                continue
            target, anchor = urldefrag(value)
            dest = path if not target else (path.parent / unquote(target)).resolve()
            if not dest.is_file():
                fail(f"{rel} : lien cassé {value}")
                continue
            if anchor:
                dp = pages.get(dest) or parse(dest)
                if anchor not in dp.ids:
                    fail(f"{rel} : ancre absente {value}")
        for img in p.imgs:
            if "alt" not in img:
                fail(f"{rel} : image sans attribut alt : {img.get('src')}")

    # Chaque page de langue renvoie vers ses trois traductions.
    for l in LANGS:
        for slug in PAGES:
            text = (out / l / f"{slug}.html").read_text(encoding="utf-8")
            for other in LANGS:
                if f'href="../{other}/{slug}.html"' not in text:
                    fail(f"{l}/{slug}.html : pas de lien vers la version {other}")

    # Aucune ressource ni lien Microsoft ; la marque n'apparaît que dans la
    # mention légale du pied de page et dans la page « Position juridique ».
    for path in out.rglob("*"):
        if path.suffix not in (".html", ".css", ".js", ".xml"):
            continue
        text = path.read_text(encoding="utf-8")
        if re.search(r"microsoft\.com|windows\.com|live\.com|bing\.com|msn\.com", text, re.I):
            fail(f"{path.relative_to(out)} : lien vers un site Microsoft")
        body = re.sub(r'<p class="muted small">.*?</p>', "", text, flags=re.S)
        # La page « Position juridique » s'adresse à Microsoft : seule exception.
        if re.search(r"microsoft", body, re.I) and path.suffix == ".html" and path.name != "position.html":
            fail(f"{path.relative_to(out)} : « Microsoft » cité hors de la mention légale")

    # Fichiers attendus par GitHub Pages.
    for name in ("index.html", "404.html", ".nojekyll", "sitemap.xml"):
        if not (out / name).exists():
            fail(f"fichier absent du site construit : {name}")

    print(f"{len(pages)} pages HTML vérifiées, {len(external)} liens externes distincts.")
    return external


def check_online(external: set[str]) -> None:
    for url in sorted(external):
        if "api.github.com" in url:
            continue
        req = urllib.request.Request(url, method="GET", headers={"User-Agent": "winlux-site-test"})
        try:
            with urllib.request.urlopen(req, timeout=20) as r:
                code = r.status
        except urllib.error.HTTPError as e:
            code = e.code
        except Exception as e:  # noqa: BLE001
            fail(f"lien externe injoignable : {url} ({e})")
            continue
        if code >= 400:
            fail(f"lien externe en erreur {code} : {url}")


def check_render(out: Path) -> bool:
    node = shutil.which("node")
    script = ROOT / "scripts" / "test-site-render.js"
    if not node:
        return False
    probe = subprocess.run([node, "-e", "require('playwright')"], capture_output=True, cwd=ROOT)
    if probe.returncode != 0:
        return False
    shots = os.environ.get("WINLUX_SITE_SHOTS", "")
    res = subprocess.run([node, str(script), str(out), shots], cwd=ROOT)
    if res.returncode != 0:
        fail("rendu du site dans Chromium (voir ci-dessus)")
    return True


def main() -> int:
    args = sys.argv[1:]
    keep = None
    if "--keep" in args:
        keep = Path(args[args.index("--keep") + 1]).resolve()
    check_sources()
    check_welcome_page()
    tmp = Path(tempfile.mkdtemp(prefix="winlux-site-"))
    out = keep or tmp / "site"
    try:
        res = subprocess.run([sys.executable, "-B", str(SITE / "build.py"), str(out)], cwd=ROOT)
        if res.returncode != 0:
            fail("construction du site")
            return 1
        external = check_build(out)
        if "--online" in args:
            check_online(external)
        if "--render" in args or os.environ.get("WINLUX_SITE_RENDER"):
            if not check_render(out):
                msg = "rendu non testé : Node.js et Playwright sont nécessaires"
                if os.environ.get("WINLUX_SITE_RENDER") == "required":
                    fail(msg)
                else:
                    print(f"IGNORÉ : {msg}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if errors:
        print(f"\n{len(errors)} problème(s) sur le site.")
        return 1
    print("Site web : tout est correct.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
