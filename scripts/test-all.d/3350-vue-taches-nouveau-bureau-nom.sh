# #3350 : vue des tâches, « Déplacer vers › Nouveau bureau » nomme le bureau créé.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 \
        && command -v openbox >/dev/null 2>&1 && command -v wmctrl >/dev/null 2>&1; then
    step "Vue des tâches : nom du bureau créé par Déplacer vers, sous Xvfb (#3350)" xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-nouveau-bureau.py
else
    skip "Vue des tâches : nom du bureau créé par Déplacer vers, sous Xvfb (#3350)" "mode rapide, xvfb-run, xdotool, openbox ou wmctrl absent"
fi
