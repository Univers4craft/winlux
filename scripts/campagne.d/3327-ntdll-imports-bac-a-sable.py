# ntdll exporte dans sa table les six imports que le bac à sable de Chromium
# (BSD, InitGlobalNt) résout et qui manquaient : NtSignalAndWaitForSingleObject,
# RtlCreateUserThread, _strnicmp, strlen, wcslen, memcpy (Acrobat, i19, #3327).
nt_3327 = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
ok(all(f'"{n}" => fn' in nt_3327 for n in [
       "NtSignalAndWaitForSingleObject", "RtlCreateUserThread",
       "_strnicmp", "strlen", "wcslen", "memcpy"])
   and "fn imports_ntdll_du_bac_a_sable_3327" in nt_3327,
   "ntdll : imports du bac à sable dans la table d'exports (#3327)")
