# #2577 : taskschd.dll est un module du processus (GetModuleHandleExW de
# Google Update l'épingle) et son DllGetClassObject sert ITaskService.
d = (ROOT / "runtime/src/dlls/taskschd_dll.rs").read_text(encoding="utf-8")
m = (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8")
ok('name: "taskschd"' in m
   and "fn taskschd_dll_epinglable_et_fabrique" in d,
   "taskschd.dll intégrée : module épinglable, fabrique d'ITaskService (#2577)")
