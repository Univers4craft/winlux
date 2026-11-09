x11 = (ROOT / "runtime/src/dlls/gui_x11.rs").read_text(encoding="utf-8")
ok("fn stale_child_focus(" in x11 and "if stale_child_focus(child, self.asked_focus, e.event)" in x11,
   "FocusIn périmé d'un contrôle sans effet sur le focus Win32 (#1286)")
