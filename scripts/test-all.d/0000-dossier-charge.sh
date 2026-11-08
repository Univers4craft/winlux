# Contrôle témoin de scripts/test-all.d : vérifie que les fichiers déposés
# voient step, $quick et la racine du dépôt.
step "scripts/test-all.d : step, \$quick et racine visibles" test -f scripts/test-all.sh -a -n "$quick"
