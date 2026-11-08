# UpdateLayeredWindow avec alpha par pixel ou couleur clé : forme X, sans cadre openbox.
ok("fn layered_shape(" in (runtime_dlls / "win32_basics.rs").read_text(encoding="utf-8")
   and "fn set_shape(" in (runtime_dlls / "gui_x11.rs").read_text(encoding="utf-8")
   and "fn layered_without_caption(" in (runtime_dlls / "user32.rs").read_text(encoding="utf-8")
   and "forme_d_une_fenetre_en_couches" in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8"),
   "Fenêtre en couches (Flow Launcher) : sans cadre openbox, fond transparent découpé, plus de noir (#1220)")
