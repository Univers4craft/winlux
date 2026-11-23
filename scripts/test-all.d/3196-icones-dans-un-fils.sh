# #3196 : les icônes des fenêtres .exe sont décodées dans un fils, seulement à
# l'apparition d'une fenêtre ; le démon du catalogue ne charge ni glycin ni X11.
step "Catalogue : icônes des fenêtres rendues hors du démon résident (#3196)" /usr/bin/python3 -B scripts/test-icones-fils.py
