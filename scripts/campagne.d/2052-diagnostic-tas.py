# Diagnostic WINLUX_HEAP_DIAG (trace, nofree) pour les objets utilisés après libération (#2052).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text()
_f = _k[_k.index("pub unsafe extern \"win64\" fn heap_free("):]
_f = _f[:_f.index("\n}\n")]
ok("WINLUX_HEAP_DIAG" in _k and "fn heap_diag_parse" in _k,
   "WINLUX_HEAP_DIAG reconnu (#2052)")
ok("if !nofree" in _f and "HeapFree({p:p})" in _f,
   "HeapFree : trace et mode nofree (#2052)")
