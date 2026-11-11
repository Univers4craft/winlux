# #1659 : WINLUX_SEH_TRACE journalise les types capturables reçus par RaiseException pour 0xE06D7363.
s = (runtime_dlls / "seh.rs").read_text(encoding="utf-8")
c = (runtime_dlls / "cxx_eh.rs").read_text(encoding="utf-8")
ok("exception C++ lancée depuis" in s and "fn thrown_type_names" in c,
   "le journal SEH nomme les types capturables d'une exception C++ reçue (#1659)")
e = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok("RoGetActivationFactory : classe « {name} »" in e,
   "RoGetActivationFactory journalise la classe WinRT refusée (#1659)")
