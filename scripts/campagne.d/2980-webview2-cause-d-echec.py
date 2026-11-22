# WebView2 : WebErrorStatus donne la cause de l'échec de navigation (#2980).
w = (ROOT / "runtime" / "src" / "dlls" / "webview2.rs").read_text(encoding="utf-8")
ok("fn cause_d_echec_de_navigation_2980" in w, "WebView2 : cause de l'échec de navigation dans WebErrorStatus (#2980)")
