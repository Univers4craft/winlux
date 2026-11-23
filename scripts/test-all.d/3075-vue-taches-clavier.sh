# #3075 : vue des tâches au clavier (flèches, Entrée, Suppr, Tab).
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1 \
        && command -v openbox >/dev/null 2>&1 && command -v wmctrl >/dev/null 2>&1; then
    step "Vue des tâches au clavier, sous Xvfb (#3075)" xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-taskview-clavier.py
else
    skip "Vue des tâches au clavier, sous Xvfb (#3075)" "mode rapide, xvfb-run, xdotool, openbox ou wmctrl absent"
fi
