# #1965 : Task, async/await (constructeurs de méthodes async), annulation,
# SemaphoreSlim, WaitHandle, Timer et RuntimeHelpers.InitializeArray dans
# notre mscorlib ; témoin compilé en version optimisée.
ok("public struct AsyncTaskMethodBuilder<TResult>" in (ROOT / "bcl" / "mscorlib" / "Taches.cs").read_text(encoding="utf-8")
   and "fn initialize_array" in (runtime_dlls.parent / "clr" / "tableaux.rs").read_text(encoding="utf-8")
   and "Option de compilation : -optimize+." in (ROOT / "bcl" / "temoins" / "Taches.cs").read_text(encoding="utf-8")
   and "Option de compilation" in (ROOT / "scripts" / "build-bcl.sh").read_text(encoding="utf-8"),
   "CLR : Task et async/await dans notre mscorlib, témoin Taches (#1965)")
