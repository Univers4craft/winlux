# #1337 : un enfant WS_EX_TRANSPARENT devient une fenêtre X InputOnly.
ok("pub fn is_see_through_child" in (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
   and "WindowClass::INPUT_ONLY" in (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8"),
   "Enfant transparent sans masque (#1337)")
