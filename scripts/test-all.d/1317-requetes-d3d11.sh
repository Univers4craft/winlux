# #1317 : requêtes Direct3D 11 typées ; TIMESTAMP_DISJOINT rend une
# fréquence non nulle (division par zéro d'ANGLE, page blanche de Claude).
step "Requêtes Direct3D 11 d'horodatage (#1317)" cargo test -q -p winlux -- requetes_d_horodatage_remplies
