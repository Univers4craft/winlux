# #2606 : WinHttpSetOption accepte WINHTTP_OPTION_CONFIGURE_PASSPORT_AUTH (83)
# sur une session (l'outil de déploiement d'Office n'ouvrait plus de
# connexion : 30183-2016, 0x80070006).
h = (ROOT / "runtime/src/dlls/winhttp.rs").read_text(encoding="utf-8")
ok("fn option_passport_acceptee_sur_une_session" in h,
   "WinHTTP : option Passport (83) acceptée sur une session (#2606)")
