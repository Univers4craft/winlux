# #1749 : ID2D1Bitmap::CopyFromBitmap et CopyFromRenderTarget (Inverser les
# couleurs de Paint.NET plantait sur E_NOTIMPL).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("bitmap_copy_from_bitmap as *const ()" in src and "bitmap_copy_from_target as *const ()" in src,
   "Direct2D : CopyFromBitmap et CopyFromRenderTarget (#1749)")
