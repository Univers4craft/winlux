# Tas : WINLUX_HEAP_DIAG=fill remplit les blocs non mis à zéro d'un motif (#2156).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
ok("fn heap_fill_parse" in _k and "0xF2F2_5A11_F2F2_5A11" in _k,
   "WINLUX_HEAP_DIAG=fill : motif non canonique pour les blocs neufs (#2156)")
_a = _k[_k.index('pub unsafe extern "win64" fn heap_alloc('):]
_a = _a[:_a.index("\n}\n")]
ok("heap_fill()" in _a and "flags & 8" in _a and "drapeaux {flags:#x}" in _a,
   "HeapAlloc : motif seulement sans HEAP_ZERO_MEMORY, drapeaux journalisés (#2156)")
_r = _k[_k.index('pub unsafe extern "win64" fn heap_realloc('):]
_r = _r[:_r.index("\n}\n")]
ok("heap_fill_bytes" in _r, "HeapReAlloc : partie agrandie remplie du motif en mode fill (#2156)")
