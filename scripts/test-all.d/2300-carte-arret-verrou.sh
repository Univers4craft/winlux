# #2300 : sur un écran verrouillé, forcer l'arrêt exige le mot de passe.
step "Carte d'arrêt sur le verrou, Annuler et Déverrouiller (#2300)" xvfb-run -a python3 scripts/test-power-carte-verrou.py
