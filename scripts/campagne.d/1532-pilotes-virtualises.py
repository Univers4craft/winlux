# #1532 : un installeur de pilote réussit, le .sys n'est jamais chargé, la personne est prévenue.
drv = (runtime_dlls / "drivers.rs").read_text(encoding="utf-8")
svc = (runtime_dlls / "advapi32_services.rs").read_text(encoding="utf-8")
ok("DiInstallDriverW" in drv and "UpdateDriverForPlugAndPlayDevicesW" in drv and "SetupCopyOEMInfW" in drv,
   "pilotes : SetupCopyOEMInf, DiInstallDriver et UpdateDriverForPlugAndPlayDevices servis (#1532)")
ok("fn start_driver" in svc and "ERROR_DRIVER_BLOCKED" in drv,
   "pilotes : un service pilote n'est jamais exécuté, appareil virtuel ou ERROR_DRIVER_BLOCKED (#1532)")
ok(all(t in drv for t in ('"en"', '"de"', '"es"')),
   "pilotes : notification « remplacé » ou « non remplacé » en 4 langues (#1532)")
