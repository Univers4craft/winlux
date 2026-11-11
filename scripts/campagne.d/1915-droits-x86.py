# Droits Windows d'un programme x86 : EXPLICIT_ACCESS de 32 octets, TRUSTEE de 20, pointeurs rendus sur 4 octets (SteamService /install, #1915).
_s = (ROOT / "runtime/src/dlls/security.rs").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/dlls/security/tests.rs").read_text(encoding="utf-8")
_g = (ROOT / "runtime/src/dlls/games_win32.rs").read_text(encoding="utf-8")
_e = (ROOT / "runtime/src/dlls/electron_win32.rs").read_text(encoding="utf-8")
ok("let e = entries.add(i * lay.size);" in _s and "entries.add(i * 48)" not in _s
   and "fn explicit_access_x86_de_32_octets" in _t,
   "SetEntriesInAcl lit les EXPLICIT_ACCESS x86 de 32 octets, avec test (#1915)")
ok("std::ptr::write_bytes(ea, 0, 48);" not in _g and "access_layout(ptr)" in _g
   and "std::ptr::write_bytes(trustee, 0, 32);" not in _e,
   "BuildExplicitAccessWithName et BuildTrustee aux tailles x86 (#1915)")
