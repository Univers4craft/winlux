# #2651 : WinHttpOpen, WinHttpConnect et WinHttpOpenRequest qui rendent un
# handle remettent le dernier code d'erreur à 0 (l'outil de déploiement
# d'Office lisait le 122 d'un GetTokenInformation antérieur).
w = (ROOT / "runtime/src/dlls/winhttp.rs").read_text(encoding="utf-8")
ok("fn handle_rendu_efface_la_derniere_erreur" in w,
   "WinHTTP : un handle rendu efface la dernière erreur (#2651)")
