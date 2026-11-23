# #3341 : le service org.freedesktop.ScreenSaver ne grossit plus à chaque appel D-Bus.
step "Veille de l'écran : mémoire stable sur 4 000 appels D-Bus (#3341)" dbus-run-session --config-file=scripts/test-session-bus.conf -- python3 -B scripts/test-veille-dbus-memoire.py
