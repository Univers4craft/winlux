# #3312 : la garde du bureau par l'init ne lance plus quatre processus par
# seconde ; « Se déconnecter » et « Redémarrer la session » restent servis.
step "Init au repos : un seul programme par seconde pour garder le bureau (#3312)" python3 -B scripts/test-init-repos.py
