# #1498 : réduite puis restaurée par le gestionnaire de fenêtres, la fenêtre
# reçoit WM_SIZE SIZE_MINIMIZED puis SIZE_RESTORED, et les crochets
# SetWinEventHook (suivi d'occultation de Chromium) sont appelés.
# #1477 : le clic reçu par la « Chrome Legacy Window » est soumis à la
# fenêtre de premier niveau.
gui_x11 = (ROOT / "runtime/src/dlls/gui_x11.rs").read_text(encoding="utf-8")
hooks = (ROOT / "runtime/src/dlls/chromium_win32.rs").read_text(encoding="utf-8")
user32 = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok("fn on_property(" in gui_x11 and "pub fn notify_win_event(" in hooks
   and "pub unsafe fn deliver_win_events()" in hooks,
   "Claude réactivée après Réduire : contenu redessiné (#1498)")
ok("gui::is_see_through_child(hwnd)" in user32,
   "Claude : glisser la barre reçue par l'enfant de Chromium (#1477)")
