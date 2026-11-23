# L'Explorateur ouvert au repos dort jusqu'au prochain message ou tic au lieu
# de se réveiller ~100 fois par seconde (#3170). Le comportement est mesuré
# sous Xvfb par scripts/test-explorateur-repos-xvfb.py (étape de test-all).
etape_3170 = ROOT / "scripts/test-all.d/3170-explorateur-repos.sh"
temoin_3170 = ROOT / "scripts/test-explorateur-repos-xvfb.py"
print("#3170 : étape", etape_3170.exists(), "témoin", temoin_3170.exists())
ok(etape_3170.exists() and temoin_3170.exists()
   and "test-explorateur-repos-xvfb.py" in etape_3170.read_text(encoding="utf-8")
   and "rate < 20" in temoin_3170.read_text(encoding="utf-8"),
   "Explorateur au repos : moins de 20 réveils par seconde, mesuré sous Xvfb (#3170)")
