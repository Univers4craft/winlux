# #2994 : le menu Démarrer ne s'ouvre pas par-dessus l'écran de verrouillage,
# même activé par D-Bus ; il s'ouvre de nouveau une fois le verrou levé.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1 && command -v xdotool >/dev/null 2>&1; then
    step "Menu Démarrer fermé tant que l'écran est verrouillé, sous Xvfb (#2994)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-start-menu-verrou.py
else
    skip "Menu Démarrer fermé tant que l'écran est verrouillé, sous Xvfb (#2994)" "mode rapide, xvfb-run ou xdotool absent"
fi
