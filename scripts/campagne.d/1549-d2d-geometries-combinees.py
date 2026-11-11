# #1549 : ID2D1Geometry::CombineWithGeometry, quatre modes (boutons de couleur de Paint.NET).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
comb = (runtime_dlls / "d2d_combine.rs").read_text(encoding="utf-8")
ok("geometry_combine as *const ()" in src and 'unsupported!("CombineWithGeometry")' not in src
   and all(m in comb for m in ("Union", "Intersect", "Xor", "Exclude")),
   "Direct2D : CombineWithGeometry (#1549)")
