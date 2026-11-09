# SetProp/GetProp gardent leurs données, atome et nom confondus : la VCL y
# retrouve ses contrôles pour leur refléter WM_NOTIFY et WM_DRAWITEM (« New
# Site » sans texte #1249, liste « File protocol » vide #1250).
ok("fn set_window_prop" in (ROOT / "runtime/src/dlls/gui.rs").read_text(encoding="utf-8")
   and "fn proprietes_de_fenetre_gardees" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
   and "vcl_props()" in (ROOT / "testc/wow32/dlg32.c").read_text(encoding="utf-8"),
   "Propriétés de fenêtre gardées, FindControl de la VCL (#1249)")
