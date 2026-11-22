# WebView2 : NavigationCompleted.HttpStatusCode rend le vrai code HTTP (#2966).
w = (ROOT / "runtime" / "src" / "dlls" / "webview2.rs").read_text(encoding="utf-8")
ok("fn code_http_de_navigation_2966" in w, "WebView2 : code HTTP de la navigation rendu par HttpStatusCode (#2966)")
