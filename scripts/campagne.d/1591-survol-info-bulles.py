# #1591 TrackMouseEvent(TME_HOVER) poste WM_MOUSEHOVER (info-bulles WinForms de Paint.NET).
u = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
g = (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
ok("gui::track_mouse_hover(hwnd, ms)" in u and "WM_MOUSEHOVER, 0, lparam" in g,
   "TrackMouseEvent(TME_HOVER) fait poster WM_MOUSEHOVER (#1591)")
