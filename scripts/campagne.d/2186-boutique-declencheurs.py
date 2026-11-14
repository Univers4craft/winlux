# #2186 : déclencheurs de dpkg annoncés sans pourcentage, clic pendant une
# opération expliqué dans le bandeau ; vérifiés par test-store-declencheurs.py et,
# sous Xvfb avec de vrais clics, test-store-bandeau-xvfb.py (CI).
_s = (ROOT / "scripts/winlux-store.py").read_text(encoding="utf-8")
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok(_s.count('"apt_triggers":') == 4, "Boutique : message des déclencheurs de dpkg en quatre langues (#2186)")
ok((ROOT / "scripts/test-store-declencheurs.py").is_file() and "test-store-declencheurs.py" in _ci,
   "Boutique : fin d'installation expliquée testée en CI (#2186)")
ok((ROOT / "scripts/test-store-bandeau-xvfb.py").is_file() and "test-store-bandeau-xvfb.py" in _ci,
   "Boutique : second clic pendant une installation expliqué à l'écran, testé sous Xvfb (#2186)")
