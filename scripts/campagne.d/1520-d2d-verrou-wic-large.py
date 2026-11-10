# #1520 : CreateSharedBitmap accepte un verrou WIC en format large (64bppPRGBAHalf, Paint.NET).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("fn wide_to_pbgra" in src,
   "Direct2D : bitmap partagé depuis un verrou WIC large (#1520)")
