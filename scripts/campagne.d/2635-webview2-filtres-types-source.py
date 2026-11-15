# #2635 : les filtres WebResourceRequested avec types de source
# (ICoreWebView2_22) se retiennent et se retirent.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn filtres_de_requetes_2635" in wv, "test des filtres de requêtes WebView2 (#2635)")
