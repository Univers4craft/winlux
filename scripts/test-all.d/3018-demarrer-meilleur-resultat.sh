# #3018 : Meilleur résultat de la recherche du menu Démarrer, application ou
# page des Paramètres, sous Xvfb.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Menu Démarrer : Meilleur résultat, application ou page des Paramètres, sous Xvfb (#3018)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-start-search-meilleur.py
else
    skip "Menu Démarrer : Meilleur résultat, application ou page des Paramètres, sous Xvfb (#3018)" "mode rapide ou xvfb-run absent"
fi
