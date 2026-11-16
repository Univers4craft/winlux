# #2736 : la taille d'une fenêtre surgissante ne vient de WebKit que si la
# page l'a demandée ; comportement gardé par scripts/test-webview-fenetre-surgissante.py.
c = (ROOT / "scripts" / "test-webview-fenetre-surgissante.py").read_text(encoding="utf-8")
ok("au lieu de {default}" in c, "window.open sans options : fenêtre à la taille par défaut (#2736)")
