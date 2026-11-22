# StartServiceCtrlDispatcher x86 : table en cases de 4 octets, ServiceMain et gestionnaire par passerelle 64 → 32 (#1971).
_s = (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text(encoding="utf-8")
_w = (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8")
ok("fn argv_x86_du_service_en_cases_de_4_octets(" in _s
   and '"RegisterServiceCtrlHandlerExW"' in _w,
   "service x86 : SERVICE_TABLE_ENTRY 4+4, ServiceMain et HandlerEx appelés en x86 (#1971)")
