# Diagnostic de pile : WINLUX_STACK_SCRUB remplit la pile morte au retour des API (#2120).
_t = (ROOT / "runtime/src/thunks.rs").read_text()
ok("fn stack_scrub_thunk" in _t and "0xF1F1_5C2B_F1F1_5C2B" in _t,
   "WINLUX_STACK_SCRUB : pont qui remplit la pile morte d'un motif non canonique (#2120)")
ok('std::env::var("WINLUX_STACK_SCRUB")' in _t and '"setjmp"' in _t and '"chkstk"' in _t,
   "WINLUX_STACK_SCRUB : jamais actif par défaut, setjmp et __chkstk exclus (#2120)")
_a = _t[_t.index("pub fn api_trace("):]
ok("stack_scrub(dll, func, target)" in _a[:400],
   "WINLUX_STACK_SCRUB : appliqué sous la trace d'API, qui garde l'appelant (#2120)")
_s = _t[_t.index("extern \"win64\" fn stack_scrub_start("):]
ok("rw_mapping_containing" in _s[:1500],
   "WINLUX_STACK_SCRUB : remplissage borné à la projection de la pile du thread (#2120)")
