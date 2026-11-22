# #2815 : le CRT exporte la classification multioctet _ismbb* et _ismbc*
# (et leurs variantes _l), avec les plages de la page 932 décrites sur Learn.
# Comportement gardé par le test crt_mbs::tests::classification_ismbb_ismbc.
mbs_2815 = (ROOT / "runtime" / "src" / "dlls" / "crt_mbs.rs").read_text(encoding="utf-8")
tests_2815 = (ROOT / "runtime" / "src" / "dlls" / "crt_mbs" / "tests.rs").read_text(encoding="utf-8")
noms_2815 = ["ismbbalnum", "ismbbkana", "ismbbtrail", "ismbchira", "ismbckata",
             "ismbcl0", "ismbcl1", "ismbcl2", "ismbclegal", "ismbcsymbol"]
ok(all(f'"_{n}"' in mbs_2815 and f'"_{n}_l"' in mbs_2815 for n in noms_2815)
   and "fn classification_ismbb_ismbc" in tests_2815,
   "CRT : classification _ismbb*/_ismbc* exportée et testée (#2815)")
