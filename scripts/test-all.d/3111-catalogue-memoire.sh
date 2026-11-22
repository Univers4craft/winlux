# #3111 : le démon du catalogue garde une mémoire stable de passe en passe
# avec un gros registre (la passe refresh() tourne dans un processus fils).
step "Catalogue des applications : mémoire du démon stable de passe en passe (#3111)" /usr/bin/python3 -B scripts/test-catalogue-memoire.py
