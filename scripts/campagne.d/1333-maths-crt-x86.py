# Aides mathématiques du CRT x86 (_CIfmod, _CIatan2, _libm_sse2_*_precise,
# _except1) en code x86 dédié ; pygame win32 les importe (#1333).
ok("fn math_code" in (ROOT / "runtime/src/wow32/crt.rs").read_text(encoding="utf-8")
   and "_libm_sse2_pow_precise" in (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8"),
   "Aides mathématiques x87/SSE2 du CRT x86 (#1333)")
