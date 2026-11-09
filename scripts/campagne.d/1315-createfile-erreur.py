# #1315 : un CreateFileW réussi ne laisse pas l'erreur d'un appel précédent.
ok("fn create_file_reussi_remet_l_erreur_a_zero" in (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
   and "create_file_reussi_remet_l_erreur_a_zero" in (SCRIPTS / "test-all.d/1315-createfile-erreur.sh").read_text(encoding="utf-8"),
   "CreateFileW réussi : GetLastError à 0 (183 si le fichier existait) (#1315)")
