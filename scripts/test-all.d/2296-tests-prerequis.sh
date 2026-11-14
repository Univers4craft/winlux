# #2296 : tests du contrôle parental, prérequis manquant nommé.
step "Tests du contrôle parental : prérequis manquant nommé (#2296)" sh -c 'for t in test-parental-ecran-gel.py test-parental-bulles.py; do WINLUX_TEST_PYGTK=/nonexistent/python3 python3 -B scripts/$t | grep -q "prérequis manquant" || exit 1; done'
