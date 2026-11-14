# Le Planificateur du bureau enregistre dans le magasin du compte, pas dans /etc/winlux/tasks (#2230).
_ui = (ROOT / "testexe/src/shell/taskschd.rs").read_text(encoding="utf-8")
_rt = (ROOT / "runtime/src/dlls/winlux_tasks.rs").read_text(encoding="utf-8")
ok("WinluxTaskSave(" in _ui and "Z:\\\\etc\\\\winlux\\\\tasks" not in _ui, "le Planificateur passe par WinluxTaskSave, plus par /etc/winlux/tasks (#2230)")
ok("tasks::save_text" in _rt and "tasks::entries()" in _rt, "WinluxTaskList et WinluxTaskSave suivent le magasin du compte (#2230)")
ok((ROOT / "testexe/src/bin/taches_compte.rs").exists(), "test taches_compte.exe : RegisterTaskDefinition sans mot de passe (#2230)")
