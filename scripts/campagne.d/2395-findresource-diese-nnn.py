# FindResource : « #NNN » désigne l'identifiant entier NNN, en A comme en W (#2395).
_r = (ROOT / "runtime/src/dlls/resource.rs").read_text()
ok("fn find_resource_args" in _r and "fn find_resource_forme_diese_nnn" in _r,
   "FindResource lit la forme « #NNN » comme un identifiant (#2395)")
