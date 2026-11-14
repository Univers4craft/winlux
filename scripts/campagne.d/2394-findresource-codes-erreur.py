# FindResource : 1812/1813/1814/1815 selon le niveau manqué, et non 1813 partout (#2394).
_r = (ROOT / "runtime/src/dlls/resource.rs").read_text()
ok("fn walk_resource_tree" in _r and "fn find_resource_code_d_erreur_par_niveau" in _r
   and "fn find_resource_sans_section_rend_1812" in _r,
   "FindResource rend le code d'erreur du niveau manqué (#2394)")
