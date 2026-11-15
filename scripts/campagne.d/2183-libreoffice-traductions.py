# #2183 : les six logiciels LibreOffice arrivent avec libreoffice-l10n-fr, -de et
# -es, que rien ne recommande ; désinstaller l'un ne les retire pas aux autres.
_c = (ROOT / "integration/store-catalog.txt").read_text(encoding="utf-8")
_comp = {l[11:].partition(":")[0]: l.partition(":")[2].split(",") for l in _c.splitlines() if l.startswith("compagnons=")}
ok(all({"libreoffice-l10n-fr", "libreoffice-l10n-de", "libreoffice-l10n-es"} <= set(_comp.get("libreoffice-" + a, []))
       for a in ("writer", "calc", "impress", "draw", "base", "math")),
   "Boutique : LibreOffice installé avec ses traductions française, allemande et espagnole (#2183)")
ok("test-store-libreoffice-traductions.py" in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"),
   "CI : traductions de LibreOffice (#2183)")
