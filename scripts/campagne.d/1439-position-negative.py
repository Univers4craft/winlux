# SetFilePointer(Ex) vers une position négative : ERROR_NEGATIVE_SEEK (#1439).
ok("ERROR_NEGATIVE_SEEK" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "WinHttpDetectAutoProxyConfigUrl" in (ROOT / "runtime/src/dlls/winhttp.rs").read_text(encoding="utf-8"),
   "manifeste Steam x86 : sonde de fin de fichier et WPAD (#1439)")
