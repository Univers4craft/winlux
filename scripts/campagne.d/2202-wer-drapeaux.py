# #2202 : WerGetFlags et WerSetFlags rendent S_OK (werapi.h) ; Teams
# s'arrêtait sur leur E_NOTIMPL dans restart_manager_win.cpp.
src = (ROOT / "runtime" / "src" / "dlls" / "kernel32_misc.rs").read_text(encoding="utf-8")
corps = src[src.index("fn wer_get_flags"):src.index("fn is_console(")]
ok("0x80004001" not in corps and "WER_FLAGS.store(flags" in corps,
   "kernel32 : WerGetFlags et WerSetFlags rendent S_OK et mémorisent les drapeaux (#2202)")
