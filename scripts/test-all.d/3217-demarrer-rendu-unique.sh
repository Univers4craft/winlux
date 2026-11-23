# #3217 : le menu Démarrer ne construit sa liste qu'une fois par ouverture.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Menu Démarrer : liste construite une fois par ouverture, sous Xvfb (#3217)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-start-menu-rendu-unique.py
else
    skip "Menu Démarrer : liste construite une fois par ouverture, sous Xvfb (#3217)" "mode rapide ou xvfb-run absent"
fi
