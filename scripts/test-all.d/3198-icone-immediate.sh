# #3198 : l'icône d'une fenêtre .exe est posée dès son apparition (réveil par
# _NET_CLIENT_LIST) et servie par le cache décodé pour une application déjà vue.
step "Catalogue : icône d'une fenêtre posée dès son apparition, cache décodé (#3198)" /usr/bin/python3 -B scripts/test-icone-immediate.py
