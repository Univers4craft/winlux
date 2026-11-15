# #2542 : NtQueryInformationProcess(ProcessBasicInformation) accepte le tampon
# de 24 octets d'un programme 32 bits et y donne le pid du parent (Google Update).
n = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
w = (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8")
ok("fn informations_de_base_x86_avec_le_parent" in n
   and "fn informations_de_base_x64_avec_le_parent" in n
   and "pub fn peb32" in w,
   "ProcessBasicInformation x86 : 24 octets et pid du parent, testé (#2542)")
