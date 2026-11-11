# FDICopy annonce le cabinet avec fdintCABINET_INFO = 0, pas 1 (fdintPARTIAL_FILE) : extraction de l'ODT (#1856).
_src = (ROOT / "runtime/src/dlls/cabinet_lz.rs").read_text(encoding="utf-8")
ok("const FDINT_CABINET_INFO: i32 = 0;" in _src, "FDICopy envoie fdintCABINET_INFO = 0 (#1856)")
