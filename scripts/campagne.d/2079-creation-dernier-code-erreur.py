# Création réussie d'un objet de synchronisation : GetLastError à 0, ou 183 (#2079).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
ok("fn create_sync(obj: SyncHandle" in _k and _k.count("create_sync(") >= 5, "Create{Event,Mutex,Semaphore,WaitableTimer} remettent GetLastError à 0 (#2079)")
ok("fn creation_reussie_remet_last_error_a_zero" in _k, "test du code d'erreur après création (#2079)")
ok("fn trace_long_wait(" in _k, "trace des longues attentes sur handles anonymes (#2079)")
