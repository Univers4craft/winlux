# __sys_nerr et _sys_errlist sous 4 Go pour un programme x86 (#1437).
ok("fn errlist_low" in (ROOT / "runtime/src/wow32/crt.rs").read_text(encoding="utf-8"),
   "_sys_errlist recopié sous 4 Go en x86 (#1437)")
