# Bac à sable de Chromium (#767) : chaque enfant désigne le gestionnaire de crashpad, et un thread refusé est journalisé avec ses limites.
ok("fn adopt_crash_handler" in (runtime_dlls / "kernel32.rs").read_text()
   and "adopt_crash_handler()" in (ROOT / "runtime/src/process.rs").read_text(),
   "crashpad : les enfants de Chromium le laissent lire leur mémoire (#767)")
ok("fn thread_limits_report" in (ROOT / "runtime/src/crash.rs").read_text(),
   "thread refusé : limites en cause journalisées (#767)")
