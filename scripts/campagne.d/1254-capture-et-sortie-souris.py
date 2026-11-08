# #1254 : WM_CAPTURECHANGED et WM_MOUSELEAVE, sans quoi WPF ignore les clics d'une deuxième fenêtre.
ok("WM_CAPTURECHANGED" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
   and "fn track_mouse_event" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
   and "fn mouse_moved_to" in (runtime_dlls / "gui.rs").read_text(encoding="utf-8"),
   "SetCapture/ReleaseCapture préviennent la fenêtre qui perd la capture, TrackMouseEvent poste WM_MOUSELEAVE (#1254)")
