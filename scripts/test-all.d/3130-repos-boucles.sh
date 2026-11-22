# #3130 : horloge, surveillance des entrées et du réseau, boucles de session
# ne lancent plus de programmes à chaque tour, bureau au repos.
step "Bureau au repos : petites boucles sans programme lancé (#3130)" python3 -B scripts/test-repos-boucles.py
