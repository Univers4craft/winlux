# #2854 : MessageBox affiche l'icône de son type MB_ICON* (vraie boîte sous Xvfb).
if command -v xvfb-run >/dev/null 2>&1; then
    step "MessageBox : icône MB_ICONERROR peinte, sous Xvfb (#2854)" xvfb-run -a -s '-screen 0 1024x768x24' sh scripts/test-messagebox-icone.sh
else
    skip "MessageBox : icône MB_ICONERROR peinte, sous Xvfb (#2854)" "xvfb-run absent"
fi
