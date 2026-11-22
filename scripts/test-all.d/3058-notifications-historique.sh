# #3058 : le centre de notifications relit tout l'historique gardé, par
# tranches au bas du défilement.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Centre de notifications : les 200 gardées se relisent en défilant, sous Xvfb (#3058)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-notification-historique.py
else
    skip "Centre de notifications : les 200 gardées se relisent en défilant, sous Xvfb (#3058)" "mode rapide ou xvfb-run absent"
fi
