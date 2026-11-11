# Un clic active une fenêtre surgissante hors gestionnaire (WS_POPUP + WS_EX_TOOLWINDOW) : la boîte « Erreur du service Steam » réagit à ses boutons (#1804).
_g = (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
_u = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok("pub fn click_activation_target(hwnd: usize)" in _g and "fn un_clic_active_une_fenetre_surgissante_hors_gestionnaire" in _g,
   "fenêtre à activer au clic choisie, avec test (#1804)")
ok("unsafe fn click_activate(m: &Msg)" in _u and _u.count("click_activate(&m);") == 2 and "WA_CLICKACTIVE" in _u,
   "GetMessage et PeekMessage activent au clic (WM_MOUSEACTIVATE, WA_CLICKACTIVE) (#1804)")
ok("fn un_menu_integre_ne_s_active_pas_au_clic" in _g and "0x0021 => {" in _g,
   "menus intégrés exclus, WM_MOUSEACTIVATE par défaut MA_ACTIVATE (#1804)")
