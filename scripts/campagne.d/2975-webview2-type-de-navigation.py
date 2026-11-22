# WebView2 : NavigationStarting.NavigationKind rend le type de navigation (#2975).
w = (ROOT / "runtime" / "src" / "dlls" / "webview2.rs").read_text(encoding="utf-8")
ok("fn type_de_navigation_2975" in w, "WebView2 : type de navigation rendu par NavigationKind (#2975)")
