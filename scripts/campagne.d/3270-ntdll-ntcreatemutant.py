# ntdll exporte NtCreateMutant (mingw-w64 ntdll.def.in) : 14e interception
# cherchée par le courtier du bac à sable d'Acrobat (#3270).
ntdll_3270 = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
ok('"NtCreateMutant" => fn nt_create_mutant' in ntdll_3270
   and "fn nt_create_mutant_cree_un_mutex_3270" in ntdll_3270,
   "ntdll : NtCreateMutant exportée et testée (#3270)")
