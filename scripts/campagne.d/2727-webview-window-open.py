# #2727 : window.open ouvre une fenêtre surgissante manipulable par la page
# (WebView2 sans NewWindowRequested traité). Le comportement est gardé par le
# test de bout en bout scripts/test-webview2.sh, étape « fenetre-dom-fermee ».
c = (ROOT / "testc" / "msvc" / "webview2_msvc.c").read_text(encoding="utf-8")
ok("fenetre-dom-fermee" in c, "window.open ouvre une fenêtre surgissante (#2727)")
