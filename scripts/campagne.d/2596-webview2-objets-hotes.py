# #2596 : AddHostObjectToScript retient l'objet hôte, RemoveHostObjectFromScript le rend.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn objets_hotes_2596" in wv, "test des objets hôtes WebView2 (#2596)")
