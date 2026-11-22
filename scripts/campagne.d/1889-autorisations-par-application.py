# Une décision d'autorisation vaut pour l'application (son dossier
# d'installation), pas pour un seul .exe : elle survit à une mise à jour
# Squirrel et couvre tous les .exe de Steam (#1889).
_p = (ROOT / "runtime" / "src" / "permissions.rs").read_text(encoding="utf-8")
ok("pub fn app_folder(" in _p and "fn stored_for_app(" in _p,
   "les autorisations sont mémorisées par application, d'après son dossier (#1889)")
ok(all(f"fn {t}" in _p for t in ("dossier_application_1889",
                                  "choix_garde_apres_mise_a_jour_squirrel_1889",
                                  "un_choix_vaut_pour_tous_les_exe_de_steam_1889",
                                  "programme_portable_reste_seul_1889")),
   "tests : mise à jour Squirrel, Steam multi-exe, programme portable seul (#1889)")
