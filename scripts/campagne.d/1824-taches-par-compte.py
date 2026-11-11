# Tâches planifiées d'un compte : rangées dans son dossier, lancées sous ce compte, jamais en root (#1824).
_src = (ROOT / "runtime/src/tasks.rs").read_text(encoding="utf-8")
_ts = (ROOT / "runtime/src/dlls/taskschd.rs").read_text(encoding="utf-8")
ok('USER_TASKS_REL: &str = ".local/share/winlux/tasks"' in _src, "tâches d'un compte dans ~/.local/share/winlux/tasks (#1824)")
ok("fn as_account(" in _src and "libc::setuid(uid)" in _src, "tâche d'un compte lancée sous ce compte (#1824)")
ok('eprintln!("winlux[taskschd] {}"' in _ts, "trace du planificateur recopiée par msiexec (#1825)")
