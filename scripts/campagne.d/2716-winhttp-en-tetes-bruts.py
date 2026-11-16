# #2716 : WinHttpQueryHeaders sert WINHTTP_QUERY_RAW_HEADERS (en-têtes
# séparés par \0), lu par l'outil de déploiement d'Office.
f = (runtime_dlls / "winhttp.rs").read_text(encoding="utf-8")
ok("fn en_tetes_bruts_separes_par_nul_2716" in f and "fn raw_headers_nul(" in f,
   "WinHTTP : en-têtes bruts séparés par \\0 rendus (#2716)")
