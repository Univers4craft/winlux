# #996 : WINLUX_COMTRACE journalise chaque méthode DXGI/Direct3D 11
# appelée (table, case, arguments, valeur rendue, appelant).
step "Trace des méthodes COM (#996)" cargo test -q -p winlux -- trace_com_rend_les_valeurs_et_journalise
