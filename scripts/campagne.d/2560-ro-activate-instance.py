# #2560 : RoActivateInstance active la classe demandée (elle jetait son
# argument : toujours REGDB_E_CLASSNOTREG, classe vide au journal).
e = (ROOT / "runtime/src/dlls/electron_win32.rs").read_text(encoding="utf-8")
ok("fn RoActivateInstance(class: usize" in e and "IID_ACTIVATION_FACTORY" in e and "#2560" in e,
   "RoActivateInstance : la classe demandée est activée (#2560)")
