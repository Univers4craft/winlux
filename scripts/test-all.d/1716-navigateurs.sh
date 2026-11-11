# #1716 : Brave, Opera, Opera GX, Tor Browser (signature épinglée) et Vivaldi au choix du navigateur.
step "navigateurs proposés : téléchargement vérifié et installation (#1716)" sh -c "python3 -B scripts/test-browser-fetch.py && sh scripts/test-browsers-autres.sh"
