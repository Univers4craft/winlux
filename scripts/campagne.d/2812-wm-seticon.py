# user32 : DefWindowProc garde l'icône posée par WM_SETICON et la rend par
# WM_GETICON (#2812), vérifié par le test unitaire de gui.rs.
_g = (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
ok("fn set_window_icon(" in _g and "fn wm_seticon_garde_l_icone_et_rend_la_precedente()" in _g,
   "user32 : WM_SETICON gardé par DefWindowProc, rendu par WM_GETICON, test (#2812)")
