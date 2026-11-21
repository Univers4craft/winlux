# #2745 : fermer le Terminal Winlux termine son shell même quand bash perd le
# SIGHUP de xterm (saisie en cours). Banc sans X : pseudo-terminal et SIGHUP
# au groupe, comme xterm à la fermeture de sa fenêtre.
step "Terminal Winlux : la fermeture termine le shell, même occupé (#2745)" python3 -I scripts/test-terminal-fermeture.py
