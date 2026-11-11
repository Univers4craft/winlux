# #1651 : DrawImage avec rectangle source place son coin haut gauche au décalage (bandes de damier de Paint.NET).
src = (runtime_dlls / "d2d1_1.rs").read_text(encoding="utf-8")
ok("(ox - src.l, oy - src.t)" in src,
   "Direct2D : DrawImage place le rectangle source au décalage (#1651)")
