# #2488 : msxml lit une chaîne d'entrée venue du programme jusqu'à son
# zéro, sans croire le préfixe de longueur et sans jamais fauter
# (getAttribute de l'outil de déploiement d'Office).
m = (ROOT / "runtime/src/dlls/msxml.rs").read_text(encoding="utf-8")
ok("fn input_units" in m
   and "fn chaine_du_programme_lue_sans_fauter" in m,
   "msxml : chaîne d'entrée lue jusqu'au zéro, sans faute (#2488)")
