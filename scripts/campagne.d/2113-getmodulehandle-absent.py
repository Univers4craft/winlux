# #2113 : GetModuleHandleA/W d'un module non chargé pose ERROR_MOD_NOT_FOUND (comportement couvert par un test unitaire).
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("fn module_absent_pose_mod_not_found" in k, "GetModuleHandle d'un module absent rend 126 (#2113)")
