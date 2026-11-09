# #1433 : RichEdit lit et rend son texte par flux (RichTextBox de WinForms).
ok("fn stream_in" in (runtime_dlls / "controls" / "richedit.rs").read_text(encoding="utf-8"),
   "RichEdit : EM_STREAMIN et EM_STREAMOUT (#1433)")
