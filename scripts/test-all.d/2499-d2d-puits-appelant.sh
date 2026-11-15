# #2499 : le puits de géométrie appartient à l'appelant (pas de Close).
step "Direct2D : puits de géométrie laissé ouvert (#2499)" cargo test -q -p winlux -- geometrie_elargie groupe_transformation_simplify_et_stream
