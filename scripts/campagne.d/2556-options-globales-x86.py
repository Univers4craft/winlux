# #2556 : IGlobalOptions est servie aux programmes x86 (Google Update la crée
# au démarrage pour COMGLB_EXCEPTION_HANDLING) ; com32 l'appelle.
ole = (ROOT / "runtime/src/wow32/com_ole.rs").read_text(encoding="utf-8")
c32 = (ROOT / "testc/wow32/com32.c").read_text(encoding="utf-8")
ok('name: "IGlobalOptions"' in ole
   and "fn options_globales_decrites_pour_le_code_x86" in ole
   and "IGlobalOptions::Set" in c32,
   "wow32 : IGlobalOptions servie aux programmes 32 bits (#2556)")
