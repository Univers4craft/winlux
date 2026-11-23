# ntdll exporte NtOpenMutant (mingw-w64 ntdll.def.in) : le courtier du bac à
# sable d'Acrobat la cherche dans la table d'exports de ntdll (#3252).
ntdll_3252 = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
ok('"NtOpenMutant" => fn nt_open_mutant' in ntdll_3252
   and "fn nt_open_mutant_ouvre_un_mutex_nomme_3252" in ntdll_3252,
   "ntdll : NtOpenMutant exportée et testée (#3252)")
