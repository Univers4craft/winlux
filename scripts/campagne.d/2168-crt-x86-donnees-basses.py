# #2168 : données du CRT universel rendues par pointeur à un programme x86
# (___lc_locale_name_func, tmpnam(NULL), lignes de commande WinMain) servies
# sous 4 Go ; l'outil de déploiement d'Office plantait.
src = (ROOT / "runtime" / "src" / "wow32" / "crt.rs").read_text(encoding="utf-8")
ok('"___lc_locale_name_func" => raw(ret_aux, d + D_LC_NAMES)' in src
   and '"tmpnam" => native(n, func, "i>i", Fix::Str { wide: false })' in src,
   "wow32 : ___lc_locale_name_func et tmpnam rendent une donnée sous 4 Go (#2168)")
