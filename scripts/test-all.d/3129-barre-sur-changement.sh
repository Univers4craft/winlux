# #3129 : la barre des tâches n'est recalculée que sur changement (événements
# X, pactl subscribe, nmcli monitor), plus toutes les 2 s barre immobile.
step "Barre des tâches recalculée sur changement, pas toutes les 2 s (#3129)" python3 -B scripts/test-tray-changements.py
