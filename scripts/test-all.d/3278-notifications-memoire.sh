# #3278 : le démon de notifications ne grossit plus à chaque notification.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Démon de notifications : mémoire stable sur 2 000 notifications, sous Xvfb (#3278)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-notifications-memoire.py
else
    skip "Démon de notifications : mémoire stable sur 2 000 notifications, sous Xvfb (#3278)" "mode rapide ou xvfb-run absent"
fi
