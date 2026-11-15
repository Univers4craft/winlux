# #2613 : MemoryUsageTargetLevel (ICoreWebView2_19) se lit et s'écrit.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn niveau_memoire_2613" in wv, "test du niveau mémoire WebView2 (#2613)")
