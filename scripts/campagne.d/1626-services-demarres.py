# MSI : les services à démarrer à l'installation (ServiceControl) sont lancés en fin d'installation (#1626).
act = (ROOT / "runtime/src/msi/actions.rs").read_text(encoding="utf-8")
svc = (runtime_dlls / "advapi32_services.rs").read_text(encoding="utf-8")
ok("pub fn start_services" in act and 'Some("start") if args.len() > 1' in svc,
   "MSI : services démarrés en fin d'installation, détachés de l'installeur (#1626)")
