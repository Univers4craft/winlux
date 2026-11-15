# #2619 : WinHttpSetStatusCallback rend le rappel précédent au lieu de
# WINHTTP_INVALID_STATUS_CALLBACK (l'outil de déploiement d'Office fermait
# chaque requête avant l'envoi : 30183-2016, 0x80070032).
h = (ROOT / "runtime/src/dlls/winhttp.rs").read_text(encoding="utf-8")
c = (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8")
ok("fn rappel_d_etat_retenu_et_rendu" in h and "#2619" in c,
   "WinHTTP : rappel d'état retenu et rendu (#2619)")
