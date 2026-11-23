# #3346 : vue des tâches, une fenêtre sur tous les bureaux figure sur chacun.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 \
        && command -v openbox >/dev/null 2>&1 && command -v wmctrl >/dev/null 2>&1; then
    step "Vue des tâches : fenêtre sur tous les bureaux, sous Xvfb (#3346)" xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-tous-bureaux.py
else
    skip "Vue des tâches : fenêtre sur tous les bureaux, sous Xvfb (#3346)" "mode rapide, xvfb-run, xdotool, openbox ou wmctrl absent"
fi
