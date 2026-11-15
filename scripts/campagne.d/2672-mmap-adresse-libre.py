# Le test du double mappage W^X ne suppose plus qu'une adresse démappée
# reste libre : il en redemande une autre si un autre test l'a prise (#2672).
_s = (ROOT / "runtime/src/dlls/mmap.rs").read_text()
_t = _s[_s.index("fn double_mappage_w_xor_x_comme_coreclr"):]
ok("let mut libre = rx;" in _t and "aucune adresse libre obtenue" in _t,
   "test du double mappage : adresse libre redemandée (#2672)")
