# Services : un service lancé ailleurs est retrouvé par son processus, ni « arrêté » à tort ni relancé en double (#1675, #1676).
svc = (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text(encoding="utf-8")
ok("fn service_process" in svc and "WINLUX_SERVICE_NAME={name}" in svc,
   "Services : service retrouvé par WINLUX_SERVICE_NAME dans /proc (#1675, #1676)")
