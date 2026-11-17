# #2792 : LookupAccountSidW lit les deux tailles d'entrée avant toute
# écriture ; une même variable passée pour les deux ne rend plus 122
# (Creative Cloud, « IsSystemProcess »). Comportement gardé par le test
# dlls::security::tests::tailles_partagees_de_lookup_account_sid_2792.
tests_2792 = (ROOT / "runtime" / "src" / "dlls" / "security" / "tests.rs").read_text(encoding="utf-8")
ok("fn tailles_partagees_de_lookup_account_sid_2792" in tests_2792,
   "LookupAccountSidW : tailles d'entrée lues avant écriture (#2792)")
