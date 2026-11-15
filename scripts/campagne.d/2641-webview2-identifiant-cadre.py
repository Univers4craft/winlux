# #2641 : get_FrameId (ICoreWebView2_20) rend un identifiant de cadre
# non nul et stable par vue.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn identifiant_de_cadre_2641" in wv, "test de l'identifiant de cadre WebView2 (#2641)")
