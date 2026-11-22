# #3066 : vue des tâches, grille des fenêtres et bande des bureaux dans l'écran.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 \
        && command -v openbox >/dev/null 2>&1 && command -v wmctrl >/dev/null 2>&1; then
    step "Vue des tâches : grille et bande des bureaux dans l'écran, sous Xvfb (#3066)" xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-grille.py
else
    skip "Vue des tâches : grille et bande des bureaux dans l'écran, sous Xvfb (#3066)" "mode rapide, xvfb-run, xdotool, openbox ou wmctrl absent"
fi
