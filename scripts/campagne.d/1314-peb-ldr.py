# #1314 : PEB->Ldr posé et tenu à jour, lu par un autre processus (crashpad).
ok("crate::ldr::install(peb)" in (ROOT / "runtime/src/process.rs").read_text(encoding="utf-8")
   and "InLoadOrderModuleList commence par l'image du programme (#1314)" in (ROOT / "testexe/src/bin/chromium_compat.rs").read_text(encoding="utf-8")
   and "peb_ldr_liste_les_modules_dans_l_ordre_de_chargement" in (SCRIPTS / "test-all.d/1314-peb-ldr.sh").read_text(encoding="utf-8"),
   "PEB->Ldr : modules lisibles par le crashpad de Chromium (#1314)")
