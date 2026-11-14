# #1845 : AudioSrv et hidserv existent pour les installeurs qui les arrêtent puis les redémarrent (driverinstalltool d'iCUE).
services_src = (runtime_dlls / "advapi32_services.rs").read_text(encoding="utf-8")
ok('("audiosrv", "Audio")' in services_src
   and '("hidserv",' in services_src
   and "fn services_systeme_arretes_puis_redemarres_par_un_installeur" in services_src,
   "Services système AudioSrv et hidserv présents (#1845)")
