# #3054 : une touche frappée avec Alt tenu (et Alt seul, F10) arrive en
# WM_SYSKEYDOWN / WM_SYSKEYUP, TranslateMessage en tire WM_SYSCHAR, et
# DefWindowProc change Alt+F4 en WM_SYSCOMMAND SC_CLOSE puis WM_CLOSE.
# Le comportement est gardé par les tests unitaires nommés.
_x11_3054 = (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8")
_gui_3054 = (runtime_dlls / "gui.rs").read_text(encoding="utf-8")
print("      tests : frappes_systeme_comme_le_pilote_clavier_de_windows, alt_lettre_donne_wm_syschar_et_alt_f4_ferme_la_fenetre")
ok("fn frappes_systeme_comme_le_pilote_clavier_de_windows" in _x11_3054,
   "clavier : Alt+touche, Alt et F10 arrivent en frappes système, Ctrl+Alt reste ordinaire (#3054)")
ok("fn alt_lettre_donne_wm_syschar_et_alt_f4_ferme_la_fenetre" in _gui_3054,
   "clavier : Alt+lettre donne WM_SYSCHAR et Alt+F4 reçu par la fenêtre la ferme (#3054)")
