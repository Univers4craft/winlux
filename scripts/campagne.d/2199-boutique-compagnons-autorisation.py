# #2199 : logiciel et compagnons sous une seule autorisation, attente annoncée,
# compagnons posés retirés avec le logiciel ; vérifié par
# test-store-compagnons-autorisation.py (CI, sans root).
_ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ok((ROOT / "scripts/test-store-compagnons-autorisation.py").is_file() and "test-store-compagnons-autorisation.py" in _ci,
   "Boutique : compagnons sous une seule autorisation testés en CI (#2199)")
