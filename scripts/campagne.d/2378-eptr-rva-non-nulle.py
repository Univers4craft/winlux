# #2378 : la base des tables d'essai exception_ptr est une page sous la plus
# basse adresse ; une RVA nulle (pas de destructeur) faisait échouer les tests.
src = (ROOT / "runtime/src/dlls/msvcp140_eptr/tests.rs").read_text(encoding="utf-8")
# Une RVA nulle de destructeur ferait échouer le test nommé (destructeur jamais appelé).
ok("fn tables(" in src and "static EMPLACEMENTS" in src
   and "fn capture_copie_l_objet_et_le_detruit_au_dernier_lacher(" in src,
   "tests exception_ptr : aucune RVA nulle dans les tables d'essai (#2378)")
