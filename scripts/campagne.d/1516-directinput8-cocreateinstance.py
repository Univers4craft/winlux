# CoCreateInstance connaît CLSID_DirectInput8 : SDL 2 (pygame 32 bits) ouvre sa fenêtre (#1516).
ok("fn classe_com_directinput8" in (ROOT / "runtime/src/dlls/dinput8.rs").read_text(encoding="utf-8")
   and "dinput8::create_builtin" in (ROOT / "runtime/src/dlls/ole32.rs").read_text(encoding="utf-8"),
   "classe COM DirectInput8 servie par CoCreateInstance (#1516)")
