# Boutique : LibreOffice arrive avec ses traductions fr/de/es, et une
# désinstallation remet les compagnons des logiciels restants (#2183).
_c = (ROOT / "integration/store-catalog.txt").read_text(encoding="utf-8")
_s = (SCRIPTS / "winlux-store.py").read_text(encoding="utf-8")
_comp = {l[11:].partition(":")[0]: l.partition(":")[2].split(",") for l in _c.splitlines() if l.startswith("compagnons=")}
ok(all({"libreoffice-l10n-fr", "libreoffice-l10n-de", "libreoffice-l10n-es"} <= set(_comp.get("libreoffice-" + a, []))
       for a in ("writer", "calc", "impress", "draw", "base", "math")),
   "Boutique : LibreOffice installé avec ses traductions française, allemande et espagnole (#2183)")
ok("for c in cs if c not in installed" in _s,
   "Boutique : après une désinstallation, les compagnons des logiciels restants sont remis (#2183)")
