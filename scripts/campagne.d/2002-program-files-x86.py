# Programme 32 bits : Program Files (x86) pour CSIDL et registre (#2002).
_kf = (ROOT / "runtime/src/dlls/knownfolders.rs").read_text(encoding="utf-8")
_reg = (ROOT / "runtime/src/registry/mod.rs").read_text(encoding="utf-8")
ok("0x2b => ProgramFiles(\"Common Files\")" in _kf, "CSIDL_PROGRAM_FILES_COMMON suit l'architecture (#2002)")
ok("0x26 => ProgramFiles(\"\")" in _kf, "CSIDL_PROGRAM_FILES suit l'architecture (#2002)")
ok("F7F1ED05-9F6D-47A2-AAAE-29D317C6F066" in _kf, "FOLDERID_ProgramFilesCommon connu (#2002)")
ok("Wow6432Node\\\\Microsoft\\\\Windows\\\\CurrentVersion" in _reg and "\"CommonFilesDir\"" in _reg, "CommonFilesDir en vue 32 bits (#2002)")
