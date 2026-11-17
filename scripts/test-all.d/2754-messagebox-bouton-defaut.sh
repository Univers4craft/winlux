# #2754 : MessageBox, Entrée choisit le bouton MB_DEFBUTTON2/3 (vraie boîte sous Xvfb).
if command -v xvfb-run >/dev/null 2>&1; then
    step "MessageBox : Entrée choisit le bouton par défaut MB_DEFBUTTON2/3, sous Xvfb (#2754)" xvfb-run -a -s '-screen 0 1024x768x24' sh scripts/test-messagebox-bouton-defaut.sh
else
    skip "MessageBox : Entrée choisit le bouton par défaut MB_DEFBUTTON2/3, sous Xvfb (#2754)" "xvfb-run absent"
fi
