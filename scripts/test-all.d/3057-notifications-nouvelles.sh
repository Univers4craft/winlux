# #3057 : pastille de la cloche = notifications nouvelles ; une bulle expirée
# ou reçue en Ne pas déranger la rallume, l'ouverture du centre l'éteint.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Notifications nouvelles : pastille rallumée, éteinte à l'ouverture du centre, sous Xvfb (#3057)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-notification-nouvelles.py
else
    skip "Notifications nouvelles : pastille rallumée, éteinte à l'ouverture du centre, sous Xvfb (#3057)" "mode rapide ou xvfb-run absent"
fi
