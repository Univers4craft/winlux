# WINLUX_TRACE : objets nommés, signalements, attentes et contenu des vues (#2072).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
_m = (ROOT / "runtime/src/dlls/mmap.rs").read_text(encoding="utf-8")
ok("\"Create{kind}({name}) = 0x{h:x}, clé {:?}, {} (erreur {err})\"" in _k, "trace des créations d'objets nommés (#2072)")
ok("\"Open{}({name}) = 0x{h:x} (erreur {})\"" in _k, "trace des ouvertures d'objets nommés (#2072)")
ok("SetEvent({n}, 0x{h:x})" in _k and "WaitForSingleObject({n}, 0x{h:x}, {timeout} ms)" in _k, "trace des signalements et attentes nommés (#2072)")
ok("fn head_hex(" in _m and "UnmapViewOfFile({n}" in _m, "trace des premiers octets des vues nommées (#2072)")
