# OpenService trace le nom demandé et l'erreur rendue (driverinstalltool d'iCUE, #1699).
svc = (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text(encoding="utf-8")
ok('"OpenService({name}) : service absent, erreur 1060"' in svc,
   "OpenService : nom et erreur 1060 tracés sous WINLUX_TRACE (#1699)")
