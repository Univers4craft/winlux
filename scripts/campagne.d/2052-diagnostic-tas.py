# Diagnostic WINLUX_HEAP_DIAG (trace, nofree) pour les objets utilisés après libération (#2052).
# Vérifie les noms de l'analyse du réglage et du test qui en lit les valeurs.
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
ok("WINLUX_HEAP_DIAG" in _k and "fn heap_diag_parse(" in _k,
   "WINLUX_HEAP_DIAG reconnu (#2052)")
ok("fn valeurs_de_winlux_heap_diag(" in _k,
   "HeapFree : trace et mode nofree, avec test (#2052)")
