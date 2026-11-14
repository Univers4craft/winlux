# __fastfail (int 0x29) rapporté en STATUS_STACK_BUFFER_OVERRUN, pas en violation d'accès (#2287).
_c = (ROOT / "runtime/src/crash.rs").read_text(encoding="utf-8")
_s = (ROOT / "runtime/src/dlls/seh_hw.rs").read_text(encoding="utf-8")
ok("pub(crate) unsafe fn is_fastfail" in _s, "__fastfail reconnu par seh_hw::is_fastfail (#2287)")
ok('b"STATUS_STACK_BUFFER_OVERRUN"' in _c and "0xC000_0409" in _c, "__fastfail : STATUS_STACK_BUFFER_OVERRUN 0xC0000409 dans le rapport de plantage (#2287)")
ok("fn fastfail_rapporte_stack_buffer_overrun" in _c, "__fastfail : vrai int 0x29 dans un processus relancé, prouvé par un test (#2287)")
