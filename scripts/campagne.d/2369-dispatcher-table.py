# StartServiceCtrlDispatcher refuse une entrée sans ServiceMain (ERROR_INVALID_DATA) et trace chaque entrée (#2369).
_s = (runtime_dlls / "advapi32_services.rs").read_text(encoding="utf-8")
ok("fn check_service_table" in _s and "fn dispatcher_refuse_une_entree_sans_service_main" in _s,
   "StartServiceCtrlDispatcher : table vérifiée, entrée sans ServiceMain refusée (#2369)")
