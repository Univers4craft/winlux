# #1758 : FCI/FDI de cabinet.dll sont __cdecl ; FDINOTIFICATION x86 pour FDICopy.
wow = (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8")
cab = (ROOT / "runtime/src/dlls/cabinet_lz.rs").read_text(encoding="utf-8")
# Nom du test conventions_d_appel, qui vérifie FDICopy en cdecl (#2711).
ok('n == "cabinet"' in wow and "fn conventions_d_appel" in wow and '"#20"' in wow,
   "FCI/FDI de cabinet.dll en cdecl pour les programmes x86, par nom et par ordinal (#1758)")
ok("fn x86_notification(" in cab and "call_notify(notify, FDINT_COPY_FILE" in cab,
   "FDICopy passe une FDINOTIFICATION à la disposition x86 (#1758)")
