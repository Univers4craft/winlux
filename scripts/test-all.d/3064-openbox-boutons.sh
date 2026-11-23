# #3064 : boutons Réduire et Agrandir selon le style Win32, sous l'openbox de Winlux.
if test -x build/openbox/openbox || sh scripts/build-openbox.sh build/openbox >/dev/null 2>&1; then
    step "Boutons Réduire et Agrandir selon le style Win32 sous openbox (#3064)" /usr/bin/python3 -B scripts/test-openbox-boutons.py
else
    skip "Boutons Réduire et Agrandir selon le style Win32 sous openbox (#3064)" "openbox de Winlux non construit"
fi
