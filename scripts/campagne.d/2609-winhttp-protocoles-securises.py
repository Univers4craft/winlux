# #2609 : WinHttpSetOption accepte WINHTTP_OPTION_SECURE_PROTOCOLS (84) sur
# une session et en tient les bornes TLS (l'outil de déploiement d'Office
# n'envoyait jamais sa requête : 30183-2016, 0x80070032).
h = (ROOT / "runtime/src/dlls/winhttp.rs").read_text(encoding="utf-8")
ok("fn option_protocoles_securises_sur_une_session" in h and "fn secure_protocols" in h,
   "WinHTTP : protocoles sécurisés (84) acceptés sur une session (#2609)")
