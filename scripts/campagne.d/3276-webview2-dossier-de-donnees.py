# WebView2 : dossier de données calculé comme sous Windows et rendu par
# ICoreWebView2Environment7::get_UserDataFolder (#3276).
_wv_3276 = (ROOT / "runtime/src/dlls/webview2.rs").read_text(encoding="utf-8")
ok("fn dossier_de_donnees_3276" in _wv_3276,
   "WebView2 : UserDataFolder rend le dossier réellement utilisé (#3276)")
