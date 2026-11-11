# #1609 : GetTarget d'une cible fenêtre ou HDC rend un bitmap qui la rétablit ; BindDC et Resize rétablissent le HDC.
src = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
ok("home_bitmap" in src and "fn leave_bitmap" in src,
   "Direct2D : GetTarget, BindDC et Resize rétablissent la fenêtre ou le HDC (#1609)")
