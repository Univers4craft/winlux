# kernel32 : RaiseException, dont le relais est construit hors du binaire,
# figure dans la table d'exports de l'image (#3348).
_pe_3348 = (ROOT / "runtime/src/dlls/pe_image.rs").read_text(encoding="utf-8")
ok("fn raise_exception_dans_la_table_3348" in _pe_3348,
   "kernel32 : RaiseException dans la table d'exports de l'image (#3348)")
