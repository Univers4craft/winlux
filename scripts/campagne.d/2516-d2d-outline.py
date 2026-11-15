# #2516 : ID2D1Geometry::Outline est implémentée (Nouvelle image de Paint.NET).
d = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("unsafe extern \"win64\" fn geometry_outline(" in d
   and 'unsupported!("Outline")' not in d,
   "ID2D1Geometry::Outline écrit le contour au lieu de E_NOTIMPL (#2516)")
