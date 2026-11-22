# WebView2 : une navigation en 4xx finit sans succès (#2974).
w = (ROOT / "runtime" / "src" / "dlls" / "webview2.rs").read_text(encoding="utf-8")
ok("fn echec_sur_reponse_4xx_2974" in w, "WebView2 : IsSuccess FALSE sur une réponse 4xx (#2974)")
