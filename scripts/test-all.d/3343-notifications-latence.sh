# #3343 : Notify répond sans attendre l'écriture de l'historique ni la bulle.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Notifications : Notify répond aussitôt, historique plein et bulle affichée, sous Xvfb (#3343)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-notifications-latence.py
else
    skip "Notifications : Notify répond aussitôt, historique plein et bulle affichée, sous Xvfb (#3343)" "mode rapide ou xvfb-run absent"
fi
