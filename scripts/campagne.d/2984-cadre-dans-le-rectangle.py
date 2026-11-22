# Le rectangle Win32 d'une fenêtre de premier niveau comprend le cadre du
# gestionnaire de fenêtres ; la zone client en est l'intérieur (#2984, #2983).
g = (ROOT / "runtime" / "src" / "dlls" / "gui.rs").read_text(encoding="utf-8")
ok("fn cadre_du_gestionnaire_dans_le_rectangle_de_fenetre" in g, "Rectangle de fenêtre cadre compris, zone client à l'intérieur (#2984)")
ok((ROOT / "testexe" / "src" / "bin" / "palette_suite.rs").exists(), "Suite PE de la palette flottante présente (#2984)")
ok((ROOT / "scripts" / "test-palette-cadre.sh").exists(), "Palette sous openbox : cadre réel dans l'écran (#2984)")
u = (ROOT / "runtime" / "src" / "dlls" / "user32.rs").read_text(encoding="utf-8")
ok("fn frame_kind" in u, "AdjustWindowRectEx annonce le cadre réel du gestionnaire (#3016)")
