# QISearch lit QITAB à la disposition de l'appelant (8 octets en x86, 16 en
# x64) et appelle l'AddRef x86 d'un objet x86 (#2476). Le comportement est
# vérifié par le test unitaire nommé ci-dessous ; ce contrôle vérifie les noms.
_s = (ROOT / "runtime/src/dlls/shell_ordinaux.rs").read_text()
ok("fn qitab_layout(" in _s and "fn find_entry(" in _s and "fn table_qitab_x86()" in _s,
   "QITAB lue à la disposition de l'appelant (#2476)")
