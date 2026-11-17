# #2779 : WinHttpOpen accepte WINHTTP_FLAG_ASYNC ; chaque opération d'une
# session asynchrone rend la main et annonce son achèvement par le rappel
# de WinHttpSetStatusCallback (installeur de Creative Cloud). Comportement
# gardé par le test dlls::winhttp::session_asynchrone_par_rappels_2779.
winhttp_2779 = (ROOT / "runtime" / "src" / "dlls" / "winhttp.rs").read_text(encoding="utf-8")
ok("fn session_asynchrone_par_rappels_2779" in winhttp_2779
   and "const STATUS_SENDREQUEST_COMPLETE" in winhttp_2779,
   "WinHTTP : session asynchrone servie par rappels (#2779)")
