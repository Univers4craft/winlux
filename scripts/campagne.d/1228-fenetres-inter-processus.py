# #1228 : un HWND vaut pour tous les processus du compte (ANGLE dans le processus GPU).
ok("fn foreign_window" in (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
   and "gui::foreign_window(hwnd).is_some()" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
   and "is_window_voit_la_fenetre_d_un_autre_processus" in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8"),
   "IsWindow, GetClientRect et GetWindowThreadProcessId voient la fenêtre d'un autre processus (#1228)")
ok("foreign_window(hwnd).map(|f| f.xid)" in (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
   and "foreign_client_rect(hwnd)" in (runtime_dlls / "dxgi.rs").read_text(encoding="utf-8")
   and "direct3d_trouve_la_fenetre_x11_d_un_autre_processus" in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8"),
   "Direct3D présente dans la fenêtre X11 d'un autre processus (#1228)")
