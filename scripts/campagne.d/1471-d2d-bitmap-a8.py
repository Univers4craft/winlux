# #1471 : bitmaps Direct2D A8 (un octet d'alpha par pixel).
src = (runtime_dlls / "d2d1.rs").read_text(encoding="utf-8")
ok("alpha_only" in src and "DXGI_FORMAT_A8_UNORM" in src,
   "Direct2D : bitmaps A8 lues octet par octet (#1471)")
