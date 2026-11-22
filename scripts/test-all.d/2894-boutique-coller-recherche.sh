# #2894 : Ctrl+V colle le presse-papiers dans la recherche de la Boutique
# (shell PE sous Xvfb, pont du presse-papiers, vrai Ctrl+V par XTest).
if command -v xvfb-run >/dev/null 2>&1 && [ -f target/x86_64-pc-windows-msvc/release/gui_demo.exe ]; then
    step "Boutique : Ctrl+V colle dans la recherche, sous Xvfb (#2894)" xvfb-run -a -s '-screen 0 1920x1080x24' python3 -B scripts/test-store-coller-xvfb.py
else
    skip "Boutique : Ctrl+V colle dans la recherche, sous Xvfb (#2894)" "xvfb-run ou gui_demo.exe absent"
fi
