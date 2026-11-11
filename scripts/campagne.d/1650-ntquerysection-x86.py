# Structures ntdll remplies pour un programme x86 à leur disposition 32 bits : steam.exe lit la vraie taille du flux partagé avec steamwebhelper (#1650).
_e = (ROOT / "runtime/src/dlls/electron_win32.rs").read_text(encoding="utf-8")
_n = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
ok("(16, 4, 8)" in _e and "query_section(h, class, info, len, ret, crate::dlls::legacy_com::x86())" in _e,
   "NtQuerySection : SECTION_BASIC_INFORMATION x86 de 16 octets (#1650)")
ok("crate::wow32::virtual_query32(" in _n and "28" in _n,
   "NtQueryVirtualMemory : MEMORY_BASIC_INFORMATION x86 de 28 octets (#1650)")
ok("let us = if x86 { 8 } else { 16 };" in _e and "us + 88" in _e,
   "NtQueryObject : UNICODE_STRING x86 de 8 octets et OBJECT_TYPE_INFORMATION de 0x60 (#1650)")
