# #2570 : ICoreWebView2Environment10 rend un objet de réglages de contrôleur.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn reglages_de_controleur_2570" in wv, "test des réglages de contrôleur WebView2 (#2570)")
