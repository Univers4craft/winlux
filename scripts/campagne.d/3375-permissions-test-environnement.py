# Les tests d'autorisations qui posent une question tiennent le verrou de
# l'environnement des tests : sinon WINLUX_SERVICE_NAME, posé par le test du
# service, faisait mémoriser un refus pendant sans_reponse_refus_apres_le_delai_1890
# (#3375).
_perm = (ROOT / "runtime" / "src" / "permissions.rs").read_text(encoding="utf-8")
ok("_garde: std::sync::MutexGuard<'static, ()>," in _perm,
   "tests d'autorisations sérialisés avec le test du service (#3375)")
