# #3257 : LCMapStringW/Ex suivent Learn : drapeaux vérifiés, tampon trop
# petit refusé (122), clé de tri en octets dans l'ordre de CompareString.
step "LCMapString : drapeaux, tampon trop petit et clé de tri (#3257)" cargo test -q -p winlux -- lcmapstring_suit_learn_3257
