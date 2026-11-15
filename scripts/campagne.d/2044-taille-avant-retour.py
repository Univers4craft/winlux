# #2044 : DefWindowProc(WM_WINDOWPOSCHANGED) envoie WM_SIZE et WM_MOVE avant le retour de SetWindowPos (Qt, iCUE 4.18 en 160x160).
user32_src = (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
gui_src = (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
ok("fn taille_annoncee_avant_le_retour_de_setwindowpos" in user32_src
   and "fn window_pos_changed_default" in user32_src
   and "crate::dlls::user32::window_pos_changed_default(hwnd)" in gui_src,
   "SetWindowPos : WM_SIZE envoyé par DefWindowProc(WM_WINDOWPOSCHANGED) (#2044)")
