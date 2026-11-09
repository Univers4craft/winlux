# #1352 : un renderer Chromium en bac à sable démarre et reste en vie.
ok('"AddMandatoryAce" => fn AddMandatoryAce' in (runtime_dlls / "security.rs").read_text(encoding="utf-8")
   and "fn table_des_handles_du_processus_courant_vide" in (runtime_dlls / "ntdll.rs").read_text(encoding="utf-8")
   and "privileges_du_jeton_et_ace_obligatoire" in (SCRIPTS / "test-all.d/1352-bac-a-sable-renderer.sh").read_text(encoding="utf-8"),
   "Bac à sable Chromium : AddMandatoryAce, TokenPrivileges et ProcessHandleTable (#1352)")
