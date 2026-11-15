# #2576 : ITaskSettings::get_IdleSettings rend un IIdleSettings (Google Update
# abandonne la création de sa tâche planifiée sans lui), aussi pour le x86.
t = (ROOT / "runtime/src/dlls/taskschd.rs").read_text(encoding="utf-8")
w = (ROOT / "runtime/src/wow32/com_taskschd.rs").read_text(encoding="utf-8")
ok("fn reglages_d_inactivite_de_la_definition" in t
   and 'name: "IIdleSettings"' in w,
   "Planificateur de tâches : IIdleSettings rendu par ITaskSettings, en x64 et en x86 (#2576)")
