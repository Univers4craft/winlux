# #1787 : un processus figé par NtSuspendProcess est relancé si celui qui l'a figé plante.
e = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
c = (runtime_dlls.parent / "crash.rs").read_text(encoding="utf-8")
ok("static SUSPENDED" in e and "fn resume_suspended" in e,
   "NtSuspendProcess retient les processus figés (#1787)")
ok("electron_win32::resume_suspended()" in c,
   "le gestionnaire de plantage relance les processus figés (#1787)")
