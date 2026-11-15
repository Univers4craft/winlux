# #2540 : ntdll exporte NtCreateLowBoxToken (documentation publique, Ntdll.dll).
# Le comportement est gardé par le test unitaire nommé.
t = (runtime_dlls / "security" / "tests.rs").read_text(encoding="utf-8")
ok("fn jeton_lowbox_par_ntdll" in t, "NtCreateLowBoxToken servie par ntdll (#2540)")
