# #1345 : GetDC et BitBlt valent pour la fenêtre d'un autre processus.
ok("fn paint_foreign" in (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
   and "fn paint_foreign" in (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8")
   and "gui::foreign_window(hwnd).is_none()" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8"),
   "GDI sur la fenêtre d'un autre processus (#1345)")
