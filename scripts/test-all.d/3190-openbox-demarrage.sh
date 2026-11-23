# #3190 : fenêtre affichée pendant le démarrage d'openbox, sous l'openbox de Winlux.
if test -x build/openbox/openbox || sh scripts/build-openbox.sh build/openbox >/dev/null 2>&1; then
    step "Fenêtre affichée pendant le démarrage d'openbox gérée (#3190)" /usr/bin/python3 -B scripts/test-openbox-demarrage.py
else
    skip "Fenêtre affichée pendant le démarrage d'openbox gérée (#3190)" "openbox de Winlux non construit"
fi
