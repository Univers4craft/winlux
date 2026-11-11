# #1641 : l'opcode IL jmp (saut en queue) est exécuté par le CLR Winlux
# (Corsair.Service.exe d'iCUE 4.18 s'arrêtait sur InvalidProgramException).
clr_il = (runtime_dlls.parent / "clr" / "il.rs").read_text(encoding="utf-8")
ok("O::Jmp => {" in clr_il and "Jmp n'est pas encore" not in clr_il
   and "jmp_saute_vers_la_methode_cible" in (runtime_dlls.parent / "clr" / "tests.rs").read_text(encoding="utf-8"),
   "CLR : opcode jmp, la cible reçoit les arguments de la méthode quittée (#1641)")
