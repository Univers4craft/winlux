# #1669 : une cible WIC rend un bitmap par GetTarget et SetTarget le rétablit.
step "Direct2D : GetTarget et retour sur une cible WIC (#1669)" cargo test -q -p winlux -- cible_wic_get_target_puis_retour
