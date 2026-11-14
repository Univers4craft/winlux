# #2184 : sur un système neuf, la Boutique télécharge les listes apt d'elle-même.
# À lancer sans les droits root.
step "Boutique : listes apt d'un système neuf (#2184)" python3 -B scripts/test-store-listes-neuves.py
