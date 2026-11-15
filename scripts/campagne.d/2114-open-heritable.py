# #2114 : OpenEvent, OpenMutex et OpenSemaphore acceptent bInheritHandle=TRUE (comportement couvert par un test unitaire).
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("fn ouverture_heritable_des_objets_nommes" in k, "Open{Event,Mutex,Semaphore} avec bInheritHandle rendent un handle héritable (#2114)")
