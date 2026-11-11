# NtQuerySection rend SECTION_BASIC_INFORMATION à la disposition x86 (16 octets) : steam.exe lit la vraie taille du flux partagé avec steamwebhelper (#1650).
ok("(16, 4, 8)" in (ROOT / "runtime/src/dlls/electron_win32.rs").read_text(encoding="utf-8"),
   "NtQuerySection connaît la disposition x86 (#1650)")
