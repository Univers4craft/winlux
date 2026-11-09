# RegNotifyChangeKeyValue (#1300) : un seul thread de surveillance du registre, pas un thread par appel.
ok("fn watch_key" in (runtime_dlls / "advapi32.rs").read_text()
   and "winlux-registre" in (runtime_dlls / "advapi32.rs").read_text(),
   "registre : un seul thread surveille les abonnements RegNotifyChangeKeyValue (#1300)")
