# #1842 : FormatMessage en anglais (LANG_ENGLISH) donne aussi un texte aux
# HRESULT de COM, et écrit les autres en hexadécimal.
k32_misc = (runtime_dlls / "kernel32_misc.rs").read_text(encoding="utf-8")
ok("0x8002_801D => \"Type library not registered.\"" in k32_misc
   and "Windows error 0x{code:08X}." in k32_misc,
   "kernel32 : textes anglais des HRESULT de COM, repli hexadécimal (#1842)")
