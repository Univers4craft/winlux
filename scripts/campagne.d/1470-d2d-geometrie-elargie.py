# #1470 : Widen et mesures du trait des géométries Direct2D (Paint.NET).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("fn geometry_widen(" in src and "fn geometry_widened_bounds(" in src
   and 'unsupported!("Widen")' not in src,
   "Direct2D : Widen, GetWidenedBounds, StrokeContainsPoint, ComputeLength (#1470)")
