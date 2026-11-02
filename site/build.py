#!/usr/bin/env python3
"""Construit le site web de Winlux (GitHub Pages) à partir de site/.

    python3 site/build.py [DOSSIER_DE_SORTIE]      (défaut : build/site)

Aucune dépendance hors de la bibliothèque standard de Python : le Markdown des
pages (site/content/<langue>/<page>.md) est converti par le petit convertisseur
ci-dessous, qui ne connaît que ce dont le site a besoin.

Liens spéciaux dans le Markdown :
    page:guide-usb#ancre   page du site, dans la même langue
    wiki:Installation      page du wiki GitHub (doit exister dans wiki/)
    repo:docs/AUDIO.md     fichier ou dossier du dépôt (doit exister)
    issue:287              issue GitHub
    img:capture.jpg        image de site/static/img/
Codes courts, seuls sur leur ligne : {{download}} (boutons de téléchargement)
et {{steps}} (liste des étapes du guide).
Blocs : ::: cards / ::: steps-grid / ::: figure-row ... ::: ; citations
« > [!note] », « > [!warning] », « > [!tip] ».
"""

from __future__ import annotations

import html
import json
import re
import shutil
import sys
import unicodedata
from pathlib import Path

SITE = Path(__file__).resolve().parent
ROOT = SITE.parent
CONFIG = json.loads((SITE / "site.json").read_text(encoding="utf-8"))
REPO = CONFIG["repo"]
GITHUB = f"https://github.com/{REPO}"
LANGS = CONFIG["languages"]
PAGES = CONFIG["pages"]
GUIDE = CONFIG["guide"]


class SiteError(Exception):
    pass


# --------------------------------------------------------------------------
# Markdown
# --------------------------------------------------------------------------


def slugify(text: str) -> str:
    text = re.sub(r"<[^>]+>", "", text)
    text = unicodedata.normalize("NFKD", text)
    text = "".join(c for c in text if not unicodedata.combining(c)).lower()
    text = re.sub(r"[^a-z0-9]+", "-", text).strip("-")
    return text or "section"


def image_size(path: Path) -> tuple[int, int]:
    """Largeur et hauteur d'un PNG ou d'un JPEG, sans bibliothèque externe."""
    data = path.read_bytes()
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")
    if data[:2] == b"\xff\xd8":
        i = 2
        while i + 9 < len(data):
            if data[i] != 0xFF:
                i += 1
                continue
            marker = data[i + 1]
            length = int.from_bytes(data[i + 2 : i + 4], "big")
            if marker in (0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF):
                return int.from_bytes(data[i + 7 : i + 9], "big"), int.from_bytes(data[i + 5 : i + 7], "big")
            i += 2 + length
    raise SiteError(f"format d'image inconnu : {path.name}")


class Renderer:
    def __init__(self, lang: str, page: str, strings: dict, meta: dict):
        self.lang = lang
        self.page = page
        self.strings = strings
        self.meta = meta  # titres et descriptions de toutes les pages de la langue
        self.ids: set[str] = set()
        self.headings: list[tuple[int, str, str]] = []

    # -- liens -------------------------------------------------------------
    def href(self, target: str) -> str:
        if target.startswith("page:"):
            slug, _, anchor = target[5:].partition("#")
            if slug not in PAGES:
                raise SiteError(f"{self.lang}/{self.page} : page inconnue « {slug} »")
            return f"{slug}.html" + (f"#{anchor}" if anchor else "")
        if target.startswith("wiki:"):
            name, _, anchor = target[5:].partition("#")
            if not (ROOT / "wiki" / f"{name}.md").is_file():
                raise SiteError(f"{self.lang}/{self.page} : page de wiki absente « {name} »")
            return f"{GITHUB}/wiki/{name}" + (f"#{anchor}" if anchor else "")
        if target.startswith("repo:"):
            path = target[5:]
            if not (ROOT / path).exists():
                raise SiteError(f"{self.lang}/{self.page} : fichier absent du dépôt « {path} »")
            kind = "tree" if (ROOT / path).is_dir() else "blob"
            return f"{GITHUB}/{kind}/master/{path.rstrip('/')}"
        if target.startswith("issue:"):
            return f"{GITHUB}/issues/{int(target[6:])}"
        if target.startswith("img:"):
            name = target[4:]
            if not (SITE / "static" / "img" / name).is_file():
                raise SiteError(f"{self.lang}/{self.page} : image absente « {name} »")
            return f"../static/img/{name}"
        if target.startswith(("https://", "mailto:", "#")):
            return target
        raise SiteError(f"{self.lang}/{self.page} : lien non pris en charge « {target} »")

    # -- texte en ligne ----------------------------------------------------
    def inline(self, text: str) -> str:
        codes: list[str] = []

        def keep_code(m: re.Match) -> str:
            codes.append(f"<code>{html.escape(m.group(1), quote=False)}</code>")
            return f"\x00{len(codes) - 1}\x00"

        text = re.sub(r"`([^`]+)`", keep_code, text)
        text = html.escape(text, quote=False)

        def image(m: re.Match) -> str:
            return f'<img src="{self.href(m.group(2))}" alt="{html.escape(m.group(1))}" loading="lazy">'

        def link(m: re.Match) -> str:
            url = self.href(html.unescape(m.group(2)))
            ext = ' rel="noopener"' if url.startswith("https://") else ""
            return f'<a href="{html.escape(url)}"{ext}>{m.group(1)}</a>'

        text = re.sub(r"!\[([^\]]*)\]\(([^)\s]+)\)", image, text)
        text = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", link, text)
        text = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
        text = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", text)
        if self.lang == "fr":
            # Espaces insécables de la typographie française.
            text = text.replace("« ", "«\u00a0").replace(" »", "\u00a0»")
            text = re.sub(r" ([:;?!])(?=\s|$|<)", "\u00a0\\1", text)
        text = re.sub(r"\x00(\d+)\x00", lambda m: codes[int(m.group(1))], text)
        return text

    # -- blocs -------------------------------------------------------------
    def heading(self, level: int, raw: str) -> str:
        m = re.match(r"(.*?)\s*\{#([a-z0-9-]+)\}\s*$", raw)
        text, ident = (m.group(1), m.group(2)) if m else (raw, slugify(raw))
        base, n = ident, 2
        while ident in self.ids:
            ident, n = f"{base}-{n}", n + 1
        self.ids.add(ident)
        inner = self.inline(text)
        self.headings.append((level, ident, re.sub(r"<[^>]+>", "", inner)))
        return f'<h{level} id="{ident}">{inner}</h{level}>'

    def figure(self, alt: str, target: str, caption: str) -> str:
        src = self.href(target)
        w, h = image_size(SITE / "static" / "img" / target[4:])
        cap = f"<figcaption>{self.inline(caption)}</figcaption>" if caption else ""
        return (
            f'<figure><a href="{src}" class="zoom"><img src="{src}" alt="{html.escape(alt)}" '
            f'width="{w}" height="{h}" loading="lazy" decoding="async"></a>{cap}</figure>'
        )

    def table(self, rows: list[str]) -> str:
        def cells(row: str) -> list[str]:
            row = row.strip()
            if row.startswith("|"):
                row = row[1:]
            if row.endswith("|"):
                row = row[:-1]
            return [c.strip() for c in row.split("|")]

        head = cells(rows[0])
        out = ['<div class="table-wrap"><table><thead><tr>']
        out += [f"<th>{self.inline(c)}</th>" for c in head]
        out.append("</tr></thead><tbody>")
        for row in rows[2:]:
            out.append("<tr>" + "".join(f"<td>{self.inline(c)}</td>" for c in cells(row)) + "</tr>")
        out.append("</tbody></table></div>")
        return "".join(out)

    def blocks(self, lines: list[str]) -> str:
        out: list[str] = []
        i = 0
        para: list[str] = []

        def flush() -> None:
            if para:
                out.append(f"<p>{self.inline(' '.join(s.strip() for s in para))}</p>")
                para.clear()

        while i < len(lines):
            line = lines[i]
            stripped = line.strip()
            if not stripped:
                flush()
                i += 1
                continue
            if stripped.startswith("```"):
                flush()
                lang = stripped[3:].strip()
                j = i + 1
                code = []
                while j < len(lines) and not lines[j].strip().startswith("```"):
                    code.append(lines[j])
                    j += 1
                if j >= len(lines):
                    raise SiteError(f"{self.lang}/{self.page} : bloc de code non fermé")
                cls = f' class="lang-{lang}"' if lang else ""
                out.append(f"<pre><code{cls}>{html.escape(chr(10).join(code), quote=False)}</code></pre>")
                i = j + 1
                continue
            if stripped.startswith(":::") and stripped != ":::":
                flush()
                name = stripped[3:].strip()
                depth, j = 1, i + 1
                while j < len(lines):
                    s = lines[j].strip()
                    if s == ":::":
                        depth -= 1
                        if depth == 0:
                            break
                    elif s.startswith(":::"):
                        depth += 1
                    j += 1
                if j >= len(lines):
                    raise SiteError(f"{self.lang}/{self.page} : bloc ::: {name} non fermé")
                inner = lines[i + 1 : j]
                out.append(self.container(name, inner))
                i = j + 1
                continue
            m = re.match(r"(#{1,4})\s+(.*)", stripped)
            if m:
                flush()
                out.append(self.heading(len(m.group(1)), m.group(2)))
                i += 1
                continue
            if stripped in ("{{download}}", "{{steps}}"):
                flush()
                out.append(self.download() if stripped == "{{download}}" else self.steps())
                i += 1
                continue
            m = re.fullmatch(r'!\[([^\]]*)\]\((img:[^)\s]+)(?:\s+"([^"]*)")?\)', stripped)
            if m:
                flush()
                out.append(self.figure(m.group(1), m.group(2), m.group(3) or ""))
                i += 1
                continue
            if stripped.startswith(">"):
                flush()
                quote = []
                while i < len(lines) and lines[i].strip().startswith(">"):
                    quote.append(re.sub(r"^\s*>\s?", "", lines[i]))
                    i += 1
                kind = "note"
                m = re.match(r"\[!(note|warning|tip)\]\s*$", quote[0].strip())
                if m:
                    kind = m.group(1)
                    quote = quote[1:]
                label = self.strings[f"callout_{kind}"]
                out.append(
                    f'<aside class="callout {kind}"><p class="callout-title">{label}</p>'
                    f"{self.blocks(quote)}</aside>"
                )
                continue
            if stripped.startswith("|") and i + 1 < len(lines) and re.match(r"^\s*\|[\s:|-]+\|\s*$", lines[i + 1]):
                flush()
                rows = []
                while i < len(lines) and lines[i].strip().startswith("|"):
                    rows.append(lines[i])
                    i += 1
                out.append(self.table(rows))
                continue
            m = re.match(r"(\s*)([-*]|\d+\.)\s+(.*)", line)
            if m and not para:
                ordered = m.group(2)[0].isdigit()
                items: list[list[str]] = []
                while i < len(lines):
                    m2 = re.match(r"(\s*)([-*]|\d+\.)\s+(.*)", lines[i])
                    if m2 and (m2.group(2)[0].isdigit()) == ordered and not m2.group(1):
                        items.append([m2.group(3)])
                    elif lines[i].startswith("  ") and lines[i].strip() and items:
                        items[-1].append(lines[i].strip())
                    else:
                        break
                    i += 1
                tag = "ol" if ordered else "ul"
                lis = "".join(f"<li>{self.inline(' '.join(it))}</li>" for it in items)
                out.append(f"<{tag}>{lis}</{tag}>")
                continue
            if stripped.startswith("<") and not para:
                raw = []
                while i < len(lines) and lines[i].strip():
                    raw.append(lines[i])
                    i += 1
                out.append("\n".join(raw))
                continue
            para.append(line)
            i += 1
        flush()
        return "\n".join(out)

    def container(self, name: str, inner: list[str]) -> str:
        if name not in ("cards", "figure-row", "steps-grid"):
            raise SiteError(f"{self.lang}/{self.page} : bloc inconnu ::: {name}")
        if name == "figure-row":
            return f'<div class="figure-row">{self.blocks(inner)}</div>'
        # Une carte par titre ###.
        cards: list[list[str]] = []
        for line in inner:
            if line.startswith("### "):
                cards.append([line])
            elif cards:
                cards[-1].append(line)
            elif line.strip():
                raise SiteError(f"{self.lang}/{self.page} : texte avant la première carte")
        body = "".join(f'<div class="card">{self.blocks(c)}</div>' for c in cards)
        return f'<div class="{name}">{body}</div>'

    # -- codes courts ------------------------------------------------------
    def download(self) -> str:
        s = self.strings
        latest = f"{GITHUB}/releases/latest"
        return f"""<div class="download" data-release>
<p class="download-version"><span data-release-version>{s['dl_latest']}</span> <span class="muted" data-release-date></span></p>
<div class="download-buttons">
<a class="btn btn-primary" data-asset="iso" href="{latest}" rel="noopener"><span class="btn-title">{s['dl_iso']}</span><span class="btn-sub" data-asset-size="iso">{s['dl_iso_sub']}</span></a>
<a class="btn" data-asset="usb-exe" href="{latest}" rel="noopener"><span class="btn-title">{s['dl_usb_exe']}</span><span class="btn-sub">{s['dl_usb_exe_sub']}</span></a>
<a class="btn" data-asset="usb-deb" href="{latest}" rel="noopener"><span class="btn-title">{s['dl_usb_deb']}</span><span class="btn-sub">{s['dl_usb_deb_sub']}</span></a>
</div>
<p class="download-more"><a data-asset="sha256" href="{latest}" rel="noopener">{s['dl_sha']}</a> · <a href="{GITHUB}/releases" rel="noopener">{s['dl_all']}</a></p>
</div>"""

    def steps(self) -> str:
        items = []
        for n, slug in enumerate(GUIDE, 1):
            m = self.meta[slug]
            items.append(
                f'<li><a href="{slug}.html"><span class="step-num">{n}</span>'
                f'<span class="step-text"><strong>{html.escape(m["title"])}</strong>'
                f'<span>{html.escape(m["description"])}</span></span></a></li>'
            )
        return f'<ol class="step-list">{"".join(items)}</ol>'


# --------------------------------------------------------------------------
# Pages
# --------------------------------------------------------------------------


def read_page(lang: str, slug: str) -> tuple[dict, list[str]]:
    path = SITE / "content" / lang / f"{slug}.md"
    if not path.is_file():
        raise SiteError(f"page manquante : {path.relative_to(ROOT)}")
    text = path.read_text(encoding="utf-8")
    m = re.match(r"---\n(.*?)\n---\n", text, re.S)
    if not m:
        raise SiteError(f"{path.relative_to(ROOT)} : en-tête --- manquant")
    meta = {}
    for line in m.group(1).splitlines():
        key, _, value = line.partition(":")
        meta[key.strip()] = value.strip()
    for key in ("title", "description"):
        if not meta.get(key):
            raise SiteError(f"{path.relative_to(ROOT)} : « {key} » manquant")
    return meta, text[m.end() :].splitlines()


def load_strings(lang: str) -> dict:
    return json.loads((SITE / "i18n" / f"{lang}.json").read_text(encoding="utf-8"))


def nav(lang: str, slug: str, strings: dict) -> str:
    items = []
    for key, target in (("nav_home", "index"), ("nav_download", "download"), ("nav_guide", "guide"), ("nav_apps", "apps")):
        current = slug == target or (target == "guide" and slug in GUIDE)
        aria = ' aria-current="page"' if current else ""
        items.append(f'<li><a href="{target}.html"{aria}>{strings[key]}</a></li>')
    items.append(f'<li><a href="{GITHUB}/wiki" rel="noopener">{strings["nav_wiki"]}</a></li>')
    return "".join(items)


def lang_switch(lang: str, slug: str, names: dict) -> str:
    links = []
    for other in LANGS:
        cur = ' aria-current="true"' if other == lang else ""
        links.append(
            f'<a href="../{other}/{slug}.html" hreflang="{other}" lang="{other}" data-lang="{other}"{cur}>'
            f"{names[other]['lang_short']}</a>"
        )
    return "".join(links)


def guide_nav(lang: str, slug: str, meta: dict, strings: dict) -> tuple[str, str]:
    side = [f'<p class="side-title"><a href="guide.html">{strings["nav_guide"]}</a></p><ol>']
    for n, s in enumerate(GUIDE, 1):
        cur = ' aria-current="page"' if s == slug else ""
        side.append(f'<li><a href="{s}.html"{cur}><span class="step-num">{n}</span>{html.escape(meta[s]["title"])}</a></li>')
    side.append("</ol>")
    idx = GUIDE.index(slug)
    pn = ['<nav class="prev-next" aria-label="' + strings["guide_pager"] + '">']
    if idx > 0:
        p = GUIDE[idx - 1]
        pn.append(f'<a class="prev" href="{p}.html"><span>{strings["prev"]}</span>{html.escape(meta[p]["title"])}</a>')
    else:
        pn.append(f'<a class="prev" href="guide.html"><span>{strings["prev"]}</span>{strings["nav_guide"]}</a>')
    if idx + 1 < len(GUIDE):
        n_ = GUIDE[idx + 1]
        pn.append(f'<a class="next" href="{n_}.html"><span>{strings["next"]}</span>{html.escape(meta[n_]["title"])}</a>')
    else:
        pn.append(f'<a class="next" href="apps.html"><span>{strings["next"]}</span>{html.escape(meta["apps"]["title"])}</a>')
    pn.append("</nav>")
    return "".join(side), "".join(pn)


def render_page(lang: str, slug: str, all_strings: dict, meta_all: dict) -> str:
    strings = all_strings[lang]
    meta, lines = read_page(lang, slug)
    r = Renderer(lang, slug, strings, meta_all[lang])
    body = r.blocks(lines)
    title = meta["title"]
    full_title = "Winlux" if slug == "index" else f"{title} · Winlux"
    base_url = CONFIG["url"].rstrip("/")
    alternates = "".join(
        f'<link rel="alternate" hreflang="{o}" href="{base_url}/{o}/{slug}.html">' for o in LANGS
    ) + f'<link rel="alternate" hreflang="x-default" href="{base_url}/">'

    if slug == "index":
        hero = f"""<section class="hero">
<div class="hero-inner">
<img class="hero-logo" src="../static/img/logo.svg" alt="" width="112" height="112">
<h1>{html.escape(meta.get("headline", title))}</h1>
<p class="lead">{r.inline(meta.get("lead", meta["description"]))}</p>
{r.download()}
<p class="hero-note">{r.inline(strings["preview_note"])}</p>
</div>
</section>"""
        main = f'{hero}<div class="home">{body}</div>'
        layout = "home"
    elif slug in GUIDE:
        side, pager = guide_nav(lang, slug, meta_all[lang], strings)
        step = GUIDE.index(slug) + 1
        main = f"""<div class="with-side">
<aside class="side" aria-label="{strings['nav_guide']}">{side}</aside>
<article class="prose">
<p class="eyebrow">{strings['guide_step'].format(n=step, total=len(GUIDE))}</p>
<h1>{html.escape(title)}</h1>
<p class="lead">{r.inline(meta['description'])}</p>
{body}
{pager}
</article>
</div>"""
        layout = "guide"
    else:
        main = f"""<article class="prose">
<h1>{html.escape(title)}</h1>
<p class="lead">{r.inline(meta['description'])}</p>
{body}
</article>"""
        layout = "page"

    s = strings
    return f"""<!doctype html>
<html lang="{lang}" data-page="{slug}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(full_title)}</title>
<meta name="description" content="{html.escape(meta['description'])}">
<meta name="color-scheme" content="dark light">
<meta name="theme-color" content="#070A1F">
<meta property="og:title" content="{html.escape(full_title)}">
<meta property="og:description" content="{html.escape(meta['description'])}">
<meta property="og:image" content="{base_url}/static/img/fond-nuit.jpg">
<meta property="og:type" content="website">
<link rel="canonical" href="{base_url}/{lang}/{slug}.html">
{alternates}
<link rel="icon" type="image/svg+xml" href="../static/img/logo.svg">
<link rel="icon" type="image/png" sizes="32x32" href="../static/img/favicon-32.png">
<link rel="stylesheet" href="../static/style.css">
<script src="../static/site.js" defer></script>
</head>
<body class="layout-{layout}" data-lang="{lang}" data-repo="{REPO}">
<a class="skip" href="#contenu">{s['skip']}</a>
<header class="topbar">
<div class="topbar-inner">
<a class="brand" href="index.html"><img src="../static/img/logo.svg" alt="" width="28" height="28"><span>Winlux</span></a>
<nav class="mainnav" aria-label="{s['nav_label']}"><ul>{nav(lang, slug, s)}</ul></nav>
<div class="langs" role="group" aria-label="{s['lang_label']}">{lang_switch(lang, slug, all_strings)}</div>
</div>
</header>
<main id="contenu" class="layout">
{main}
</main>
<footer class="footer">
<div class="footer-inner">
<div class="footer-brand"><img src="../static/img/logo.svg" alt="" width="24" height="24"> <strong>Winlux</strong></div>
<ul class="footer-links">
<li><a href="{GITHUB}" rel="noopener">{s['footer_code']}</a></li>
<li><a href="{GITHUB}/wiki" rel="noopener">{s['footer_wiki']}</a></li>
<li><a href="{GITHUB}/releases" rel="noopener">{s['footer_releases']}</a></li>
<li><a href="{GITHUB}/issues" rel="noopener">{s['footer_issues']}</a></li>
<li><a href="{GITHUB}/blob/master/LICENSE" rel="noopener">{s['footer_license']}</a></li>
<li><a href="legal.html">{s['footer_legal']}</a></li>
<li><a href="privacy.html">{s['footer_privacy']}</a></li>
</ul>
<p class="muted small">{r.inline(s['footer_disclaimer'])}</p>
</div>
</footer>
</body>
</html>
"""


def render_root(all_strings: dict) -> str:
    links = "".join(
        f'<li><a href="{l}/index.html" hreflang="{l}" lang="{l}" data-lang="{l}">'
        f"<strong>{all_strings[l]['lang_name']}</strong><span>{html.escape(all_strings[l]['root_tagline'])}</span></a></li>"
        for l in LANGS
    )
    base_url = CONFIG["url"].rstrip("/")
    alternates = "".join(f'<link rel="alternate" hreflang="{l}" href="{base_url}/{l}/index.html">' for l in LANGS)
    return f"""<!doctype html>
<html lang="fr" data-page="root">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Winlux</title>
<meta name="description" content="{html.escape(all_strings['en']['root_tagline'])}">
<meta name="color-scheme" content="dark light">
<link rel="canonical" href="{base_url}/">
{alternates}
<link rel="icon" type="image/svg+xml" href="static/img/logo.svg">
<link rel="stylesheet" href="static/style.css">
<script src="static/site.js" defer></script>
</head>
<body class="layout-root">
<main id="contenu" class="root-choose">
<img src="static/img/logo.svg" alt="" width="96" height="96">
<h1>Winlux</h1>
<ul class="root-langs">{links}</ul>
</main>
</body>
</html>
"""


def render_404(all_strings: dict) -> str:
    base_url = CONFIG["url"].rstrip("/")
    msgs = "".join(
        f'<li lang="{l}"><a href="{base_url}/{l}/index.html">{html.escape(all_strings[l]["not_found"])}</a></li>'
        for l in LANGS
    )
    return f"""<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>404 · Winlux</title>
<link rel="icon" type="image/svg+xml" href="{base_url}/static/img/logo.svg">
<link rel="stylesheet" href="{base_url}/static/style.css">
</head>
<body class="layout-root">
<main class="root-choose">
<img src="{base_url}/static/img/logo.svg" alt="" width="96" height="96">
<h1>404</h1>
<ul class="root-langs">{msgs}</ul>
</main>
</body>
</html>
"""


def build(out: Path) -> None:
    all_strings = {l: load_strings(l) for l in LANGS}
    meta_all = {l: {s: read_page(l, s)[0] for s in PAGES} for l in LANGS}
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    shutil.copytree(SITE / "static", out / "static")
    for lang in LANGS:
        (out / lang).mkdir()
        for slug in PAGES:
            (out / lang / f"{slug}.html").write_text(
                render_page(lang, slug, all_strings, meta_all), encoding="utf-8"
            )
    (out / "index.html").write_text(render_root(all_strings), encoding="utf-8")
    (out / "404.html").write_text(render_404(all_strings), encoding="utf-8")
    (out / ".nojekyll").write_text("", encoding="utf-8")
    sitemap = ['<?xml version="1.0" encoding="UTF-8"?>', '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">']
    base_url = CONFIG["url"].rstrip("/")
    for lang in LANGS:
        for slug in PAGES:
            sitemap.append(f"<url><loc>{base_url}/{lang}/{slug}.html</loc></url>")
    sitemap.append("</urlset>")
    (out / "sitemap.xml").write_text("\n".join(sitemap) + "\n", encoding="utf-8")


def main() -> int:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "build" / "site"
    try:
        build(out.resolve())
    except SiteError as exc:
        print(f"site : {exc}", file=sys.stderr)
        return 1
    print(f"Site construit dans {out} ({len(LANGS)} langues, {len(PAGES)} pages par langue).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
