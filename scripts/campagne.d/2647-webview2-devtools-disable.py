# #2647 : Network.disable, Log.disable et Runtime.disable répondent par
# CallDevToolsProtocolMethod, comme les .enable.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn devtools_disable_2647" in wv, "test des commandes DevTools .disable (#2647)")
