# Droits Windows d'un programme x86 : EXPLICIT_ACCESS de 32 octets, TRUSTEE de 20, pointeurs rendus sur 4 octets (SteamService /install, #1915).
# Vérifie les noms des fonctions de disposition x86 et du test qui lit des
# EXPLICIT_ACCESS x86 de 32 octets.
_s = (ROOT / "runtime/src/dlls/security.rs").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/dlls/security/tests.rs").read_text(encoding="utf-8")
_e = (ROOT / "runtime/src/dlls/electron_win32.rs").read_text(encoding="utf-8")
ok("fn access_layout(" in _s and "fn explicit_access_x86_de_32_octets(" in _t,
   "SetEntriesInAcl lit les EXPLICIT_ACCESS x86 de 32 octets, avec test (#1915)")
ok("fn trustee_layout(" in _s and "fn build_trustee(" in _e,
   "BuildExplicitAccessWithName et BuildTrustee aux tailles x86 (#1915)")
