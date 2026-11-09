# #1328 : WM_SIZE porte SIZE_MAXIMIZED/SIZE_MINIMIZED, sans quoi le bouton « Restaurer » de WPF ne fait rien.
ok("gui::size_kind(" in (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8")
   and "wm_size_dit_l_etat_de_la_fenetre" in (runtime_dlls / "gui.rs").read_text(encoding="utf-8"),
   "WM_SIZE dit si la fenêtre est agrandie ou réduite (#1328)")
