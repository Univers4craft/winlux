# Tas façon LFH : bloc libéré rendu intact à la même classe de taille (#2067).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("fn lfh_alloc" in _k and "fn lfh_free" in _k and 'has("lfh")' in _k,
   "WINLUX_HEAP_DIAG=lfh : blocs libérés gardés par classe de taille (#2067)")
_f = _k[_k.index('pub unsafe extern "win64" fn heap_free('):]
_f = _f[:_f.index("\n}\n")]
ok("lfh_free(p, old)" in _f, "HeapFree : passe par la réserve lfh dans ce mode (#2067)")
