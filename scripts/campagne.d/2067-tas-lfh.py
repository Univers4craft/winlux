# Tas façon LFH : bloc libéré rendu intact à la même classe de taille (#2067).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("fn lfh_alloc(" in _k and "fn lfh_free(" in _k and "fn valeurs_de_winlux_heap_diag(" in _k,
   "WINLUX_HEAP_DIAG=lfh : blocs libérés gardés par classe de taille (#2067)")
ok("fn lfh_rend_le_bloc_libere_intact(" in _k, "HeapFree : passe par la réserve lfh dans ce mode, avec test (#2067)")
