# ICoreWebView2Controller4 : AllowExternalDrop servi, le moteur refuse un
# dépôt venu d'une autre application quand il est faux (#3237).
_wv_3237 = (ROOT / "runtime/src/dlls/webview2.rs").read_text(encoding="utf-8")
ok("fn depot_externe_du_controleur_3237" in _wv_3237,
   "WebView2 : AllowExternalDrop du contrôleur servi (#3237)")
