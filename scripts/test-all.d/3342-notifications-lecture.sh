# #3342 : une notification qui arrive pendant la lecture du centre ne refait
# pas la liste ni ne renvoie aux 50 premières.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Centre de notifications : la lecture reste en place quand une notification arrive, sous Xvfb (#3342)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-notification-lecture.py
else
    skip "Centre de notifications : la lecture reste en place quand une notification arrive, sous Xvfb (#3342)" "mode rapide ou xvfb-run absent"
fi
