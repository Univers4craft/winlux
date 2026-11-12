# WMI GetNames en x86 : BSTR écrits dans des cases de la taille d'un pointeur du programme (#1926).
src = (ROOT / "runtime/src/dlls/wbem.rs").read_text()
ok("safe_array_get_elem_size(sa)" in src and "write_cell(" in src,
   "GetNames : BSTR au pas de SafeArrayGetElemsize, sans débordement en x86 (#1926)")
