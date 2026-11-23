# #3315 : un panneau de la barre des tâches fermé ne réveille plus son processus
# chaque seconde ; le rafraîchissement repart à l'ouverture.
step "Panneaux de la barre des tâches : aucun réveil panneau fermé (#3315)" /usr/bin/python3 -B scripts/test-popup-repos.py
