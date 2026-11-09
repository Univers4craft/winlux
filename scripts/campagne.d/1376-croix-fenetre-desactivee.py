# #1376 : la croix d'une fenêtre désactivée par une boîte modale ne la ferme pas.
ok("message == WM_CLOSE || message == WM_SETFOCUS) && input_disabled(hwnd)" in (runtime_dlls / "gui.rs").read_text(encoding="utf-8"),
   "Fenêtre désactivée par une boîte modale : croix refusée (#1376)")
