# #1399 : CreateSharedBitmap partage un IWICBitmapLock en ID2D1Bitmap.
ok("fn target_shared_bitmap(" in (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8"),
   "Direct2D : CreateSharedBitmap (#1399)")
