# Démo de Winlux jouable dans le navigateur, sur le site (#2926).
import re
_demo = ROOT / "site" / "static" / "demo"
_js = (_demo / "demo.js").read_text(encoding="utf-8")
_css = (_demo / "demo.css").read_text(encoding="utf-8")
_page = (_demo / "index.html").read_text(encoding="utf-8")
# Les quatre langues du site ont les mêmes chaînes.
_blocs = {l: re.search(r"\n    %s: \{(.*?)\n    \}," % l, _js, re.S) for l in ("fr", "en", "de", "es")}
_cles = {l: set(re.findall(r'(?:^\s+|, )([a-z_0-9]+): "', b.group(1), re.M)) if b else set() for l, b in _blocs.items()}
ok(all(_cles.values()) and all(c == _cles["fr"] for c in _cles.values()),
   "démo : mêmes chaînes en français, anglais, allemand et espagnol (#2926)")
# Aucune marque affichée dans la démo ; seule la mention légale de la page la cite.
_hors_mention = re.sub(r'<p class="muted small">.*?</p>', "", _page, flags=re.S)
ok(not re.search(r"Windows", _js + _css + _hors_mention)
   and not re.search(r"microsoft", _js + _css + _hors_mention, re.I),
   "démo : ni « Windows » ni « Microsoft » hors de la mention légale (#2926)")
# Mêmes zones d'ancrage que la session native.
_snap = (SCRIPTS / "winlux-snap.py").read_text(encoding="utf-8")
_zones_snap = set(re.findall(r'"((?:half|wide|narrow|third|quarter):\w+)": \(', _snap))
_zones_demo = set(re.findall(r'"((?:half|wide|narrow|third|quarter):\w+)": \[', _js))
ok(_zones_snap and _zones_snap == _zones_demo,
   "démo : mêmes zones d'ancrage que winlux-snap (#2926)")
# Elle dit au visiteur que c'est une démonstration, et le site y mène.
ok('data-t="avis"' in _page and "pas Winlux en train de tourner" in _js,
   "démo : le bandeau dit que ce n'est pas le système qui tourne (#2926)")
ok("nav_demo" in (ROOT / "site" / "build.py").read_text(encoding="utf-8")
   and all('"nav_demo"' in (ROOT / "site" / "i18n" / f"{l}.json").read_text(encoding="utf-8") for l in ("fr", "en", "de", "es")),
   "site : lien « Démo » dans la navigation, quatre langues (#2926)")
