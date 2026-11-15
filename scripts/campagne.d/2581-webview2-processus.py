# #2581 : GetProcessInfos et GetProcessExtendedInfos rendent une collection.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn processus_du_moteur_2581" in wv, "test des processus du moteur WebView2 (#2581)")
