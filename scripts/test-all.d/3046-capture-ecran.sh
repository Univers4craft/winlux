# #3046 : capture d'écran du bureau natif (zone tracée, plein écran, Échap),
# image au presse-papiers après la fermeture de l'outil, sous Xvfb.
if [ "$quick" = 0 ] && command -v xvfb-run >/dev/null 2>&1; then
    step "Capture d'écran : Windows + Maj + S et Impr. écran, sous Xvfb (#3046)" xvfb-run -a -s '-screen 0 1366x768x24' /usr/bin/python3 -B scripts/test-capture.py
else
    skip "Capture d'écran : Windows + Maj + S et Impr. écran, sous Xvfb (#3046)" "mode rapide ou xvfb-run absent"
fi
