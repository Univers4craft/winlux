# #3076 : vue des tâches, glisser une fenêtre sur un autre bureau.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 \
        && command -v openbox >/dev/null 2>&1 && command -v wmctrl >/dev/null 2>&1; then
    step "Vue des tâches : glisser une fenêtre sur un bureau, sous Xvfb (#3076)" xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-glisser.py
else
    skip "Vue des tâches : glisser une fenêtre sur un bureau, sous Xvfb (#3076)" "mode rapide, xvfb-run, xdotool, openbox ou wmctrl absent"
fi
