# #1605 : un installeur crée son appareil racine, puis UpdateDriverForPlugAndPlayDevices le trouve.
drv = (runtime_dlls / "drivers.rs").read_text(encoding="utf-8")
dev = (runtime_dlls / "devnodes.rs").read_text(encoding="utf-8")
ok(all(f'"{n}"' in drv for n in ("SetupDiCreateDeviceInfoW", "SetupDiSetDeviceRegistryPropertyW",
                                  "SetupDiCallClassInstaller", "CMP_WaitNoPendingInstallEvents",
                                  "CM_Request_Device_EjectW")),
   "pilotes : création d'un appareil racine et CMP_WaitNoPendingInstallEvents servis (#1605)")
ok("fn register_virtual" in dev and "registered_virtual()" in dev,
   "pilotes : l'appareil racine inscrit est énuméré avec les appareils réels (#1605)")
