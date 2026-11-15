# #2637 : get_CookieManager (ICoreWebView2_2) rend un gestionnaire de
# cookies qui crée, range, rend et retire les cookies.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn gestionnaire_de_cookies_2637" in wv, "test du gestionnaire de cookies WebView2 (#2637)")
