# Le test des codes d'erreur du registre prend le verrou des tests du
# registre, sous lequel d'autres tests changent WINLUX_REGISTRY_PATH (#2632).
_s = (ROOT / "runtime/src/dlls/conformite_codes_erreur.rs").read_text()
_i = _s.index("fn codes_d_erreur_des_modules_du_texte_et_du_registre")
ok("crate::registry::TEST_LOCK" in _s[_i:_i + 600],
   "test des codes du registre sous le verrou du registre (#2632)")
