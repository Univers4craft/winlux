# #3347 : vue des tâches, elle suit les fenêtres ouvertes ou fermées.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 \
        && command -v openbox >/dev/null 2>&1 && command -v wmctrl >/dev/null 2>&1; then
    step "Vue des tâches : fenêtres ouvertes ou fermées pendant la vue, sous Xvfb (#3347)" xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-a-jour.py
else
    skip "Vue des tâches : fenêtres ouvertes ou fermées pendant la vue, sous Xvfb (#3347)" "mode rapide, xvfb-run, xdotool, openbox ou wmctrl absent"
fi
