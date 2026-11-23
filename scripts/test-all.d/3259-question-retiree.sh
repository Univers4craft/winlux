# #3259 : la question « … ne répond pas » se retire quand le programme meurt ou
# que sa fenêtre passe à un autre processus, sans rien tuer ni annoncer « fermé ».
if [ "$quick" != 1 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Question « ne répond pas » retirée avec le programme (#3259)" xvfb-run -a -s '-screen 0 1920x1080x24' /usr/bin/python3 -B scripts/test-force-close.py
else
    skip "Question « ne répond pas » retirée avec le programme (#3259)" "xvfb-run absent ou mode rapide"
fi
