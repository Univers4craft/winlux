# #1902 : cases de SAFEARRAY à la taille du programme (VARIANT 24 octets en x64, pointeurs 4 en x86).
ole = (ROOT / "runtime/src/dlls/oleaut32.rs").read_text(encoding="utf-8")
ok("12 => 8 + 2 * pointer" in ole, "case VT_VARIANT de 24 octets en x64 et 16 en x86 (#1902)")
ok("VT_BSTR | 9 | 13 => pointer" in ole and "fn read_cell" in ole,
   "cases BSTR, IDispatch et IUnknown de 4 octets pour un programme x86 (#1902)")
ok("fn variants_espaces_de_24_octets_en_x64" in ole, "test des VARIANT espacés de 24 octets (#1902)")
