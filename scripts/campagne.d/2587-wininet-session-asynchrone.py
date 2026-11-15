# #2587 : InternetOpen accepte INTERNET_FLAG_ASYNC (le transport HTTP de
# l'outil de déploiement d'Office n'avait pas de session : 30183-2016).
w = (ROOT / "runtime/src/dlls/wininet.rs").read_text(encoding="utf-8")
ok("fn session_asynchrone_aboutit_tout_de_suite" in w and "asynchrone non pris en charge" not in w,
   "wininet : session asynchrone ouverte, opérations menées tout de suite (#2587)")
