# WebView2 : NavigationStarting.RequestHeaders lit les en-têtes de la navigation (#2964).
e = (ROOT / "runtime" / "src" / "dlls" / "webview2_entetes.rs").read_text(encoding="utf-8")
ok("fn entetes_de_navigation_lus_2964" in e, "WebView2 : en-têtes de la navigation lus par RequestHeaders (#2964)")
