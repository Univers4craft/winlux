# #2382 : Marshal.StructureToPtr marshale un champ délégué en pointeur de
# fonction natif ; Corsair.Service.exe (iCUE) passait lpServiceProc = 0 à
# StartServiceCtrlDispatcher.
ok("FnPtr" in (ROOT / "runtime" / "src" / "clr" / "marshal.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Rappels.cs").read_text(encoding="utf-8"),
   "CLR : champ délégué marshalé en pointeur de fonction (#2382)")
