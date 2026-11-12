# #1849 : CreateSharedBitmap accepte un verrou WIC 8bppAlpha (masque de
# sélection de Paint.NET, plantage à Ctrl+A).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("let alpha8 = fmt == FMT_ALPHA8;" in src and "!alpha8 && wide.is_none()" in src,
   "Direct2D : CreateSharedBitmap d'un verrou WIC 8bppAlpha (#1849)")
