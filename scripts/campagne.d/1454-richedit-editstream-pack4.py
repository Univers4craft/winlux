# #1454 : EDITSTREAM est sous pack(4) ; en x64, pfnCallback est au décalage 12.
ok("const EDITSTREAM_CALLBACK: usize = 12;" in (runtime_dlls / "controls" / "richedit.rs").read_text(encoding="utf-8"),
   "RichEdit : rappel EDITSTREAM lu au décalage 12 en x64 (#1454)")
