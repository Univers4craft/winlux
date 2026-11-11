# SECURITY_DESCRIPTOR absolu x86 de 20 octets : InitializeSecurityDescriptor et SetSecurityDescriptorDacl/Owner/Group n'écrivent plus aux places x64 (SteamService.exe, #1881).
_s = (ROOT / "runtime/src/dlls/security.rs").read_text(encoding="utf-8")
_t = (ROOT / "runtime/src/dlls/security/tests.rs").read_text(encoding="utf-8")
ok("fn absolute_sd_size(ptr: usize) -> usize" in _s and "ABSOLUTE_SD_SIZE" not in _s
   and "sd.add(32) as *mut *mut u8" not in _s,
   "descripteur absolu à la taille du programme (20 octets en x86) (#1881)")
ok(_s.count("write_abs_slot(sd, ") == 2 and "write_abs_slot(\n        sd,\n        16," in _s and "fn descripteur_absolu_x86_tient_dans_20_octets" in _t,
   "DACL, propriétaire et groupe aux places x86, avec test (#1881)")
