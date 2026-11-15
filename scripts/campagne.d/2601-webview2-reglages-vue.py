# #2601 : les réglages de vue ICoreWebView2Settings3 à 9 se lisent et s'écrivent.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn reglages_de_vue_3_a_9_2601" in wv, "test des réglages de vue WebView2 3 à 9 (#2601)")
