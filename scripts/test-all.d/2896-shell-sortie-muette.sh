# #2896 : applications du shell et bureau sans lignes de test sur stdout ;
# la démo garde les siennes.
if command -v xvfb-run >/dev/null 2>&1 && [ -f target/x86_64-pc-windows-msvc/release/gui_demo.exe ]; then
    step "Applications du shell : aucune ligne de test sur stdout, sous Xvfb (#2896)" xvfb-run -a -s '-screen 0 1920x1080x24' python3 -B scripts/test-shell-sortie-muette-xvfb.py
else
    skip "Applications du shell : aucune ligne de test sur stdout, sous Xvfb (#2896)" "xvfb-run ou gui_demo.exe absent"
fi
