# #2378 : la base des tables d'essai exception_ptr est une page sous la plus
# basse adresse ; une RVA nulle (pas de destructeur) faisait échouer les tests.
src = (ROOT / "runtime/src/dlls/msvcp140_eptr/tests.rs").read_text(encoding="utf-8")
ok("let base = (adr.min(copie).min(dtor) & !0xFFF) - 0x1000;" in src,
   "tests exception_ptr : aucune RVA nulle dans les tables d'essai (#2378)")
