# #1902 : taille des cases de SAFEARRAY selon le programme.
step "SAFEARRAY : cases à la taille du programme (#1902)" cargo test -q -p winlux -- safearray_tests::
