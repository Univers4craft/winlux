# #2643 : une commande refusée par CallDevToolsProtocolMethod est notée par
# son nom dans le journal, sans ses paramètres. Le comportement est gardé
# par le test unitaire du canal DevTools.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn cdp_refused(" in wv and "cdp_refusals()).contains(\"Debugger.enable\")" in wv,
   "WebView2 : commande DevTools refusée notée par son nom (#2643)")
