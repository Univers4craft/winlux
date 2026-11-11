# #1630 : WindowsCreateStringReference et CoIncrementMTAUsage écrivent des pointeurs de 4 octets en x86.
e = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
o = (runtime_dlls / "ole32.rs").read_text(encoding="utf-8")
ok("16 + crate::wow32::pointer_size()" in e and "fn set_header_text" in e,
   "HSTRING_HEADER de 20 octets en x86, texte sur 4 octets (#1630)")
ok("CO_MTA_USAGE_COOKIE est un pointeur" in o,
   "CoIncrementMTAUsage écrit un jeton de la taille d'un pointeur (#1630)")
