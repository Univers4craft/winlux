# #2643 : CallDevToolsProtocolMethod et GetDevToolsProtocolEventReceiver
# (ICoreWebView2) servent le canal DevTools de base.
# Le comportement est gardé par le test unitaire nommé.
wv = (runtime_dlls / "webview2.rs").read_text(encoding="utf-8")
ok("fn canal_devtools_2643" in wv, "test du canal DevTools WebView2 (#2643)")
