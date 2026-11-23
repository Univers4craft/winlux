# #3190 : une fenêtre affichée pendant le démarrage d'openbox (juste après
# « wmctrl -m ») est gérée grâce à winlux-wm-reveil, lancé par les deux
# scripts de session.
step "Fenêtre affichée au démarrage d'openbox gérée (winlux-wm-reveil, #3190)" python3 -B scripts/test-wm-reveil.py
