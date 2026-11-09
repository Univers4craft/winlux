# ExitProcess termine par _exit, sans les gestionnaires atexit de la glibc (#1440).
ok("libc::_exit(code)" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "(#1440)" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8"),
   "fin de processus x86 sans code invité après ExitProcess (#1440)")
