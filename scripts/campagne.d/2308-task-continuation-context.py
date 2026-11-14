# msvcp140 : constructeur de task_continuation_context, mur d'iCUE après #2307 (#2308).
_m = (ROOT / "runtime/src/dlls/msvcp140.rs").read_text()
ok('"??0task_continuation_context@Concurrency@@AEAA@XZ"' in _m,
   "msvcp140 exporte le constructeur de task_continuation_context (#2308)")
