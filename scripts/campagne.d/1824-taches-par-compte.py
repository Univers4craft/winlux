# Tâches planifiées d'un compte : rangées dans son dossier, lancées sous ce compte, jamais en root (#1824).
_src = (ROOT / "runtime/src/tasks.rs").read_text(encoding="utf-8")
_ts = (ROOT / "runtime/src/dlls/taskschd.rs").read_text(encoding="utf-8")
ok('USER_TASKS_REL: &str = ".local/share/winlux/tasks"' in _src, "tâches d'un compte dans ~/.local/share/winlux/tasks (#1824)")
# Noms : as_account et libc::setuid, test tache_de_compte_rangee_chez_lui_et_lue_par_le_service (#2711).
ok("fn as_account(" in _src and "libc::setuid" in _src
   and "fn tache_de_compte_rangee_chez_lui_et_lue_par_le_service" in _src, "tâche d'un compte lancée sous ce compte (#1824)")
ok('eprintln!("winlux[taskschd] {}"' in _ts, "trace du planificateur recopiée par msiexec (#1825)")
# Nom : libc::chdir dans as_account (#2711).
_compte = _src[_src.index("fn as_account("):]
ok("libc::chdir" in _compte[:_compte.index("\n}\n")], "dossier de travail d'une tâche de compte pris après setuid (#1824)")
