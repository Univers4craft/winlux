# #1565 : le processus GPU de Claude ne meurt plus à l'agrandissement
# (connexions X des contextes GL gardées, cadres dans le bon contexte).
if command -v xvfb-run >/dev/null 2>&1; then
    step "Vue libérée après la fin de sa chaîne d'échange (#1565)" xvfb-run -a cargo test -q -p winlux -- --ignored --test-threads=1 vue_liberee_apres_la_fin_de_sa_chaine_x11 tampon_redimensionne_sous_un_autre_contexte_reste_presente_x11
fi
