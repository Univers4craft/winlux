# ICoreWebView2Controller3 : RasterizationScale, ShouldDetectMonitorScaleChanges
# et BoundsMode servis et appliqués, RasterizationScaleChanged levé (#3231).
_wv_3231 = (ROOT / "runtime/src/dlls/webview2.rs").read_text(encoding="utf-8")
ok("fn echelle_de_rendu_du_controleur_3231" in _wv_3231,
   "WebView2 : échelle de rendu et BoundsMode du contrôleur servis (#3231)")
