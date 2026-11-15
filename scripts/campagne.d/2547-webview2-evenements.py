# #2547 : toute inscription à un événement WebView2 est acceptée.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn inscriptions_aux_evenements_acceptees_2547" in wv, "test des inscriptions aux événements WebView2 (#2547)")
