# ntdll exporte NtQueryInformationToken (mingw-w64 ntdll.def.in ; Learn,
# ntifs.h) : 28e interception cherchée par le courtier du bac à sable
# d'Acrobat (passe i18, #3310).
nt_3310 = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
ok('"NtQueryInformationToken" => fn nt_query_information_token' in nt_3310
   and "fn nt_query_information_token_lit_le_jeton_3310" in nt_3310,
   "ntdll : NtQueryInformationToken exportée et testée (#3310)")
