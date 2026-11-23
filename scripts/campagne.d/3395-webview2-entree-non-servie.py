# WebView2 : une entrée non servie rend E_NOTIMPL en nommant sa famille et
# son rang dans le journal (#3395).
_wv_3395 = (ROOT / "runtime/src/dlls/webview2.rs").read_text(encoding="utf-8")
ok("fn entree_non_servie_nommee_3395" in _wv_3395,
   "WebView2 : une entrée non servie se nomme dans le journal (#3395)")
