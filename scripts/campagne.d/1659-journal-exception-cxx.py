# #1659 : WINLUX_SEH_TRACE journalise les types capturables reçus par RaiseException pour 0xE06D7363.
s = (runtime_dlls / "seh.rs").read_text(encoding="utf-8")
c = (runtime_dlls / "cxx_eh.rs").read_text(encoding="utf-8")
ok("exception C++ lancée depuis" in s and "fn thrown_type_names" in c,
   "le journal SEH nomme les types capturables d'une exception C++ reçue (#1659)")
