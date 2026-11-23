# ntdll exporte NtQueryInformationAtom (mingw-w64 ntdll.def.in) : 27e
# interception cherchée par le courtier du bac à sable d'Acrobat (passe i17,
# #3304). Refus franc, comme les autres atomes (#3003).
at_3304 = (ROOT / "runtime/src/dlls/ntdll_atoms.rs").read_text(encoding="utf-8")
ok('"NtQueryInformationAtom" => fn nt_query_information_atom' in at_3304
   and "nt_query_information_atom(bad, bad, bad, bad, bad)" in at_3304,
   "ntdll : NtQueryInformationAtom exportée et testée (#3304)")
