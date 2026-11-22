# WebView2 : WebMessageReceived.AdditionalObjects rend une collection (#2979).
w = (ROOT / "runtime" / "src" / "dlls" / "webview2.rs").read_text(encoding="utf-8")
ok("fn objets_joints_aux_messages_2979" in w, "WebView2 : objets joints aux messages servis par AdditionalObjects (#2979)")
