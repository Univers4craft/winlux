# msvcp140 : appui de <ppltasks.h> documenté par la MS STL (journaux, contexte, GetCurrentThreadId) (#2308).
_m = (ROOT / "runtime/src/dlls/mod.rs").read_text()
ok("m.extend(msvcp140_ppl::exports());" in _m,
   "msvcp140 sert l'appui des tâches PPL (#2308)")
_p = (ROOT / "runtime/src/dlls/msvcp140_ppl.rs").read_text()
ok(_p.count("@_TaskEventLogger@details@Concurrency@@") == 6
   and "?_CallInContext@_ContextCallback@details@Concurrency@@QEBAXV?$function@$$A6AXXZ@std@@_N@Z" in _p,
   "msvcp140 : journaux _TaskEventLogger et _ContextCallback::_CallInContext exportés (#2308)")
ok("?_Schedule_chore@details@Concurrency@@YAHPEAU_Threadpool_chore@12@@Z" in _p
   and "?ReportUnhandledError@_ExceptionHolder@details@Concurrency@@AEAAXXZ" in _p,
   "msvcp140 : _Schedule_chore relevé et ReportUnhandledError servis (#2308)")
