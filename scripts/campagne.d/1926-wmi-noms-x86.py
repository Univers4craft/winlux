# WMI GetNames en x86 : BSTR écrits dans des cases de la taille d'un pointeur du programme (#1926).
src = (ROOT / "runtime/src/dlls/wbem.rs").read_text()
ok("fn o_get_names(" in src and "safe_array_get_elem_size" in src and "write_cell" in src,
   "GetNames : BSTR au pas de SafeArrayGetElemsize, sans débordement en x86 (#1926)")
