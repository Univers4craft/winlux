# Rectangle sans plume (#1239) : rempli depuis (left, top), un pixel de moins
# à droite et en bas ; plus de ligne noire contre « G/D » des vumètres d'Audacity.
step "Rectangle sans plume rempli depuis le coin haut gauche (#1239)" cargo test -q -p winlux -- rectangle_sans_plume_commence_au_coin_haut_gauche
