# AssocCreateForClasses lit ASSOCIATIONELEMENT à la disposition de
# l'appelant : 12 octets en x86, 24 en x64 (#2475). Le comportement est
# vérifié par le test unitaire nommé ci-dessous ; ce contrôle vérifie les noms.
_s = (ROOT / "runtime/src/dlls/shell32_releve.rs").read_text()
_c = (ROOT / "runtime/src/wow32/com.rs").read_text()
ok("fn association_layout(" in _s and "fn associations_a_la_disposition_de_l_appelant()" in _s,
   "ASSOCIATIONELEMENT lu à la disposition de l'appelant (#2475)")
ok('("shell32", "AssocCreateForClasses")' in _c,
   "AssocCreateForClasses décrite pour les programmes x86 (#2475)")
