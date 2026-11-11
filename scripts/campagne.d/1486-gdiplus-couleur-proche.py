# #1486 : GdipGetNearestColor (couleurs des menus WinForms, Paint.NET).
src = (runtime_dlls / "gdiplus.rs").read_text(encoding="utf-8")
ok('"GdipGetNearestColor" => fn get_nearest_color' in src,
   "GDI+ : GdipGetNearestColor (#1486)")
