# #1590 : SetTarget vers un bitmap sur une cible fenêtre ou HDC, puis retour (canevas et panneaux de Paint.NET).
src = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
ok("SetTarget refusé sur une cible fenêtre ou HDC" not in src and "fn end_draw_home" in src,
   "Direct2D : SetTarget vers un bitmap depuis une cible fenêtre ou HDC (#1590)")
