# setup.exe /download de l'ODT (x86) : imports CRT et kernel32 manquants, 0xC0000139 (#1874).
_crt = (ROOT / "runtime/src/dlls/crt_more.rs").read_text(encoding="utf-8")
_k32 = (ROOT / "runtime/src/dlls/kernel32_ext.rs").read_text(encoding="utf-8")
ok('"__initialize_lconv_for_unsigned_char" => fn' in _crt, "ucrtbase sert __initialize_lconv_for_unsigned_char (#1874)")
ok('"_ldsign" => fn ldsign' in _crt, "ucrtbase sert _ldsign (#1874)")
ok(all(f'"{n}" => fn' in _k32 for n in ("IsSystemResumeAutomatic", "WerRegisterMemoryBlock", "WerUnregisterMemoryBlock")),
   "kernel32 sert IsSystemResumeAutomatic et WerRegister/UnregisterMemoryBlock (#1874)")
