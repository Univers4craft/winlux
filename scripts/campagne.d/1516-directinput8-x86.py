# DirectInput8 et ses périphériques ont des tables x86 : pygame 32 bits reçoit IDirectInput8W (#1516).
ok("vtable_sig(\"IDirectInput8\"" in (ROOT / "runtime/src/dlls/dinput8.rs").read_text(encoding="utf-8")
   and "\"IDirectInputDevice8\"" in (ROOT / "runtime/src/dlls/dinput8.rs").read_text(encoding="utf-8"),
   "tables x86 de IDirectInput8 et IDirectInputDevice8 (#1516)")
