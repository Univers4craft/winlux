# #3064 : Réduire et Agrandir suivent WS_MINIMIZEBOX et WS_MAXIMIZEBOX, grâce à
# l'openbox de Winlux. Le comportement est gardé par
# scripts/test-openbox-boutons.py (openbox réel sous Xvfb, CI « Tests runtime »).
# Ici : le correctif publié, et NOTICES qui déclare openbox modifié (GPL).
patches = sorted((ROOT / "integration" / "native" / "openbox").glob("*.patch"))
ok(any("Winlux" in p.read_text(encoding="utf-8") for p in patches),
   "Correctif openbox de Winlux publié dans integration/native/openbox (#3064)")
ok((ROOT / "scripts" / "test-openbox-boutons.py").exists(),
   "Test des boutons Réduire et Agrandir sous openbox réel présent (#3064)")
notices = (ROOT / "NOTICES").read_text(encoding="utf-8")
ok("Openbox modifié" in notices and "integration/native/openbox/" in notices,
   "NOTICES déclare l'openbox modifié et la source de ses correctifs (#3064)")
