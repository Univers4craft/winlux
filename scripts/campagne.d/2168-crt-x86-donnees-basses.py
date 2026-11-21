# #2168 : données du CRT universel rendues par pointeur à un programme x86
# (___lc_locale_name_func, tmpnam(NULL), lignes de commande WinMain) servies
# sous 4 Go ; l'outil de déploiement d'Office plantait.
src = (ROOT / "runtime" / "src" / "wow32" / "crt.rs").read_text(encoding="utf-8")
# Test unitaire noms_de_locale_x86_sous_4_go : six pointeurs x86 nuls, lisibles sous 4 Go.
ok('"___lc_locale_name_func"' in src and '"tmpnam"' in src and "const D_LC_NAMES" in src
   and "fn noms_de_locale_x86_sous_4_go(" in src,
   "wow32 : ___lc_locale_name_func et tmpnam rendent une donnée sous 4 Go (#2168)")
