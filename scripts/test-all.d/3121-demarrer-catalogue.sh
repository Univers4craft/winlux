# #3121 : catalogue des applications relu sur signal de GIO, plus à chaque ouverture du menu Démarrer.
step "Démarrer : catalogue des applications sans relecture à l'ouverture (#3121)" xvfb-run -a -s '-screen 0 1366x768x24' dbus-run-session --config-file=scripts/test-session-bus.conf -- /usr/bin/python3 -B scripts/test-start-menu-catalogue.py
