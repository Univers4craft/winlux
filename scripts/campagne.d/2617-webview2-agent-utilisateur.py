# #2617 : get_UserAgent rend l'agent du moteur, put_UserAgent("") ne change rien.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn agent_utilisateur_par_defaut_2617" in wv, "test de l'agent utilisateur WebView2 (#2617)")
