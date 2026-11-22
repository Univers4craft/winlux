# #2736 : window.open sans options ne donne pas une fenêtre de 100x100.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Fenêtre surgissante WebView2 à la taille demandée ou par défaut, sous Xvfb (#2736)" env NO_AT_BRIDGE=1 xvfb-run -a python3 -B scripts/test-webview-fenetre-surgissante.py
else
    step "Taille par défaut d'une fenêtre surgissante WebView2 (#2736)" python3 -B scripts/test-webview-fenetre-surgissante.py
fi
