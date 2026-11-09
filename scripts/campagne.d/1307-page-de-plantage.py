# #1307, #1308 : message « a planté », stop code APPLICATION_FAILURE, [rsp] affiché.
ok("le programme a planté" in (ROOT / "runtime" / "src" / "crash.rs").read_text(encoding="utf-8")
   and "IRQL_NOT_LESS_OR_EQUAL" not in (ROOT / "runtime" / "src" / "crash.rs").read_text(encoding="utf-8")
   and "winlux: [rsp]=0x" in (ROOT / "runtime" / "src" / "crash.rs").read_text(encoding="utf-8"),
   "Page de plantage d'une application (#1307, #1308)")
