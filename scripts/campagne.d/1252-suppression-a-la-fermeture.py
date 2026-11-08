# CreateFileW : FILE_FLAG_DELETE_ON_CLOSE et code 80 de CREATE_NEW ; Sumatra
# PDF portable garde ses réglages et son historique d'un lancement à l'autre.
ok("FILE_FLAG_DELETE_ON_CLOSE" in (runtime_dlls / "kernel32.rs").read_text()
   and "fichier_supprime_a_la_fermeture_et_create_new_sur_existant" in (SCRIPTS / "test-all.d" / "1252-suppression-a-la-fermeture.sh").read_text(),
   "CreateFileW : suppression à la fermeture et ERROR_FILE_EXISTS pour CREATE_NEW, Sumatra portable (#1252)")
