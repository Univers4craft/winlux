# #2858 : le menu Démarrer préchargé s'ouvre aussi vite avec 1 000 fichiers
# récents et 3 000 téléchargements qu'avec un dossier personnel vide (mesure
# sous Xvfb, médiane de cinq ouvertures).
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1; then
    step "Menu Démarrer : ouverture indépendante de l'historique, sous Xvfb (#2858)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-start-menu-ouverture.py
else
    skip "Menu Démarrer : ouverture indépendante de l'historique, sous Xvfb (#2858)" "mode rapide, xvfb-run ou xdotool absent"
fi
