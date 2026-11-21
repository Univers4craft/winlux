# Lancer une application installée ne pose aucune question ; Chromium sans bac à sable est confiné par Winlux (#1733).
perm = (ROOT / "runtime/src/permissions.rs").read_text(encoding="utf-8")
proc_ = (ROOT / "runtime/src/process.rs").read_text(encoding="utf-8")
ok("Capability::Camera | Capability::Microphone | Capability::Screen | Capability::Input" in perm
   and "fn lancement_sans_question" in perm,
   "lancement : ni bac à sable ni dossiers personnels demandés, caméra/micro/écran/saisie toujours demandés (#1733)")
# Nom : confine_chromium_child, définie et appelée (#2711).
ok("PR_SET_NO_NEW_PRIVS" in proc_ and "fn confine_chromium_child(" in proc_
   and proc_.count("confine_chromium_child(") >= 2
   and "processus enfants confinés par Winlux (#1733)" in proc_,
   "Chromium sans bac à sable : enfants confinés, état consigné au journal (#1733)")
