# #1647 : CreateSharedBitmap partage la mémoire du verrou WIC au lieu de la copier une fois (pinceau de Paint.NET).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("struct SharedLock" in src and "fn refresh(&self)" in src,
   "Direct2D : bitmap partagé relu à chaque usage (#1647)")
