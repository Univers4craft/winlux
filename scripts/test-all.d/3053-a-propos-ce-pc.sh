# #3053 : Paramètres › À propos de ce PC (en-tête, tuiles, démarrage mesuré),
# affichée aussitôt puis complétée hors du fil GTK.
if command -v xvfb-run >/dev/null 2>&1; then
    step "Paramètres : À propos de ce PC, démarrage mesuré, sans attendre le matériel (#3053)" xvfb-run -a -s '-screen 0 1366x768x24' python3 -B scripts/test-about-pc.py
else
    skip "Paramètres : À propos de ce PC, démarrage mesuré, sans attendre le matériel (#3053)" "xvfb-run absent"
fi
