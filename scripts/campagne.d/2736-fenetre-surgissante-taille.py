# #2736 : la taille d'une fenêtre surgissante ne vient de WebKit que si la
# page l'a demandée. Le comportement est gardé par le test Xvfb
# scripts/test-webview-fenetre-surgissante.py, lancé par l'étape test-all 2736.
etape = (ROOT / "scripts" / "test-all.d" / "2736-fenetre-surgissante-taille.sh").read_text(encoding="utf-8")
moteur = (ROOT / "scripts" / "winlux-webview.py").read_text(encoding="utf-8")
ok("test-webview-fenetre-surgissante.py" in etape and "def popup_default_size" in moteur,
   "window.open sans options : fenêtre à la taille par défaut (#2736)")
