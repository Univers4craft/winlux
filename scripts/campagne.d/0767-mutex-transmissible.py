# Bac à sable de Chromium (#767) : DuplicateHandle vers un autre processus transmet un mutex ou un sémaphore sans nom.
ok('"x" =>' in (runtime_dlls / "inherit.rs").read_text()
   and "fn share_sync" in (runtime_dlls / "kernel32.rs").read_text(),
   "DuplicateHandle : mutex et sémaphores sans nom transmissibles à un autre processus (#767)")
