# #2454 : ID2D1Geometry::ComputePointAtLength (Paint.NET, Fichier > Nouveau).
d = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("geometry_point_at_length as *const ()" in d
   and 'unsupported!("ComputePointAtLength")' not in d,
   "ComputePointAtLength est servie (#2454)")
