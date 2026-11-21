# #1887 : GetSecurityDescriptorLength sur un descripteur absolu rend la
# taille de la structure plus celle des SID et ACL désignés, sans lire ses
# pointeurs comme des décalages. Comportement gardé par le test
# dlls::security::tests::longueur_d_un_descripteur_absolu.
tests_1887 = (ROOT / "runtime" / "src" / "dlls" / "security" / "tests.rs").read_text(encoding="utf-8")
ok("fn longueur_d_un_descripteur_absolu" in tests_1887,
   "GetSecurityDescriptorLength : descripteur absolu mesuré avec ses parties (#1887)")
