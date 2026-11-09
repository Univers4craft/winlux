# #1400 : groupes de géométries, géométries transformées, Simplify et Stream.
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("fn create_geometry_group(" in src and "fn path_stream(" in src,
   "Direct2D : groupes, transformations, Simplify et Stream (#1400)")
