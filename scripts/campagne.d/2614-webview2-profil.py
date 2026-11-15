# #2614 : get_Profile rend un ICoreWebView2Profile lisible.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn profil_de_la_vue_2614" in wv, "test du profil WebView2 (#2614)")
